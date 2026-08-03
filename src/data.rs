//! Data bölümünün decoding ve encoding işlemleri.
//!
//! Data Matrix symbol içine encode edilen byte'lar iki bölümden oluşur. İlk bölüm
//! encode edilmek istenen gerçek bilgiyi, ikinci bölüm error correction byte'larını
//! içerir.
//!
//! Bu modüldeki fonksiyonlar ilk bölümü, yani data bölümünü decode ve encode etmek
//! için kullanılabilir.
//!
//! Kütüphanenin son kullanıcısının bunları doğrudan çağırması normalde gerekmez;
//! ancak daha düşük seviyede çalışırken yararlı olabilirler.
use alloc::{string::String, vec::Vec};
use flagset::FlagSet;

pub use crate::decodation::{
    DataDecodingError, DecodedMessage, decode_data, decode_message, decode_str,
};
pub use crate::encodation::{DataEncodingError, EncodationType, Fnc1Position};
use crate::encodation::{
    GenericDataEncoder, READER_PROGRAMMING, ascii, planner::optimize, write_eci_codewords,
};

use super::{SymbolList, SymbolSize};

#[cfg(test)]
use pretty_assertions::assert_eq;

/// Bir Data Matrix mesajındaki veri ve ASCII-mode denetim bölümleri.
///
/// Bu yapı, ECI'nin mesaj içinde birden fazla kez veya veri sonrasında; FNC1'in
/// ise alan ayırıcı olarak sonraki symbol karakteri konumlarında encode edilmesini
/// sağlar. `ReaderProgramming` yalnızca ilk codeword olabilir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataMatrixSegment<'a> {
    /// Varsayılan veya en son seçilmiş ECI altında yorumlanacak byte'lar.
    Data(&'a [u8]),
    /// 0..=999999 aralığındaki ECI assignment numarasını etkinleştirir.
    Eci(u32),
    /// FNC1 codeword yazar. İlk/ikinci konumda format bayrağı, daha sonra GS alan
    /// ayırıcıdır (7.2.4.7).
    Fnc1,
    /// Reader Programming mesajını işaretler; yalnızca ilk codeword olabilir.
    ReaderProgramming,
}

/// Input'u Data Matrix data codeword'lerine encode eder.
pub fn encode_data(
    data: &[u8],
    symbol_list: &SymbolList,
    eci: Option<u32>,
    enabled_modes: impl Into<FlagSet<EncodationType>>,
    use_macros: bool,
) -> Result<(Vec<u8>, SymbolSize), DataEncodingError> {
    encode_data_internal(
        data,
        symbol_list,
        eci,
        enabled_modes,
        use_macros,
        None,
        false,
    )
}

