use super::ContextInformation;
use super::{Frac, Plan, StepResult, frac::C};
use crate::encodation::ascii;
use crate::encodation::x12::is_native_x12;

#[cfg(test)]
use pretty_assertions::assert_eq;

#[derive(Debug, PartialEq, Clone)]
pub(super) struct X12Plan<T: ContextInformation> {
    /// Henüz yazılmamış değer sayısı.
    ctx: T,
    values: usize,
    ascii_end: Option<Frac>,
    cost: Frac,
}

impl<T: ContextInformation> X12Plan<T> {
    pub(super) fn new(ctx: T) -> Self {
        Self {
            ctx,
            ascii_end: None,
            values: 0,
            cost: 0.into(),
        }
    }

    pub(super) fn context(&self) -> &T {
        &self.ctx
    }
}

impl<T: ContextInformation> Plan for X12Plan<T> {
    type Context = T;

    fn mode_switch_cost(&self) -> Option<Frac> {
        if self.values == 0 {
            Some(self.cost + 1)
        } else {
            None
        }
    }

    fn cost(&self) -> Frac {
        let mut cost = self.cost;
        // X12 yalnızca tam üçlüler üretir. Veri symbol'ü tam doldurmuyorsa üçlü
        // boundary üzerindeki end of data noktasında encoder padding öncesinde
        // UNLATCH ile ASCII'ye döner. Sondaki ASCII kuyruğu `ascii_end` ile izlenir
        // ve kendi cost'unu hesaba katar.
        if !self.ctx.has_more_characters()
            && self.ascii_end.is_none()
            && self.values == 0
            && self.ctx.symbol_size_left(0).unwrap_or(0) > 0
        {
            cost += 1;
        }
        cost
    }

    fn state_key(&self) -> usize {
        if self.ascii_end.is_some() {
            4
        } else {
            self.values
        }
    }

    fn write_unlatch(&self) -> Self::Context {
        let mut ctx = self.ctx.clone();
        ctx.write(1);
        ctx
    }

    fn step(&mut self) -> Option<StepResult> {
        let end = !self.ctx.has_more_characters();
        if !end {
            if self.values == 0 && self.ctx.characters_left() <= 2 && self.ascii_end.is_none() {
                // Olası bir end of data durumunda mıyız?
                let ascii_size = ascii::encoding_size(self.ctx.rest());
                if ascii_size == 1 {
                    let space_left = self.ctx.symbol_size_left(ascii_size)?;
                    if space_left <= 1 {
                        if space_left == 1 {
                            // Unlatch
                            self.cost += 1;
                        }
                        let portion_per_char =
                            Frac::new(ascii_size as C, self.ctx.characters_left() as C);
                        self.ascii_end = Some(portion_per_char);
                    }
                }
                if self.ascii_end.is_none() {
                    // İki karakter kalmıştır ve ascii_size > 1'dir veya gereğinden
                    // fazla alan vardır. Bu durumda saf ASCII iyileştirilemez.
                    self.cost += 1; // Unlatch
                    let portion_per_char =
                        Frac::new(ascii_size as C, self.ctx.characters_left() as C);
                    self.ascii_end = Some(portion_per_char);
                }
            }
            if self.ascii_end.is_none() {
                let ch = self.ctx.eat()?;
                if !is_native_x12(ch) {
                    return None;
                }
            }
            if let Some(portion_per_char) = self.ascii_end {
                // Doğru boyut için okunan her karakterde (ascii_size / chars_to_read) ekler.
                self.ctx.eat()?;
                self.cost += portion_per_char;
            } else {
                self.cost += Frac::new(2, 3);
                self.values = (self.values + 1) % 3;
                if self.values == 0 {
                    self.ctx.write(2);
                }
            }
        }
        Some(StepResult {
            end,
            unbeatable: self.ascii_end.is_some(),
        })
    }
}

#[test]
fn test_eod_case1() {
    use super::generic::Context;

    let symbols = crate::SymbolList::default();
    let mut plan = X12Plan::new(Context::new(b"DEABCFG", &symbols));
    for _ in 0..7 {
        assert!(plan.step().is_some());
    }
    assert_eq!(plan.cost(), 5.into());
}

#[test]
fn test_eod_case2() {
    use super::generic::Context;

    let symbols = crate::SymbolList::default();
    let mut plan = X12Plan::new(Context::new(b"AIMAIMAIMAIMAI", &symbols));
    for i in 0..12 {
        assert!(plan.step().is_some(), "karakter {}", i + 1);
    }
    assert_eq!(plan.cost(), 8.into());
    // İki karakter (AI) kalmıştır ancak symbol gereğinden büyüktür;
    // toplam cost 3'tür ve sonraki iki adıma bölünür.
    assert!(plan.step().is_some());
    assert_eq!(plan.cost(), 10.into());
    assert!(plan.step().is_some());
    assert_eq!(plan.cost(), 11.into());
}
