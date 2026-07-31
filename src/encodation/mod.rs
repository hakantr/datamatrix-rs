//! Belirtilen bütün mode'ları kullanan data encodation implementasyonu.
use crate::symbol_size::{SymbolList, SymbolSize};

use alloc::{vec, vec::Vec};
use ascii::FNC1;
use flagset::FlagSet;

pub(crate) mod ascii;
mod base256;
mod c40;
pub(crate) mod edifact;
mod text;
mod x12;

mod encodation_type;
pub(crate) mod planner;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod proptests;

pub use encodation_type::EncodationType;

pub(crate) const MACRO05: u8 = 236;
pub(crate) const MACRO06: u8 = 237;
pub(crate) const MACRO05_HEAD: &[u8] = b"[)>\x1E05\x1D";
pub(crate) const MACRO06_HEAD: &[u8] = b"[)>\x1E06\x1D";
pub(crate) const MACRO_TRAIL: &[u8] = b"\x1E\x04";

// Aşağıdakiler henüz uygulanmamıştır.
// const STRUCT_APPEND: u8 = 233;
// const READER_PROGRAMMING: u8 = 234;

pub(crate) const UNLATCH: u8 = 254;

#[cfg(test)]
use pretty_assertions::assert_eq;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data bölümü encode edilirken oluşan hata.
pub enum DataEncodingError {
    TooMuchOrIllegalData,
    SymbolListEmpty,
    InvalidEci(u32),
}

impl core::fmt::Display for DataEncodingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooMuchOrIllegalData => {
                f.write_str("veri seçilen Data Matrix symbol boyutlarına sığmıyor veya geçersiz")
            }
            Self::SymbolListEmpty => f.write_str("izin verilen Data Matrix symbol listesi boş"),
            Self::InvalidEci(eci) => write!(f, "ECI değeri 0..=999999 aralığında olmalı: {eci}"),
        }
    }
}

impl core::error::Error for DataEncodingError {}

trait EncodingContext {
    /// İleriye bakar ve gerekirse mode'u değiştirir.
    ///
    /// Mode değiştirildiyse `true` döndürür.
    fn maybe_switch_mode(&mut self) -> bool;

    /// Symbol içinde ne kadar alan kalacağını hesaplar.
    ///
    /// `extra_codewords`, yazılacak ek codeword sayısıdır. Bu sayı kalan alana
    /// dahil değildir. Örneğin symbol içinde iki alan kalmışsa ve
    /// `extra_codewords` 1 ise fonksiyon `Some(1)` döndürür.
    fn symbol_size_left(&mut self, extra_codewords: usize) -> Option<usize>;

    fn eat(&mut self) -> Option<u8>;

    fn backup(&mut self, steps: usize);

    fn rest(&self) -> &[u8];

    fn push(&mut self, ch: u8);

    fn replace(&mut self, index: usize, ch: u8);

    fn insert(&mut self, index: usize, ch: u8);

    /// Şimdiye kadar yazılan codeword'leri döndürür.
    fn codewords(&self) -> &[u8];

    fn set_ascii_until_end(&mut self);

    /// Henüz encode edilmemiş karakter sayısı.
    fn characters_left(&self) -> usize {
        self.rest().len()
    }

    /// İşlenecek başka karakter var mı?
    fn has_more_characters(&self) -> bool {
        !self.rest().is_empty()
    }
}

pub(crate) struct GenericDataEncoder<'a> {
    data: &'a [u8],
    input: &'a [u8],
    encodation: EncodationType,
    symbol_list: &'a SymbolList,
    planned_switches: Vec<(usize, EncodationType)>,
    new_mode: Option<u8>,
    codewords: Vec<u8>,
    enabled_modes: FlagSet<EncodationType>,
}

impl<'a> EncodingContext for GenericDataEncoder<'a> {
    fn maybe_switch_mode(&mut self) -> bool {
        let chars_left = self.characters_left();
        let Some(next_switch) = self.planned_switches.first().copied() else {
            crate::invariant_violation("encodation planında sonraki mode switch bulunamadı");
        };
        if chars_left < next_switch.0 {
            crate::invariant_violation("encodation planındaki mode switch konumu geçildi");
        }
        let new_mode = if chars_left > 0 && chars_left == next_switch.0 {
            let switch = self.planned_switches.remove(0);
            switch.1
        } else {
            self.encodation
        };
        let switch = new_mode != self.encodation;
        if switch {
            // Yeni mode ASCII değilse ilgili LATCH codeword'ünü hazırla.
            self.encodation = new_mode;
            if !new_mode.is_ascii() {
                let Some(latch) = new_mode.latch_from_ascii() else {
                    crate::invariant_violation(
                        "ASCII mode'dan geçiş için LATCH codeword bulunamadı",
                    );
                };
                self.new_mode = Some(latch);
            }
        }
        switch
    }

