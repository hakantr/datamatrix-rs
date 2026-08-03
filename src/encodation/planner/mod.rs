//! Bu modül en uygun encodation mode'unu bulur.
//!
//! Crate'in geri kalanından neredeyse tamamen ayrıdır. Input'un herhangi bir
//! noktasında hangi encodation mode'una geçileceğini belirlemek için kullanılabilir.
mod ascii;
mod base256;
mod c40;
mod edifact;
mod text;
mod x12;

mod frac;
mod generic;
mod shortest_path;
use frac::Frac;

pub(crate) use shortest_path::optimize;
#[cfg(test)]
pub(crate) use shortest_path::optimize_cost;

trait ContextInformation: Clone {
    fn symbol_size_left(&self, extra_chars: usize) -> Option<usize>;

    fn rest(&self) -> &[u8];

    fn eat(&mut self) -> Option<u8>;

    fn write(&mut self, bytes: usize);

    fn peek(&self, n: usize) -> Option<u8> {
        self.rest().get(n).copied()
    }

    fn characters_left(&self) -> usize {
        self.rest().len()
    }

    fn has_more_characters(&self) -> bool {
        !self.rest().is_empty()
    }
}

#[derive(Debug, PartialEq)]
struct StepResult {
    /// İşlem yapılmadığını ve planner'ın input sonunda olduğunu belirtir.
    end: bool,
    /// Bu adımın önceki bir mode switch tarafından iyileştirilemeyeceğini belirtir.
    unbeatable: bool,
}

trait Plan: Clone {
    type Context;

    /// Mümkünse ASCII mode'a geçişten sonraki yeni cost'u döndürür.
    fn mode_switch_cost(&self) -> Option<Frac>;

    /// Geçerli cost'u döndürür.
    fn cost(&self) -> Frac;

    /// Input sonundaki plan geçerliyse kesin bitiş cost'unu döndürür.
    ///
    /// Bazı encodation mode'larında kesirli bir grubun her konumda geçerli bir
    /// bitişi yoktur. Bu ayrım, standarda aykırı dolgu üretmek yerine planner'ın
    /// daha önceki geçerli bir mode switch'i seçmesini sağlar.
    fn end_cost(&self) -> Option<Frac> {
        Some(self.cost())
    }

    /// Dominance pruning sırasında korunması gereken mode-içi durum.
    fn state_key(&self) -> usize {
        0
    }

    /// Varsa sıradaki karakteri okur; başarısız olursa None döndürür.
    fn step(&mut self) -> Option<StepResult>;

    /// Mode değiştirildiğinde çağrılır.
    fn write_unlatch(&self) -> Self::Context;
}
