use super::ContextInformation;
use super::{Frac, Plan, StepResult, frac::C};
use crate::encodation::ascii;
use crate::encodation::edifact::is_encodable;

#[derive(Debug, PartialEq, Clone)]
pub(super) struct EdifactPlan<T: ContextInformation> {
    /// Henüz yazılmamış değer sayısı.
    ctx: T,
    written: usize,
    ascii_end: Option<Frac>,
    cost: Frac,
}

impl<T: ContextInformation> EdifactPlan<T> {
    pub(super) fn new(ctx: T) -> Self {
        Self {
            ctx,
            ascii_end: None,
            written: 0,
            cost: 0.into(),
        }
    }

    pub(super) fn context(&self) -> &T {
        &self.ctx
    }
}

impl<T: ContextInformation> Plan for EdifactPlan<T> {
    type Context = T;

    fn mode_switch_cost(&self) -> Option<Frac> {
        if self.written == 3 {
            // Dört değerden üçü yazıldığında UNLATCH ek cost oluşturmaz.
            Some(self.cost.ceil())
        } else {
            Some((self.cost + Frac::new(3, 4)).ceil())
        }
    }

    fn cost(&self) -> Frac {
        // Kesirli tahmin encoding sırasında tamdır; yalnızca end of data kesin bir
        // flush gerektirir. ASCII kuyruk durumu (`ascii_end`) kendi cost'unu zaten
        // hesaba katar ve UNLATCH eklemez.
        if self.ctx.has_more_characters() || self.ascii_end.is_some() {
            return self.cost;
        }
        // Buffer'da kalan `written` değerleri için edifact.rs handle_end davranışını yansıtır.
        let w = self.written;
        let space = self.ctx.symbol_size_left(w).unwrap_or(0);
        let trailing = if w == 0 {
            // Buffer boşsa padding öncesinde UNLATCH yalnızca ikiden fazla codeword
            // kaldığında gerekir. Bir veya iki alan, EDIFACT end-of-data kuralına
            // göre UNLATCH olmadan ASCII pad ile doldurulur.
            if space > 2 { 1 } else { 0 }
        } else {
            // Buffer'daki değerleri tek grup halinde flush eder; symbol içinde alan
            // varsa veya grup doluysa (üç değer) sonuna UNLATCH ekler.
            let symbols = if space > 0 || w == 3 { w + 1 } else { w };
            symbols.min(3)
        };
        // Kısmi grubun kesirli tahminini encoder'ın gerçekte yazdığı codeword
        // sayısıyla değiştirir.
        self.cost - Frac::new(3 * w as C, 4) + trailing as C
    }

    fn state_key(&self) -> usize {
        if self.ascii_end.is_some() {
            4
        } else {
            self.written
        }
    }

    fn write_unlatch(&self) -> Self::Context {
        let mut ctx = self.ctx.clone();
        // Encoder bunu herhangi bir byte yazılmadan önce çağırır.
        ctx.write((self.written + 1).min(3));
        ctx
    }

    fn step(&mut self) -> Option<StepResult> {
        let end = !self.ctx.has_more_characters();
        if !end {
            if self.written == 0 && self.ctx.characters_left() <= 4 && self.ascii_end.is_none() {
                // Olası bir end of data durumunda mıyız?
                let ascii_size = ascii::encoding_size(self.ctx.rest());
                if ascii_size <= 2 {
                    let space_left = self.ctx.symbol_size_left(ascii_size)?;
                    if space_left + ascii_size <= 2 {
                        let chars_to_read = self.ctx.characters_left();
                        self.ascii_end = Some(Frac::new(ascii_size as C, chars_to_read as C));
                    }
                }
            }
            if self.ascii_end.is_none() {
                let ch = self.ctx.peek(0)?;
                if !is_encodable(ch) {
                    return None;
                }
            }
            self.ctx.eat()?;
            if let Some(portion_per_char) = self.ascii_end {
                // Doğru boyut için okunan her karakterde (ascii_size / chars_to_read) ekler.
                self.cost += portion_per_char;
            } else {
                self.cost += Frac::new(3, 4);
                self.written = (self.written + 1) % 4;
                if self.written == 0 {
                    self.ctx.write(3);
                }
            }
        }
        Some(StepResult {
            end,
            unbeatable: self.ascii_end.is_some(),
        })
    }
}