    fn symbol_size_left(&mut self, extra_codewords: usize) -> Option<usize> {
        self.symbol_list
            .space_left_for(self.codewords.len() + extra_codewords)
    }

    fn eat(&mut self) -> Option<u8> {
        let (ch, rest) = self.data.split_first()?;
        self.data = rest;
        Some(*ch)
    }

    fn backup(&mut self, steps: usize) {
        let Some(consumed) = self.input.len().checked_sub(self.data.len()) else {
            crate::invariant_violation("input ve kalan veri uzunlukları tutarsız");
        };
        let Some(offset) = consumed.checked_sub(steps) else {
            crate::invariant_violation("istenen backup miktarı tüketilen input miktarını aşıyor");
        };
        let Some(data) = self.input.get(offset..) else {
            crate::invariant_violation("backup sonrasında input dilimi oluşturulamadı");
        };
        self.data = data;
    }

    fn rest(&self) -> &[u8] {
        self.data
    }

    fn push(&mut self, ch: u8) {
        self.codewords.push(ch);
    }

    fn codewords(&self) -> &[u8] {
        &self.codewords
    }

    fn replace(&mut self, index: usize, ch: u8) {
        let Some(codeword) = self.codewords.get_mut(index) else {
            crate::invariant_violation("değiştirilecek codeword konumu bulunamadı");
        };
        *codeword = ch;
    }

    fn insert(&mut self, index: usize, ch: u8) {
        if index > self.codewords.len() {
            crate::invariant_violation("codeword ekleme konumu mevcut uzunluğu aşıyor");
        }
        self.codewords.insert(index, ch);
    }

    fn set_ascii_until_end(&mut self) {
        self.encodation = EncodationType::Ascii;
        self.planned_switches = vec![(0, EncodationType::Ascii)];
    }
}

impl<'a> GenericDataEncoder<'a> {
    pub fn with_size(
        data: &'a [u8],
        symbol_list: &'a SymbolList,
        enabled_modes: FlagSet<EncodationType>,
        start_with_fnc1: bool,
    ) -> Self {
        let codewords = if start_with_fnc1 { vec![FNC1] } else { vec![] };
        Self {
            data,
            input: data,
            symbol_list,
            new_mode: None,
            encodation: EncodationType::Ascii,
            codewords,
            planned_switches: vec![],
            enabled_modes,
        }
    }

    pub fn use_macro_if_possible(&mut self) {
        if !self.codewords.is_empty() || !self.data.ends_with(MACRO_TRAIL) {
            return;
        }
        for (head, cw) in [(MACRO05_HEAD, MACRO05), (MACRO06_HEAD, MACRO06)] {
            if self.data.starts_with(head) {
                let Some(end) = self.data.len().checked_sub(MACRO_TRAIL.len()) else {
                    return;
                };
                let Some(body) = self.data.get(head.len()..end) else {
                    return;
                };
                self.codewords.push(cw);
                self.data = body;
                break;
            }
        }
    }

    pub fn write_eci(&mut self, mut c: u32) -> Result<(), DataEncodingError> {
        if c > 999_999 {
            return Err(DataEncodingError::InvalidEci(c));
        }
        self.codewords.push(ascii::ECI);
        match c {
            0..=126 => self.codewords.push(c as u8 + 1),
            127..=16382 => {
                c -= 127;
                self.codewords.push((c / 254 + 128) as u8);
                self.codewords.push((c % 254 + 1) as u8);
            }
            16383..=999999 => {
                c -= 16383;
                self.codewords.push((c / 64516 + 192) as u8);
                self.codewords.push(((c / 254) % 254 + 1) as u8);
                self.codewords.push((c % 254 + 1) as u8);
            }
            _ => return Err(DataEncodingError::InvalidEci(c)),
        }
        Ok(())
    }

