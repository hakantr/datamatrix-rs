use super::ContextInformation;
use super::{Frac, Plan, StepResult};

#[derive(Debug, PartialEq, Clone)]
pub(super) struct Base256Plan<T: ContextInformation> {
    ctx: T,
    written: usize,
    cost: Frac,
}

impl<T: ContextInformation> Base256Plan<T> {
    pub(super) fn new(mut ctx: T) -> Self {
        // Uzunluk byte'ı için
        ctx.write(1);
        Self {
            ctx,
            written: 0,
            cost: 1.into(), // Başlangıç uzunluk byte'ı
        }
    }

    pub(super) fn context(&self) -> &T {
        &self.ctx
    }
}

impl<T: ContextInformation> Plan for Base256Plan<T> {
    type Context = T;

    fn mode_switch_cost(&self) -> Option<Frac> {
        if self.written >= 250 {
            Some(self.cost + 1)
        } else {
            Some(self.cost)
        }
    }

    fn cost(&self) -> Frac {
        if !self.ctx.has_more_characters() {
            let left = self.ctx.symbol_size_left(0).unwrap_or(1);
            if left > 0 && self.written >= 250 {
                // Uzunluk için fazladan 1 byte kullanılmalıdır.
                return self.cost + 1;
            }
        }
        self.cost
    }

    fn state_key(&self) -> usize {
        usize::from(self.written >= 250)
    }

    fn write_unlatch(&self) -> T {
        let mut ctx = self.ctx.clone();
        if self.written >= 250 {
            // Büyük uzunluk için ek byte
            ctx.write(1);
        }
        ctx
    }

    fn step(&mut self) -> Option<StepResult> {
        let end = !self.ctx.has_more_characters();
        if !end {
            self.ctx.eat()?;
            self.written += 1;
            self.cost += 1;
            self.ctx.write(1);
            if self.written == 1556 {
                return None;
            }
        }
        Some(StepResult {
            end,
            unbeatable: false,
        })
    }
}
