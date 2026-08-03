use core::fmt::Debug;
use core::marker::PhantomData;

use super::ContextInformation;
use super::frac::C;
use super::{Frac, Plan, StepResult};
use crate::encodation::c40;

#[cfg(test)]
use pretty_assertions::assert_eq;

pub(super) trait CharsetInfo: Clone + Debug + PartialEq {
    fn val_size(ch: u8) -> u8;

    fn in_base_set(ch: &u8) -> bool;
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct C40Charset;

impl CharsetInfo for C40Charset {
    fn val_size(ch: u8) -> u8 {
        c40::val_size(ch)
    }

    fn in_base_set(ch: &u8) -> bool {
        c40::in_base_set(*ch)
    }
}

#[derive(Debug, PartialEq, Clone)]
pub(super) struct C40LikePlan<T: ContextInformation, U: CharsetInfo> {
    /// Henüz yazılmamış değer sayısı.
    ctx: T,
    values: u8,
    unbeatable_reads: usize,
    ch: u8,
    cost: Frac,
    dummy: PhantomData<U>,
}

impl<T: ContextInformation, U: CharsetInfo> C40LikePlan<T, U> {
    pub(super) fn new(ctx: T) -> Self {
        Self {
            ctx,
            values: 0,
            ch: 0,
            unbeatable_reads: 0,
            cost: 0.into(),
            dummy: PhantomData,
        }
    }

    pub(super) fn context(&self) -> &T {
        &self.ctx
    }
}

pub(super) fn unbeatable_strike<F>(rest: &[u8], nice_char: F) -> usize
where
    F: Fn(&u8) -> bool,
{
    let mut consecutive_digits = 0;
    let mut unbeatable_reads = 0;
    for ch in rest.iter().cloned().take_while(nice_char) {
        unbeatable_reads += 1;
        // Bu noktadan sonra yalnızca yeterli sayıdaki rakamı ASCII ile encode etmek daha iyi olabilir.
        if ch.is_ascii_digit() {
            consecutive_digits += 1;
            if consecutive_digits == 7 {
                unbeatable_reads -= consecutive_digits;
                break;
            }
        } else {
            consecutive_digits = 0;
        }
    }
    (unbeatable_reads / 3) * 3
}

impl<T: ContextInformation, U: CharsetInfo> Plan for C40LikePlan<T, U> {
    type Context = T;

    fn mode_switch_cost(&self) -> Option<Frac> {
        if self.values == 0 {
            // Boundary üzerindedir; yalnızca bir UNLATCH gerekir.
            Some(self.cost + 1)
        } else {
            // Eksik C40/Text üçlüsü doldurularak mode switch yapılamaz. Planner,
            // karakter ve üçlü sınırındaki daha önceki ASCII geçişini kullanmalıdır.
            None
        }
    }

    fn write_unlatch(&self) -> Self::Context {
        let mut ctx = self.ctx.clone();
        ctx.write(1);
        ctx
    }

    fn cost(&self) -> Frac {
        if self.ctx.has_more_characters() {
            return self.cost + Frac::new(2 * self.values as C, 3);
        }
        self.end_cost().unwrap_or(self.cost + 1_000_000)
    }

    fn end_cost(&self) -> Option<Frac> {
        if self.ctx.has_more_characters() {
            return Some(self.cost + Frac::new(2 * self.values as C, 3));
        }
        match self.values {
            0 => {
                // Padding öncesinde ASCII'ye açıkça dönülür.
                let unlatch = u32::from(self.ctx.symbol_size_left(0)? > 0);
                Some(self.cost + unlatch)
            }
            1 if U::val_size(self.ch) == 1 => {
                // 7.2.5.3 d: tek yuva tam doluyorsa örtük unlatch. Daha geniş
                // symbol'de 7.2.5.3'ün "diğer bütün durumlar" kuralıyla açık
                // UNLATCH ve ASCII karakteri kullanılır.
                if self.ctx.symbol_size_left(1) == Some(0) {
                    Some(self.cost + 1)
                } else if self.ctx.symbol_size_left(2).is_some() {
                    Some(self.cost + 2)
                } else {
                    None
                }
            }
            2 if self.ctx.symbol_size_left(2) == Some(0) => {
                // 7.2.5.3 b yalnızca son iki symbol karakterinde geçerlidir.
                Some(self.cost + 2)
            }
            _ => None,
        }
    }

    fn state_key(&self) -> usize {
        usize::from(self.values)
    }

    fn step(&mut self) -> Option<StepResult> {
        // En uygun karakterleri yalnızca boundary üzerindeyken ve daha önce
        // hesaplanmamışsa hesaplar.
        if self.values == 0 && self.unbeatable_reads == 0 {
            // Sıradaki base set karakterlerini sayar; rakamlara dikkat eder.
            self.unbeatable_reads = unbeatable_strike(self.ctx.rest(), U::in_base_set);
            self.ctx.write((self.unbeatable_reads / 3) * 2);
        }
        let unbeatable = self.unbeatable_reads > 0;
        let end = !self.ctx.has_more_characters();
        if !end {
            self.ch = self.ctx.eat()?;
            if self.unbeatable_reads > 0 {
                self.values += 1;
                self.unbeatable_reads -= 1;
            } else {
                self.values += U::val_size(self.ch);
            }
            while self.values >= 3 {
                self.cost += 2;
                if !unbeatable {
                    self.ctx.write(2);
                }
                self.values -= 3;
            }
        }
        Some(StepResult { end, unbeatable })
    }
}

pub(super) type C40Plan<T> = C40LikePlan<T, C40Charset>;

#[test]
fn test_eod_case1() {
    use super::generic::Context;

    let symbols = crate::SymbolList::default();
    let mut plan = C40Plan::new(Context::new(b"DEABCFG", &symbols));
    for _ in 0..7 {
        plan.step();
    }
    assert_eq!(plan.cost(), 5.into());
}