    pub fn codewords(&mut self) -> Result<(Vec<u8>, SymbolSize), DataEncodingError> {
        let symbol_size = self.encode_to_unpadded()?;
        self.add_padding(symbol_size);

        let mut codewords = vec![];
        core::mem::swap(&mut codewords, &mut self.codewords);

        Ok((codewords, symbol_size))
    }

    /// Planner ve encoder'ları çalıştırıp unpadded data codeword'leri
    /// `self.codewords` içinde bırakır. Seçilen symbol size değerini döndürür.
    fn encode_to_unpadded(&mut self) -> Result<SymbolSize, DataEncodingError> {
        if self.symbol_list.is_empty() {
            return Err(DataEncodingError::SymbolListEmpty);
        }

        // Teorik sınır aşılıyorsa erken hata ver.
        if self.data.len() > self.symbol_list.max_capacity() {
            return Err(DataEncodingError::TooMuchOrIllegalData);
        }

        self.codewords
            .reserve(self.upper_limit_for_number_of_codewords()?);

        self.planned_switches = planner::optimize(
            self.data,
            self.codewords.len(),
            EncodationType::Ascii,
            self.symbol_list,
            self.enabled_modes,
        )
        .ok_or(DataEncodingError::TooMuchOrIllegalData)?;

        let mut no_write_run = 0;
        while self.has_more_characters() {
            if let Some(new_mode) = self.new_mode.take() {
                self.push(new_mode);
            }
            let len = self.codewords.len();

            self.encodation.clone().encode(self)?;

            let Some(words_written) = self.codewords.len().checked_sub(len) else {
                crate::invariant_violation("encoder codeword uzunluğu beklenmedik biçimde azaldı");
            };
            if words_written <= 1 {
                // End of data noktasında hiçbir mode 1 codeword ile yararlı işlem yapamaz; bu normaldir.
                no_write_run += 1;
                if no_write_run > 5 {
                    crate::invariant_violation(
                        "encoder art arda altı adım boyunca ilerleme sağlayamadı",
                    );
                }
            } else {
                no_write_run = 0;
            }
        }

        self.symbol_for(0)
            .ok_or(DataEncodingError::TooMuchOrIllegalData)
    }

    /// Encoder'ın padding öncesinde ürettiği data codeword sayısı.
    ///
    /// Planner'ın tahmin ettiği cost ile encoder'ın gerçek çıktısının eşleştiğini
    /// denetleyen tutarlılık proptest'leri tarafından kullanılır.
    #[cfg(test)]
    pub(crate) fn unpadded_len(&mut self) -> Result<usize, DataEncodingError> {
        self.encode_to_unpadded()?;
        Ok(self.codewords.len())
    }

    fn symbol_for(&self, extra_codewords: usize) -> Option<SymbolSize> {
        self.symbol_list
            .first_symbol_big_enough_for(self.codewords.len() + extra_codewords)
    }

    fn add_padding(&mut self, size: SymbolSize) {
        let mut size_left = size.num_data_codewords() - self.codewords.len();
        if size_left == 0 {
            return;
        }
        if self.encodation != EncodationType::Ascii {
            self.encodation = EncodationType::Ascii;
            self.push(UNLATCH);
            size_left -= 1;
        }
        if size_left > 0 {
            self.push(ascii::PAD);
            size_left -= 1;
        }
        for _ in 0..size_left {
            // "randomize 253 state"
            let pos = self.codewords.len() + 1;
            let pseudo_random = (((149 * pos) % 253) + 1) as u16;
            let tmp = ascii::PAD as u16 + pseudo_random;
            if tmp <= 254 {
                self.push(tmp as u8);
            } else {
                self.push((tmp - 254) as u8);
            }
        }
    }

    /// Symbol listesiyle encode edilebilecek teorik azami veri uzunluğunu döndürür.
    fn upper_limit_for_number_of_codewords(&self) -> Result<usize, DataEncodingError> {
        self.symbol_list
            .upper_limit_for_number_of_codewords(self.data.len())
            .ok_or(DataEncodingError::SymbolListEmpty)
    }
}

#[test]
fn test_empty() -> Result<(), DataEncodingError> {
    let symbols = crate::SymbolList::default();
    let mut enc = GenericDataEncoder::with_size(&[], &symbols, EncodationType::all(), false);
    let (cw, _) = GenericDataEncoder::codewords(&mut enc)?;
    assert_eq!(cw, vec![ascii::PAD, 175, 70]);
    Ok(())
}
