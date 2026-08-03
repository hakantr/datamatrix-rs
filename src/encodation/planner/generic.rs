use alloc::{vec, vec::Vec};
use core::fmt::{Debug, Error, Formatter};

use flagset::FlagSet;

use crate::{encodation::encodation_type::EncodationType, symbol_size::SymbolList};

use super::{
    ContextInformation, Plan, StepResult, ascii::AsciiPlan, base256::Base256Plan, c40::C40Plan,
    edifact::EdifactPlan, frac::Frac, text::TextPlan, x12::X12Plan,
};

#[cfg(test)]
use pretty_assertions::assert_eq;

/// İç plan hangi mode'da olursa olsun `$body` bloğunu çalıştırır.
///
/// İç plan `$pl` değişkenine bağlanır. Mutable borrow için `mut` biçimi kullanılır.
macro_rules! dispatch {
    ($self:expr, $pl:ident => $body:expr) => {
        match &$self.plan {
            PlanImpl::Ascii($pl) => $body,
            PlanImpl::C40($pl) => $body,
            PlanImpl::Text($pl) => $body,
            PlanImpl::X12($pl) => $body,
            PlanImpl::Edifact($pl) => $body,
            PlanImpl::Base256($pl) => $body,
        }
    };
    (mut $self:expr, $pl:ident => $body:expr) => {
        match &mut $self.plan {
            PlanImpl::Ascii($pl) => $body,
            PlanImpl::C40($pl) => $body,
            PlanImpl::Text($pl) => $body,
            PlanImpl::X12($pl) => $body,
            PlanImpl::Edifact($pl) => $body,
            PlanImpl::Base256($pl) => $body,
        }
    };
}

#[derive(Clone, PartialEq)]
pub(super) struct GenericPlan<'a> {
    extra: Frac,
    pub(super) switches: Vec<(usize, EncodationType)>,
    plan: PlanImpl<'a>,
}

impl<'a> Debug for GenericPlan<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        dispatch!(self, pl => f.write_fmt(format_args!(
            "{:?}(start {:?}, {:?}, cw {:?}, rest {:?}, switches = {:?})",
            self.current(),
            self.start_mode(),
            pl.cost() + self.extra,
            pl.context().written,
            pl.context().rest().len(),
            self.switches,
        )))
    }
}

