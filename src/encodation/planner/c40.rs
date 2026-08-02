use core::fmt::Debug;
use core::marker::PhantomData;

use super::ContextInformation;
use super::frac::C;
use super::{Frac, Plan, StepResult};
use crate::encodation::{ascii, c40};

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
    two_digit_ascii_end: bool,
    /// Sonda iki rakam bulunan ASCII bitişinin kullandığı codeword'ler.
    two_digit_tail: u8,
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
            two_digit_ascii_end: false,
            two_digit_tail: 0,
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
            // Doldurur, ardından UNLATCH uygular.
            Some(self.cost + 2 + 1)
        }
    }

    fn write_unlatch(&self) -> Self::Context {
        let mut ctx = self.ctx.clone();
        if self.values > 0 {
            if self.values > 2 {
                return ctx;
            }
            // C40 çiftini tamamlar.
            ctx.write(2);
        }
        ctx.write(1);
        ctx
    }

    fn cost(&self) -> Frac {
        if self.ctx.has_more_characters() {
            return self.cost + Frac::new(2 * self.values as C, 3);
        }
        if self.two_digit_ascii_end {
            // Sondaki iki rakam ASCII ile encode edilir; ASCII codeword ve isteğe
            // bağlı UNLATCH'ten oluşan kuyruğun cost'u algılama sırasında hesaplandı.
            return self.cost + self.two_digit_tail as C;
        }
        // Kalan değerleri saklamanın ek cost'unu hesaplar.
        let extra = if self.values == 2 {
            let space_left = self.ctx.symbol_size_left(2).unwrap_or(0);
            if space_left == 0 {
                2
            } else {
                // (val1, val2, 0) = 2 codeword olarak encode eder ve padding ile
                // devam etmek için son bir unlatch ekler.
                3
            }
        } else if self.values == 1 {
            // Rakam olmayan tek bir değer kalmıştır. Sonda iki rakam bulunan ASCII
            // bitişi daha önce döndüğü için buraya ulaşmaz.
            let space_left = self.ctx.symbol_size_left(1).unwrap_or(0);
            let ascii_size = ascii::encoding_size(&[self.ch]);
            if space_left == 0 {
                if ascii_size == 1 {
                    1
                } else {
                    // Bu durumda mümkünse daha büyük bir symbol gerekir.
                    1 + ascii_size
                }
            } else if space_left == 1 {
                // UNLATCH uygular, ardından ASCII ile encode eder (c40.rs handle_end c durumu).
                1 + ascii_size
            } else {
                // İki veya daha fazla codeword kaldığında encoder tek değer için
                // ASCII'ye geçmez: değeri tam bir C40 üçlüsüne (2 codeword) tamamlar
                // ve padding öncesinde UNLATCH uygular.
                3
            }
        } else {
            // End of data noktasında buffer boştur. Veri symbol'ü tam doldurmuyorsa
            // encoder padding öncesinde sona UNLATCH yazar; pad karakterlerinden önce
            // ASCII'ye dönülmelidir (ISO/IEC 16022:2024, 7.2.4.4).
            if self.ctx.symbol_size_left(0).unwrap_or(0) > 0 {
                1
            } else {
                0
            }
        };
        self.cost + extra as C
    }

    fn step(&mut self) -> Option<StepResult> {
        // En uygun karakterleri yalnızca boundary üzerindeyken ve daha önce
        // hesaplanmamışsa hesaplar.
        if self.values == 0 && self.unbeatable_reads == 0 {
            // Kalan karakterler yalnızca iki ASCII rakamı mı?
            if matches!(self.ctx.rest(), [a, b] if a.is_ascii_digit() && b.is_ascii_digit()) {
                // Encoder sondaki iki rakamı her zaman tek bir ASCII codeword olarak
                // encode eder. Symbol içinde alan varsa önüne UNLATCH gelir; yoksa
                // UNLATCH symbol sonunda örtüktür. Rakamlar C40 stream içinde tutulmaz.
                let space_left = self.ctx.symbol_size_left(1)?;
                self.two_digit_ascii_end = true;
                self.unbeatable_reads = 2;
                self.two_digit_tail = if space_left >= 1 { 2 } else { 1 };
                self.ctx.write(self.two_digit_tail as usize);
            }
            if !self.two_digit_ascii_end {
                // Sıradaki base set karakterlerini sayar; rakamlara dikkat eder.
                self.unbeatable_reads = unbeatable_strike(self.ctx.rest(), U::in_base_set);
                self.ctx.write((self.unbeatable_reads / 3) * 2);
            }
        }
        let unbeatable = self.unbeatable_reads > 0;
        let end = !self.ctx.has_more_characters();
        if !end {
            self.ch = self.ctx.eat()?;
            if self.unbeatable_reads > 0 {
                if !self.two_digit_ascii_end || self.values == 0 {
                    self.values += 1;
                }
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
