use flagset::{FlagSet, flags};

use super::{DataEncodingError, GenericDataEncoder, ascii, base256, c40, edifact, text, x12};

flags! {
    /// Data encodation türlerinin listesi.
    ///
    /// Data Matrix, tek bir symbol içinde farklı "codec"ler arasında geçiş
    /// yapabilir. Her birinin güçlü ve zayıf yönleri vardır.
    pub enum EncodationType: u8 {
        Ascii   = 0b000001,
        C40     = 0b000010,
        Text    = 0b000100,
        X12     = 0b001000,
        Edifact = 0b010000,
        Base256 = 0b100000,
    }
}

impl EncodationType {
    /// Encodation türü için 0 ile 5 arasında sabit bir indeks döndürür.
    ///
    /// İndeks encodation türlerinin tercih sırasını da belirtir; küçük sayılar
    /// daha çok tercih edilir.
    pub fn index(&self) -> usize {
        match self {
            // Sıra, mode'ların karmaşıklığına ilişkin kişisel tahmine göre seçilmiştir.
            Self::Ascii => 0,
            Self::Base256 => 1,
            Self::Edifact => 2,
            Self::X12 => 3,
            Self::C40 => 4,
            Self::Text => 5,
        }
    }

    /// Bütün encodation türlerinin etkin olduğu flag set'i döndürür.
    pub fn all() -> FlagSet<Self> {
        FlagSet::full()
    }

    pub(super) fn encode<'a, 'b: 'a>(
        &self,
        encoder: &'a mut GenericDataEncoder<'b>,
    ) -> Result<(), DataEncodingError> {
        match self {
            Self::Ascii => ascii::encode(encoder),
            Self::C40 => c40::encode(encoder),
            Self::Text => text::encode(encoder),
            Self::X12 => x12::encode(encoder),
            Self::Edifact => edifact::encode(encoder),
            Self::Base256 => base256::encode(encoder),
        }
    }

    pub(super) fn is_ascii(&self) -> bool {
        matches!(self, EncodationType::Ascii)
    }

    /// ASCII'den bu mode'a geçmek için gereken LATCH codeword'ü döndürür.
    pub(super) fn latch_from_ascii(&self) -> Option<u8> {
        match self {
            Self::Ascii => None,
            Self::C40 => Some(ascii::LATCH_C40),
            Self::Text => Some(ascii::LATCH_TEXT),
            Self::X12 => Some(ascii::LATCH_X12),
            Self::Edifact => Some(ascii::LATCH_EDIFACT),
            Self::Base256 => Some(ascii::LATCH_BASE256),
        }
    }
}