/// Denetim bölümleri içeren bir mesajı Data Matrix data codeword'lerine encode eder.
///
/// ECI yalnızca ASCII encodation'dan çağrılabildiği için her [`Data`](DataMatrixSegment::Data)
/// bölümü ASCII scheme ile encode edilir. Rakam çiftleri ve extended ASCII'nin
/// standart compaction kuralları uygulanır; rakam çifti ardışık `Data`
/// bölümlerinin sınırından da devam eder, araya giren ECI/FNC1 gibi denetim
/// codeword'leri çifti böler. Tek ve başlangıç ECI'li veri için bütün mode'ları
/// optimize eden [`encode_data`] daha küçük sonuç verebilir.
pub fn encode_data_segments(
    segments: &[DataMatrixSegment<'_>],
    symbol_list: &SymbolList,
) -> Result<(Vec<u8>, SymbolSize), DataEncodingError> {
    encode_data_segments_internal(segments, symbol_list, None, false)
}

pub(crate) fn encode_data_segments_internal(
    segments: &[DataMatrixSegment<'_>],
    symbol_list: &SymbolList,
    fnc1: Option<Fnc1Position>,
    reader_programming: bool,
) -> Result<(Vec<u8>, SymbolSize), DataEncodingError> {
    if symbol_list.is_empty() {
        return Err(DataEncodingError::SymbolListEmpty);
    }

    let mut codewords = Vec::new();
    if reader_programming {
        codewords.push(READER_PROGRAMMING);
    }

    let mut fnc1_second_pending = false;
    match fnc1 {
        Some(Fnc1Position::First) if reader_programming => {
            return Err(DataEncodingError::InvalidControlPosition(
                "FNC1 ve Reader Programming aynı ilk konumu kullanamaz",
            ));
        }
        Some(Fnc1Position::First) => codewords.push(ascii::FNC1),
        Some(Fnc1Position::Second) if reader_programming => codewords.push(ascii::FNC1),
        Some(Fnc1Position::Second) => fnc1_second_pending = true,
        None => (),
    }

    // Ardışık Data bölümleri tek ASCII akışı oluşturur: bölüm sonunda eşlenmemiş
    // kalan tek rakam, bir sonraki bölümün ilk rakamıyla çift codeword yapabilir.
    let mut pending_digit: Option<u8> = None;
    for segment in segments {
        match *segment {
            DataMatrixSegment::Data(mut bytes) => {
                if bytes.is_empty() {
                    continue;
                }
                if fnc1_second_pending {
                    let consumed = write_first_ascii_codeword(bytes, &mut codewords)?;
                    let Some(rest) = bytes.get(consumed..) else {
                        crate::invariant_violation(
                            "ilk ASCII codeword'ün tükettiği byte sayısı input'u aştı",
                        );
                    };
                    bytes = rest;
                    codewords.push(ascii::FNC1);
                    fnc1_second_pending = false;
                }
                if let Some(digit) = pending_digit.take() {
                    match bytes {
                        [next @ b'0'..=b'9', rest @ ..] => {
                            codewords.push((digit - b'0') * 10 + (next - b'0') + 130);
                            bytes = rest;
                        }
                        _ => codewords.push(digit + 1),
                    }
                }
                pending_digit = write_ascii_codewords(bytes, &mut codewords);
            }
            DataMatrixSegment::Eci(eci) => {
                if fnc1_second_pending {
                    return Err(DataEncodingError::InvalidControlPosition(
                        "ikinci konumdaki FNC1'den önce ECI kullanılamaz",
                    ));
                }
                flush_pending_digit(&mut pending_digit, &mut codewords);
                write_eci_codewords(&mut codewords, eci)?;
            }
            DataMatrixSegment::Fnc1 => {
                if fnc1_second_pending {
                    return Err(DataEncodingError::InvalidControlPosition(
                        "ikinci konumdaki FNC1 için önce bir ASCII data codeword gerekir",
                    ));
                }
                flush_pending_digit(&mut pending_digit, &mut codewords);
                codewords.push(ascii::FNC1);
            }
            DataMatrixSegment::ReaderProgramming => {
                flush_pending_digit(&mut pending_digit, &mut codewords);
                if fnc1_second_pending || !codewords.is_empty() {
                    return Err(DataEncodingError::InvalidControlPosition(
                        "Reader Programming ilk codeword olmalıdır",
                    ));
                }
                codewords.push(READER_PROGRAMMING);
            }
        }
    }

    if fnc1_second_pending {
        return Err(DataEncodingError::InvalidControlPosition(
            "ikinci konumdaki FNC1 için bir ASCII data codeword gerekir",
        ));
    }

    flush_pending_digit(&mut pending_digit, &mut codewords);
    pad_ascii_codewords(codewords, symbol_list)
}

fn flush_pending_digit(pending_digit: &mut Option<u8>, codewords: &mut Vec<u8>) {
    if let Some(digit) = pending_digit.take() {
        codewords.push(digit + 1);
    }
}

fn write_first_ascii_codeword(
    data: &[u8],
    codewords: &mut Vec<u8>,
) -> Result<usize, DataEncodingError> {
    match data {
        [a, b, ..] if a.is_ascii_digit() && b.is_ascii_digit() => {
            codewords.push((a - b'0') * 10 + (b - b'0') + 130);
            Ok(2)
        }
        [ch @ 0..=127, ..] => {
            codewords.push(ch + 1);
            Ok(1)
        }
        _ => Err(DataEncodingError::InvalidControlPosition(
            "ikinci konumdaki FNC1 öncesindeki veri tek ASCII codeword olmalıdır",
        )),
    }
}

/// Sondaki eşlenmemiş tek rakamı yazmaz; bir sonraki `Data` bölümüyle rakam
/// çifti oluşturabilmesi için çağırana geri verir.
fn write_ascii_codewords(mut data: &[u8], codewords: &mut Vec<u8>) -> Option<u8> {
    loop {
        match data {
            [a, b, rest @ ..] if a.is_ascii_digit() && b.is_ascii_digit() => {
                codewords.push((a - b'0') * 10 + (b - b'0') + 130);
                data = rest;
            }
            [digit @ b'0'..=b'9'] => return Some(*digit),
            [ch, rest @ ..] => {
                match ch {
                    0..=127 => codewords.push(ch + 1),
                    128..=255 => {
                        codewords.push(ascii::UPPER_SHIFT);
                        codewords.push(ch - 127);
                    }
                }
                data = rest;
            }
            [] => return None,
        }
    }
}

fn pad_ascii_codewords(
    mut codewords: Vec<u8>,
    symbol_list: &SymbolList,
) -> Result<(Vec<u8>, SymbolSize), DataEncodingError> {
    let size = symbol_list
        .first_symbol_big_enough_for(codewords.len())
        .ok_or(DataEncodingError::TooMuchOrIllegalData)?;
    let mut left = size.num_data_codewords() - codewords.len();
    if left > 0 {
        codewords.push(ascii::PAD);
        left -= 1;
    }
    for _ in 0..left {
        let pos = codewords.len() + 1;
        let pseudo_random = (((149 * pos) % 253) + 1) as u16;
        let randomized = ascii::PAD as u16 + pseudo_random;
        codewords.push(if randomized <= 254 {
            randomized as u8
        } else {
            (randomized - 254) as u8
        });
    }
    Ok((codewords, size))
}

pub(crate) fn encode_data_internal(
    data: &[u8],
    symbol_list: &SymbolList,
    eci: Option<u32>,
    enabled_modes: impl Into<FlagSet<EncodationType>>,
    use_macros: bool,
    fnc1: Option<Fnc1Position>,
    reader_programming: bool,
) -> Result<(Vec<u8>, SymbolSize), DataEncodingError> {
    let mut encoder = GenericDataEncoder::with_size(data, symbol_list, enabled_modes.into(), fnc1);
    if reader_programming {
        encoder.write_reader_programming()?;
    }
    // Macro'lar ilk symbol karakteri konumunu kullanır (7.2.4.8); FNC1 ile
    // birlikte kullanılamazlar.
    if use_macros && fnc1.is_none() {
        encoder.use_macro_if_possible();
    }
    if fnc1 == Some(Fnc1Position::Second) {
        encoder.write_fnc1_second()?;
    }
    if let Some(eci) = eci {
        encoder.write_eci(eci)?;
    }
    encoder.codewords()
}

/// Encoder'ın padding öncesinde ürettiği data codeword sayısı.
///
/// `planner::optimize_cost` ile karşılaştırılabilmesi için varsayılan
/// [`encode_data`] yolu ile aynı seçenekleri kullanır: ECI, macro ve FNC1 yoktur.
#[cfg(test)]
pub(crate) fn encode_data_unpadded_len(
    data: &[u8],
    symbol_list: &SymbolList,
    enabled_modes: impl Into<FlagSet<EncodationType>>,
) -> Option<usize> {
    let mut encoder = GenericDataEncoder::with_size(data, symbol_list, enabled_modes.into(), None);
    encoder.unpadded_len().ok()
}

/// Data encoding sırasında encodation türlerinin ne zaman değiştirileceğini planlar.
///
/// `data` verilen `symbol_size` içine sığmazsa `None` döndürür. Aksi halde mode'un
/// ne zaman değiştirileceğini açıklayan `(usize, EncodationType)` tuple'larından
/// oluşan bir vektör döndürür. Tuple'ın ilk öğesi planlanan mode switch noktasında
/// kalan input karakteri sayısıdır. Örneğin `(20, EncodationType::C40)`, encode
/// edilecek yalnızca 20 karakter kaldığında C40'a geçileceğini belirtir.
///
/// Plan, en küçük encoding boyutunu elde edecek biçimde seçilir. Birden fazla
/// çözüm varsa önce mode'ların "karmaşıklığına", ardından mode switch sayısına
/// göre filtreleme yapılır. Hâlâ birden fazla seçenek varsa döndürülen plan bir
/// implementasyon ayrıntısıdır.
///
/// # Örnek
///
/// ```rust
/// # use datamatrix::{data::encodation_plan, EncodationType, SymbolList};
/// encodation_plan(b"Hello!", &SymbolList::default(), EncodationType::all());
/// encodation_plan(b"Hello!", &SymbolList::default(), EncodationType::Ascii | EncodationType::Edifact);
/// ```
pub fn encodation_plan(
    data: &[u8],
    symbol_list: &SymbolList,
    enabled_modes: impl Into<FlagSet<EncodationType>>,
) -> Option<Vec<(usize, EncodationType)>> {
    optimize(
        data,
        0,
        EncodationType::Ascii,
        symbol_list,
        enabled_modes.into(),
    )
}

/// UTF-8 encoded string'i Latin-1'e dönüştürmeyi dener.
pub fn utf8_to_latin1(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len());
    for ch in s.chars() {
        out.push(u8::try_from(u32::from(ch)).ok()?);
    }
    Some(out)
}

/// Latin-1 encoded string'i UTF-8 string'e dönüştürür.
///
/// ISO/IEC 8859-1'in C0/C1 kontrol karakterleri dahil bütün byte değerleri
/// geçerlidir; bu nedenle dönüşüm yalnızca API tutarlılığı için `Option` döndürür.
pub fn latin1_to_utf8(latin1: &[u8]) -> Option<String> {
    let mut out = String::with_capacity(latin1.len());
    latin1_to_utf8_mut(latin1, &mut out)?;
    Some(out)
}

pub(crate) fn latin1_to_utf8_mut(latin1: &[u8], out: &mut String) -> Option<()> {
    for ch in latin1.iter().copied() {
        out.push(char::from(ch));
    }
    Some(())
}

#[test]
fn test_macro() -> Result<(), DataEncodingError> {
    use crate::encodation::{MACRO05, MACRO06, ascii::PAD};
    use alloc::vec;

    let macro05 = encode_data(
        b"[)>\x1E05\x1D01\x1E\x04",
        &SymbolList::default(),
        None,
        EncodationType::all(),
        true,
    )?;
    assert_eq!(macro05.0, vec![MACRO05, 130 + 1, PAD]);
    let macro06 = encode_data(
        b"[)>\x1E06\x1D11\x1E\x04",
        &SymbolList::default(),
        None,
        EncodationType::all(),
        true,
    )?;
    assert_eq!(macro06.0, vec![MACRO06, 130 + 11, PAD]);
    Ok(())
}

#[test]
fn macro_body_backup_never_reintroduces_the_trailer() -> Result<(), DataEncodingError> {
    use alloc::vec;

    for (head, macro_codeword) in [
        (crate::encodation::MACRO05_HEAD, crate::encodation::MACRO05),
        (crate::encodation::MACRO06_HEAD, crate::encodation::MACRO06),
    ] {
        for len in 0..=64 {
            let mut input = Vec::new();
            input.extend_from_slice(head);
            input.extend_from_slice(&vec![b'a'; len]);
            input.extend_from_slice(crate::encodation::MACRO_TRAIL);

            let (codewords, _) = encode_data(
                &input,
                &SymbolList::default(),
                None,
                EncodationType::all(),
                true,
            )?;
            assert_eq!(codewords.first(), Some(&macro_codeword), "body len={len}");
            assert_eq!(decode_data(&codewords), Ok(input), "body len={len}");
        }
    }
    Ok(())
}

#[test]
fn segment_boundaries_keep_digit_pair_compaction() -> Result<(), DataEncodingError> {
    // "1" + "" + "23" tek ASCII akışı gibi çiftlenir: (1, 2) çifti ve tek '3'.
    let (codewords, _) = encode_data_segments(
        &[
            DataMatrixSegment::Data(b"1"),
            DataMatrixSegment::Data(b""),
            DataMatrixSegment::Data(b"23"),
        ],
        &SymbolList::default(),
    )?;
    assert_eq!(codewords.get(..2), Some([142, b'3' + 1].as_slice()));
    assert_eq!(decode_data(&codewords), Ok(b"123".to_vec()));

    // Denetim codeword'leri alan sınırıdır; FNC1 üzerinden rakam çifti kurulmaz.
    let (codewords, _) = encode_data_segments(
        &[
            DataMatrixSegment::Data(b"A1"),
            DataMatrixSegment::Fnc1,
            DataMatrixSegment::Data(b"2"),
        ],
        &SymbolList::default(),
    )?;
    assert_eq!(
        codewords.get(..4),
        Some([b'A' + 1, b'1' + 1, 232, b'2' + 1].as_slice())
    );
    assert_eq!(decode_data(&codewords), Ok([b'A', b'1', 29, b'2'].to_vec()));
    Ok(())
}

#[test]
fn latin1_helpers_cover_all_byte_values() -> Result<(), &'static str> {
    let bytes: Vec<u8> = (0..=u8::MAX).collect();
    let text = latin1_to_utf8(&bytes).ok_or("ISO/IEC 8859-1 dönüşümü başarısız")?;
    assert_eq!(utf8_to_latin1(&text), Some(bytes));
    assert_eq!(decode_str(&[11]), Ok("\n".into()));
    Ok(())
}