#[derive(Debug, Clone, PartialEq)]
enum PlanImpl<'a> {
    Ascii(AsciiPlan<Context<'a>>),
    C40(C40Plan<Context<'a>>),
    Text(TextPlan<Context<'a>>),
    X12(X12Plan<Context<'a>>),
    Edifact(EdifactPlan<Context<'a>>),
    Base256(Base256Plan<Context<'a>>),
}

impl<'a> GenericPlan<'a> {
    /// Verilen encodation türüyle başlayan bir örnek oluşturur.
    pub(super) fn for_mode(
        mode: EncodationType,
        data: &'a [u8],
        written: usize,
        symbol_list: &'a SymbolList,
    ) -> Self {
        let mut ctx = Context::new(data, symbol_list);
        ctx.write(written);
        let plan = match mode {
            EncodationType::Ascii => PlanImpl::Ascii(AsciiPlan::new(ctx)),
            EncodationType::C40 => PlanImpl::C40(C40Plan::new(ctx)),
            EncodationType::Text => PlanImpl::Text(TextPlan::new(ctx)),
            EncodationType::Edifact => PlanImpl::Edifact(EdifactPlan::new(ctx)),
            EncodationType::X12 => PlanImpl::X12(X12Plan::new(ctx)),
            EncodationType::Base256 => PlanImpl::Base256(Base256Plan::new(ctx)),
        };
        Self {
            extra: 0.into(),
            switches: vec![(data.len(), mode)],
            plan,
        }
    }

    /// Planın başladığı mode'u döndürür.
    pub(super) fn start_mode(&self) -> EncodationType {
        self.switches
            .first()
            .map(|switch| switch.1)
            .unwrap_or_else(|| self.current())
    }

    /// Geçerli encodation türünü (mode) döndürür.
    pub(super) fn current(&self) -> EncodationType {
        match self.plan {
            PlanImpl::Ascii(_) => EncodationType::Ascii,
            PlanImpl::C40(_) => EncodationType::C40,
            PlanImpl::Text(_) => EncodationType::Text,
            PlanImpl::X12(_) => EncodationType::X12,
            PlanImpl::Edifact(_) => EncodationType::Edifact,
            PlanImpl::Base256(_) => EncodationType::Base256,
        }
    }

    pub(super) fn add_switches(
        self,
        list: &mut Vec<Self>,
        rest_len: usize,
        as_start: bool,
        enabled_modes: FlagSet<EncodationType>,
    ) {
        let ascii_cost = if let Some(cost) = self.mode_switch_cost() {
            cost
        } else {
            return;
        };
        let ctx = self.write_unlatch();

        macro_rules! add_switch {
            ($plan:ident, $enum:ident, $cost_extra:expr) => {
                let switches = if as_start {
                    vec![(rest_len, EncodationType::$enum)]
                } else {
                    let mut switches = self.switches.clone();
                    switches.push((rest_len, EncodationType::$enum));
                    switches
                };
                let mut ctx = ctx.clone();
                ctx.write($cost_extra); // LATCH byte'ı
                let mut new = $plan::new(ctx);
                if let Some(_) = new.step() {
                    list.push(Self {
                        extra: ascii_cost + $cost_extra,
                        switches,
                        plan: PlanImpl::$enum(new),
                    });
                }
            };
        }

        // ASCII'ye geçiş ekler. C40/Text bitişindeki zorunlu ASCII kuyruğu mode
        // planına ayrıca eklenmez; ilgili EOD kuralı tarafından fiyatlandırılır.
        // Böylece yalnızca tek mode isteyen çağrıların kısıtı da korunur.
        if !self.is_ascii() && enabled_modes.contains(EncodationType::Ascii) {
            add_switch!(AsciiPlan, Ascii, 0);
        }

        // Base256'ya geçiş ekler.
        if !matches!(self.plan, PlanImpl::Base256(_))
            && enabled_modes.contains(EncodationType::Base256)
        {
            add_switch!(Base256Plan, Base256, 1);
        }

        // Edifact'e geçiş ekler.
        if !matches!(self.plan, PlanImpl::Edifact(_))
            && enabled_modes.contains(EncodationType::Edifact)
        {
            add_switch!(EdifactPlan, Edifact, 1);
        }

        // X12'ye geçiş ekler.
        if !self.is_x12() && enabled_modes.contains(EncodationType::X12) {
            add_switch!(X12Plan, X12, 1);
        }

        // Text'e geçiş ekler.
        if !matches!(self.plan, PlanImpl::Text(_)) && enabled_modes.contains(EncodationType::Text) {
            add_switch!(TextPlan, Text, 1);
        }

        // C40'a geçiş ekler.
        if !self.is_c40() && enabled_modes.contains(EncodationType::C40) {
            add_switch!(C40Plan, C40, 1);
        }
    }

    fn is_ascii(&self) -> bool {
        matches!(self.plan, PlanImpl::Ascii(_))
    }

    pub(super) fn is_c40(&self) -> bool {
        matches!(self.plan, PlanImpl::C40(_))
    }

    pub(super) fn is_x12(&self) -> bool {
        matches!(self.plan, PlanImpl::X12(_))
    }

    /// Verilen mode'a geçişten sonraki toplam cost'u döndürür.
    pub(super) fn cost_for_switching_to(&self, other: EncodationType) -> Option<Frac> {
        // Aynı mode'a geçiş cost oluşturmaz.
        if self.current() == other {
            return Some(self.cost());
        }
        match (&self.plan, other) {
            // Ascii'ye geçiş mode_switch_cost() tarafından sağlanır.
            (_, EncodationType::Ascii) => self.mode_switch_cost(),
            // Diğerleri için önce ASCII'ye çıkış, hedef ASCII değilse ayrıca +1 gerekir.
            (_, EncodationType::Base256) => self.mode_switch_cost().map(|x| x + 2),
            (_, _) => self.mode_switch_cost().map(|x| x + 1),
        }
    }
}

impl<'a> Plan for GenericPlan<'a> {
    type Context = Context<'a>;

    fn mode_switch_cost(&self) -> Option<Frac> {
        // Yalnızca önceki mode'ların cost'larını ekler.
        dispatch!(self, pl => pl.mode_switch_cost().map(|x| x + self.extra))
    }

    fn cost(&self) -> Frac {
        // Yalnızca önceki mode'ların cost'larını ekler.
        dispatch!(self, pl => pl.cost() + self.extra)
    }

    fn end_cost(&self) -> Option<Frac> {
        dispatch!(self, pl => pl.end_cost().map(|cost| cost + self.extra))
    }

    fn state_key(&self) -> usize {
        dispatch!(self, pl => pl.state_key())
    }

    fn step(&mut self) -> Option<StepResult> {
        dispatch!(mut self, pl => pl.step())
    }

    fn write_unlatch(&self) -> Context<'a> {
        dispatch!(self, pl => pl.write_unlatch())
    }
}

#[derive(Debug, PartialEq, Clone)]
pub(super) struct Context<'a> {
    data: &'a [u8],
    symbol_list: &'a SymbolList,
    consumed: usize,
    written: usize,
}

impl<'a> Context<'a> {
    pub(super) fn new(data: &'a [u8], symbol_list: &'a SymbolList) -> Self {
        Self {
            data,
            symbol_list,
            consumed: 0,
            written: 0,
        }
    }
}

impl<'a> ContextInformation for Context<'a> {
    fn symbol_size_left(&self, extra_chars: usize) -> Option<usize> {
        self.written
            .checked_add(extra_chars)
            .and_then(|written| self.symbol_list.space_left_for(written))
    }

    fn write(&mut self, size: usize) {
        self.written = self.written.saturating_add(size);
    }

    fn rest(&self) -> &[u8] {
        self.data
    }

    fn eat(&mut self) -> Option<u8> {
        if let Some((ch, rest)) = self.data.split_first() {
            self.data = rest;
            self.consumed += 1;
            Some(*ch)
        } else {
            None
        }
    }
}

#[test]
fn test_add_switch_ascii() -> Result<(), &'static str> {
    let symbols = crate::SymbolList::default();
    let mut plan = GenericPlan::for_mode(EncodationType::Ascii, b"[]ABC01", 0, &symbols);
    plan.step();
    plan.step();
    plan.step();
    assert_eq!(plan.cost(), 3.into());
    let mut list = vec![];
    plan.add_switches(&mut list, 20, false, EncodationType::all());
    let plan = list.get(4).ok_or("beklenen planner geçişi bulunamadı")?;
    let PlanImpl::C40(c40_plan) = &plan.plan else {
        return Err("planner beklenen C40 planını döndürmedi");
    };
    // Bir karakter tüketildi.
    assert_eq!(c40_plan.cost(), Frac::new(2, 3));
    assert_eq!(plan.cost(), Frac::new(2, 3) + 4);
    Ok(())
}
