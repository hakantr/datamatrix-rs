//! Data decodation. Bu aşama error correction ve görsel algılamadan sonra gelir.
//!
//! `encodation` modülünün ters işlemini uygular.
use super::encodation::{
    EncodationType, Fnc1Position, MACRO_TRAIL, MACRO05, MACRO05_HEAD, MACRO06, MACRO06_HEAD,
    READER_PROGRAMMING, UNLATCH, ascii, edifact,
};
use alloc::{string::String, vec::Vec};

#[cfg(test)]
use alloc::vec;

#[cfg(test)]
mod tests;

#[cfg(test)]
use pretty_assertions::assert_eq;

mod eci;

pub(crate) use eci::ECI_UTF8;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data bölümü decode edilirken oluşan hata.
pub enum DataDecodingError {
    UnexpectedCharacter(&'static str, u8),
    NotImplemented(&'static str),
    UnexpectedEnd,
    CharsetError,
    /// Ham data decoding sırasında ECI code desteklenmez.
    ECICode,
    /// Reader Programming mesajı host verisi olarak döndürülemez.
    ReaderProgrammingMessage,
}

impl core::fmt::Display for DataDecodingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnexpectedCharacter(mode, codeword) => {
                write!(f, "{mode} içinde beklenmeyen codeword: {codeword}")
            }
            Self::NotImplemented(feature) => {
                write!(f, "desteklenmeyen Data Matrix özelliği: {feature}")
            }
            Self::UnexpectedEnd => f.write_str("Data Matrix verisi beklenmedik biçimde sona erdi"),
            Self::CharsetError => f.write_str("ECI karakter kümesi dönüşümü başarısız"),
            Self::ECICode => f.write_str("ham byte çıktısında ECI codeword desteklenmiyor"),
            Self::ReaderProgrammingMessage => f.write_str(
                "Reader Programming mesajı host verisi değildir; metadata için decode_message kullanın",
            ),
        }
    }
}

impl core::error::Error for DataDecodingError {}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Reader<'a>(&'a [u8], usize);

impl<'a> Reader<'a> {
    fn pos(&self) -> usize {
        self.1 + 1
    }

    fn eat(&mut self) -> Result<u8, DataDecodingError> {
        if let Some((ch, rest)) = self.0.split_first() {
            self.1 += 1;
            self.0 = rest;
            Ok(*ch)
        } else {
            Err(DataDecodingError::UnexpectedEnd)
        }
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn peek(&self, i: usize) -> Option<u8> {
        self.0.get(i).copied()
    }
}

/// Data Matrix'in data codeword'lerini decode eder.
pub fn decode_data(data: &[u8]) -> Result<Vec<u8>, DataDecodingError> {
    let parts = decode_parts(data, true)?;
    if parts.reader_programming {
        Err(DataDecodingError::ReaderProgrammingMessage)
    } else if !parts.eci_spans.is_empty() {
        Err(DataDecodingError::ECICode)
    } else {
        Ok(parts.output)
    }
}

struct DecodedParts {
    output: Vec<u8>,
    eci_spans: Vec<(usize, u32)>,
    fnc1: Option<Fnc1Position>,
    reader_programming: bool,
}

fn decode_parts(data: &[u8], raw: bool) -> Result<DecodedParts, DataDecodingError> {
    let mut data = Reader(data, 0);
    let mut mode = EncodationType::Ascii;
    let mut out = Vec::with_capacity(data.len());
    let mut ecis = Vec::new();
    let mut fnc1 = None;

    let reader_programming = if data.peek(0) == Some(READER_PROGRAMMING) {
        data.eat()?;
        true
    } else {
        false
    };

    let add_macro_trail = match (reader_programming, data.peek(0)) {
        (false, Some(MACRO05)) => {
            out.extend_from_slice(MACRO05_HEAD);
            data.eat()?;
            true
        }
        (false, Some(MACRO06)) => {
            out.extend_from_slice(MACRO06_HEAD);
            data.eat()?;
            true
        }
        _ => false,
    };

    if !raw && add_macro_trail {
        ecis.push((0, ECI_UTF8));
        ecis.push((out.len(), 0));
    }

    while !data.is_empty() {
        let (rest, new_mode) = match mode {
            EncodationType::Ascii => decode_ascii(data, &mut out, &mut ecis, &mut fnc1)?,
            EncodationType::Base256 => decode_base256(data, &mut out)?,
            EncodationType::X12 => decode_x12(data, &mut out)?,
            EncodationType::Edifact => decode_edifact(data, &mut out)?,
            EncodationType::C40 => decode_c40_like(data, &mut out, BASE_C40, SHIFT3_C40)?,
            EncodationType::Text => decode_c40_like(data, &mut out, BASE_TEXT, SHIFT3_TEXT)?,
        };
        data = rest;
        mode = new_mode;
    }

    if add_macro_trail {
        if !ecis.is_empty() {
            ecis.push((out.len(), ECI_UTF8));
        }
        out.extend_from_slice(MACRO_TRAIL);
    }

    Ok(DecodedParts {
        output: out,
        eci_spans: ecis,
        fnc1,
        reader_programming,
    })
}

/// Data Matrix'in data codeword'lerini string olarak decode eder.
///
/// Bu fonksiyon sınırlı ECI desteğine sahiptir. ECI yoksa Latin-1 encoding
/// kullanıldığı varsayılır.
pub fn decode_str(data: &[u8]) -> Result<String, DataDecodingError> {
    let parts = decode_parts(data, false)?;
    if parts.reader_programming {
        return Err(DataDecodingError::ReaderProgrammingMessage);
    }
    eci::convert(&parts.output, &parts.eci_spans)
}

/// Decode edilmiş Data Matrix mesajı ve ISO/IEC 16022:2024 Clause 12 iletim
/// protokolü için gereken metadata.
///
/// [decode_data] yalnızca normal host mesajlarının ham byte'larını döndürür; bu
/// yapı ek olarak FNC1 konumunu, ECI bölümlerini ve Reader Programming işaretini
/// korur, böylece Annex H symbology identifier ve Clause 12 iletim formatı
/// üretilebilir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedMessage {
    data: Vec<u8>,
    eci_spans: Vec<(usize, u32)>,
    fnc1: Option<Fnc1Position>,
    reader_programming: bool,
}

impl DecodedMessage {
    /// Decode edilmiş veri byte'larını döndürür.
    ///
    /// Macro başlık/kuyrukları açılmış, alan ayırıcı FNC1'ler GS (ASCII 29)
    /// karakterine dönüştürülmüş haldedir. Format bayrağı olan FNC1 ve ECI
    /// escape dizileri veri içinde yer almaz; bunlara [fnc1()](Self::fnc1),
    /// [eci_spans()](Self::eci_spans) ve [transmission()](Self::transmission)
    /// üzerinden erişilir.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Symbol'de format bayrağı olarak bulunan FNC1'in konumunu döndürür.
    pub fn fnc1(&self) -> Option<Fnc1Position> {
        self.fnc1
    }

    /// `(veri offset'i, ECI numarası)` çiftlerini artan offset sırasında döndürür.
    ///
    /// Her ECI, [data()](Self::data) çıktısında verilen offset'ten itibaren, bir
    /// sonraki ECI'ye veya verinin sonuna kadar geçerlidir (7.3.3).
    pub fn eci_spans(&self) -> &[(usize, u32)] {
        &self.eci_spans
    }

    /// Symbol'de en az bir ECI codeword'ü bulunup bulunmadığını döndürür.
    pub fn has_eci(&self) -> bool {
        !self.eci_spans.is_empty()
    }

    /// Symbol'ün Reader Programming mesajı olup olmadığını döndürür.
    ///
    /// Bu mesajlar okuyucuyu programlamak içindir ve host verisi olarak
    /// iletilmemelidir (7.2.4.10).
    pub fn is_reader_programming(&self) -> bool {
        self.reader_programming
    }

    /// ISO/IEC 16022:2024 Annex H symbology identifier değerini döndürür.
    ///
    /// ISO/IEC 15424 uyarınca iletilen verinin önüne eklenmesi gereken `]dm`
    /// önekidir; `m` seçenek değeri Table H.1'e göre belirlenir. ECI veya format
    /// bayrağı FNC1 içeren symbol'lerde bu önekin iletimi zorunludur (12.6).
    pub fn symbology_identifier(&self) -> &'static str {
        match (self.fnc1, self.has_eci()) {
            (None, false) => "]d1",
            (Some(Fnc1Position::First), false) => "]d2",
            (Some(Fnc1Position::Second), false) => "]d3",
            (None, true) => "]d4",
            (Some(Fnc1Position::First), true) => "]d5",
            (Some(Fnc1Position::Second), true) => "]d6",
        }
    }

    /// ISO/IEC 16022:2024 Clause 12 uyarınca iletilecek byte dizisini üretir.
    ///
    /// Çıktı, Annex H symbology identifier ile başlar. Symbol ECI içeriyorsa
    /// 12.5'teki escape protokolü uygulanır: her ECI `\nnnnnn` olarak iletilir
    /// ve verideki her backslash (92) iki kez yazılır. Macro başlık/kuyrukları
    /// identifier'dan sonra, verinin parçası olarak yer alır (12.4). Reader
    /// Programming mesajları host'a iletilmediğinden bu durumda boş vektör döner.
    pub fn transmission(&self) -> Vec<u8> {
        if self.reader_programming {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(self.data.len() + 16);
        out.extend_from_slice(self.symbology_identifier().as_bytes());
        if self.eci_spans.is_empty() {
            out.extend_from_slice(&self.data);
            return out;
        }
        let mut spans = self.eci_spans.iter().peekable();
        for (i, byte) in self.data.iter().copied().enumerate() {
            while spans.peek().is_some_and(|(offset, _)| *offset == i) {
                let Some((_, eci)) = spans.next() else {
                    crate::invariant_violation("ECI span iterator peek sonrası boş");
                };
                push_eci_escape(&mut out, *eci);
            }
            if byte == b'\\' {
                out.push(byte);
            }
            out.push(byte);
        }
        // Verinin sonunda bildirilen ECI'ler de iletilir.
        for (_, eci) in spans {
            push_eci_escape(&mut out, *eci);
        }
        out
    }
}

/// ECI escape dizisini `\nnnnnn` biçiminde yazar (12.5).
fn push_eci_escape(out: &mut Vec<u8>, eci: u32) {
    out.push(b'\\');
    let mut digits = [0u8; 6];
    let mut value = eci;
    for digit in digits.iter_mut().rev() {
        *digit = b'0' + (value % 10) as u8;
        value /= 10;
    }
    out.extend_from_slice(&digits);
}

/// Data Matrix'in data codeword'lerini iletim metadata'sıyla birlikte decode eder.
///
/// [decode_data] fonksiyonundan farklı olarak ECI veya Reader Programming içeren
/// symbol'leri reddetmez; denetim metadata'sını [DecodedMessage] içinde döndürür.
pub fn decode_message(data: &[u8]) -> Result<DecodedMessage, DataDecodingError> {
    let parts = decode_parts(data, true)?;
    Ok(DecodedMessage {
        data: parts.output,
        eci_spans: parts.eci_spans,
        fnc1: parts.fnc1,
        reader_programming: parts.reader_programming,
    })
}

fn derandomize_253_state(ch: u8, pos: usize) -> u8 {
    let pseudo_random = ((149 * pos) % 253) + 1;
    let tmp = ch as i16 - pseudo_random as i16;
    if tmp >= 1 {
        tmp as u8
    } else {
        (tmp + 254) as u8
    }
}

fn read_eci(mut data: Reader) -> Result<(Reader, u32), DataDecodingError> {
    let mut ch1 = data.eat()?;
    let eci = match ch1 {
        1..=127 => ch1 as u32 - 1,
        128..=191 => {
            let mut ch2 = data.eat()?;
            if !matches!(ch2, 1..=254) {
                return Err(DataDecodingError::UnexpectedCharacter(
                    "ECI sonrasındaki ikinci codeword",
                    ch2,
                ));
            }
            ch2 -= 1;
            ch1 -= 128;
            (ch1 as u32) * 254 + ch2 as u32 + 127
        }
        192..=207 => {
            let mut ch2 = data.eat()?;
            if !matches!(ch2, 1..=254) {
                return Err(DataDecodingError::UnexpectedCharacter(
                    "ECI sonrasındaki ikinci codeword",
                    ch2,
                ));
            }
            let mut ch3 = data.eat()?;
            if !matches!(ch3, 1..=254) {
                return Err(DataDecodingError::UnexpectedCharacter(
                    "ECI sonrasındaki üçüncü codeword",
                    ch3,
                ));
            }
            ch1 -= 192;
            ch2 -= 1;
            ch3 -= 1;
            (ch1 as u32) * 64516 + (ch2 as u32) * 254 + ch3 as u32 + 16383
        }
        _ => {
            return Err(DataDecodingError::UnexpectedCharacter(
                "ECI sonrasındaki ilk codeword",
                ch1,
            ));
        }
    };
    Ok((data, eci))
}

fn decode_ascii<'a>(
    mut data: Reader<'a>,
    out: &mut Vec<u8>,
    ecis: &mut Vec<(usize, u32)>,
    fnc1: &mut Option<Fnc1Position>,
) -> Result<(Reader<'a>, EncodationType), DataDecodingError> {
    let mut upper_shift = false;
    while let Ok(ch) = data.eat() {
        if upper_shift && !matches!(ch, 1..=128) {
            return Err(DataDecodingError::UnexpectedCharacter(
                "ASCII 'Upper Shift' sonrasındaki karakter 1..=128 aralığında değil",
                ch,
            ));
        }
        match ch {
            ch @ 1..=128 => {
                if upper_shift {
                    out.push(ch + 127);
                    upper_shift = false;
                } else {
                    out.push(ch - 1);
                }
            }
            ascii::PAD => {
                // Kalan veriyi tüketir ve padding biçimini denetler.
                while let Ok(ch) = data.eat() {
                    let ch = derandomize_253_state(ch, data.pos() - 1);
                    if ch != ascii::PAD {
                        return Err(DataDecodingError::UnexpectedCharacter(
                            "padding alanında padding olmayan karakter",
                            ch,
                        ));
                    }
                }
                return Ok((data, EncodationType::Ascii));
            }
            ch @ 130..=229 => {
                let digit = ch - 130;
                out.push(b'0' + (digit / 10));
                out.push(b'0' + (digit % 10));
            }
            ascii::LATCH_C40 => return Ok((data, EncodationType::C40)),
            ascii::LATCH_BASE256 => return Ok((data, EncodationType::Base256)),
            ascii::FNC1 => {
                // 7.2.4.7 ve 12.2/12.3: ilk veya ikinci symbol karakteri
                // konumundaki ilk FNC1 format bayrağıdır ve veri olarak
                // iletilmez. Diğer bütün FNC1'ler alan ayırıcıdır ve GS
                // (ASCII 29) olarak iletilir.
                let position = data.pos() - 1;
                if fnc1.is_none() && position <= 2 {
                    *fnc1 = Some(if position == 1 {
                        Fnc1Position::First
                    } else {
                        Fnc1Position::Second
                    });
                } else {
                    out.push(29);
                }
            }
            233 => return Err(DataDecodingError::NotImplemented("Structured Append")),
            READER_PROGRAMMING => {
                return Err(DataDecodingError::UnexpectedCharacter(
                    "Reader Programming yalnızca ilk codeword olabilir",
                    READER_PROGRAMMING,
                ));
            }
            ascii::UPPER_SHIFT => {
                upper_shift = true;
            }
            ascii::LATCH_X12 => return Ok((data, EncodationType::X12)),
            ascii::LATCH_TEXT => return Ok((data, EncodationType::Text)),
            ascii::LATCH_EDIFACT => return Ok((data, EncodationType::Edifact)),
            ascii::ECI => {
                let (rest, eci) = read_eci(data)?;
                data = rest;
                ecis.push((out.len(), eci));
            }
            ch => {
                return Err(DataDecodingError::UnexpectedCharacter(
                    "ASCII içinde geçersiz",
                    ch,
                ));
            }
        }
    }
    if upper_shift {
        return Err(DataDecodingError::UnexpectedEnd);
    }
    Ok((data, EncodationType::Ascii))
}

fn derandomize_255_state(ch: u8, pos: usize) -> u8 {
    let pseudo_random = ((149 * pos) % 255) + 1;
    let tmp = ch as i16 - pseudo_random as i16;
    if tmp >= 0 {
        tmp as u8
    } else {
        (tmp + 256) as u8
    }
}

fn decode_base256<'a>(
    mut data: Reader<'a>,
    out: &mut Vec<u8>,
) -> Result<(Reader<'a>, EncodationType), DataDecodingError> {
    let length = if let Ok(ch1) = data.eat() {
        let ch1 = derandomize_255_state(ch1, data.pos() - 1) as usize;
        if ch1 == 0 {
            data.len()
        } else if ch1 < 250 {
            ch1
        } else {
            let ch2 = data.eat()?;
            let ch2 = derandomize_255_state(ch2, data.pos() - 1) as usize;
            250 * (ch1 - 249) + ch2
        }
    } else {
        return Err(DataDecodingError::UnexpectedEnd);
    };
    for _ in 0..length {
        if let Ok(ch) = data.eat() {
            out.push(derandomize_255_state(ch, data.pos() - 1));
        } else {
            return Err(DataDecodingError::UnexpectedEnd);
        }
    }
    Ok((data, EncodationType::Ascii))
}

fn dec_edifcat_char(ch: u8) -> u8 {
    if (ch & 0b10_0000) != 0 {
        ch
    } else {
        ch | 0b0100_0000
    }
}

fn decode_edifact<'a>(
    mut data: Reader<'a>,
    out: &mut Vec<u8>,
) -> Result<(Reader<'a>, EncodationType), DataDecodingError> {
    while !data.is_empty() {
        if data.len() <= 2 {
            // Kalan veri ASCII olarak encode edilmiştir.
            break;
        }
        if data.peek(0).is_some_and(|ch| ch >> 2 == edifact::UNLATCH) {
            data.eat()?;
            break;
        }
        let mut chunk: u32 = (data.eat()? as u32) << 16;
        let val = (chunk >> 18) as u8;
        if val == edifact::UNLATCH {
            break;
        }
        out.push(dec_edifcat_char(val));

        if let Ok(ch) = data.eat() {
            chunk |= (ch as u32) << 8;
            let val = ((chunk >> 12) & 0b11_1111) as u8;
            if val == edifact::UNLATCH {
                break;
            }
            out.push(dec_edifcat_char(val));

            if let Ok(ch) = data.eat() {
                chunk |= ch as u32;
                let val = ((chunk >> 6) & 0b11_1111) as u8;
                if val == edifact::UNLATCH {
                    break;
                }
                out.push(dec_edifcat_char(val));

                let val = (chunk & 0b11_1111) as u8;
                if val == edifact::UNLATCH {
                    break;
                }
                out.push(dec_edifcat_char(val));
            }
        }
    }
    Ok((data, EncodationType::Ascii))
}

fn decode_c40_tuple(a: u8, b: u8) -> Result<(u8, u8, u8), DataDecodingError> {
    let encoded = ((a as u16) << 8) + b as u16;
    // 7.2.5.3: tuple formülü yalnızca 1..=64_000 aralığını üretir. Özellikle
    // (0, 0) için önce çıkarma yapmak debug build'de panic'e yol açıyordu.
    if !(1..=64_000).contains(&encoded) {
        return Err(DataDecodingError::UnexpectedCharacter(
            "C40/Text/X12 codeword çiftinin 16-bit değeri 1..=64000 aralığında değil",
            a,
        ));
    }
    let mut full = encoded - 1;
    let tmp = full / 1600;
    let c1 = tmp as u8;
    full -= tmp * 1600;
    let tmp = full / 40;
    Ok((c1, tmp as u8, (full - tmp * 40) as u8))
}

fn dec_x12_val(ch: u8) -> Result<u8, DataDecodingError> {
    match ch {
        0 => Ok(13),
        1 => Ok(42),
        2 => Ok(62),
        3 => Ok(b' '),
        ch @ 4..=13 => Ok(b'0' + (ch - 4)),
        ch @ 14..=39 => Ok(b'A' + (ch - 14)),
        ch => Err(DataDecodingError::UnexpectedCharacter(
            "X12 içinde geçersiz",
            ch,
        )),
    }
}

fn decode_x12<'a>(
    mut data: Reader<'a>,
    out: &mut Vec<u8>,
) -> Result<(Reader<'a>, EncodationType), DataDecodingError> {
    while data.len() > 1 {
        let first = data.eat()?;
        if first == UNLATCH {
            break;
        }
        let second = data.eat()?;
        let (c1, c2, c3) = decode_c40_tuple(first, second)?;

        out.push(dec_x12_val(c1)?);
        out.push(dec_x12_val(c2)?);
        out.push(dec_x12_val(c3)?);
    }
    if data.len() == 1 && data.peek(0) == Some(UNLATCH) {
        // End of data noktasında tek UNLATCH
        data.eat()?;
    }
    Ok((data, EncodationType::Ascii))
}

const BASE_C40: &[u8; 37] = b" 0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const SHIFT3_C40: &[u8; 32] = b"`abcdefghijklmnopqrstuvwxyz{|}~\x7f";

const BASE_TEXT: &[u8; 37] = b" 0123456789abcdefghijklmnopqrstuvwxyz";
const SHIFT3_TEXT: &[u8; 32] = b"`ABCDEFGHIJKLMNOPQRSTUVWXYZ{|}~\x7f";

const SHIFT2: &[u8] = b"!\"#$%&'()*+,-./:;<=>?@[\\]^_";

fn decode_c40_like<'a>(
    mut data: Reader<'a>,
    out: &mut Vec<u8>,
    map_base: &[u8; 37],
    map_shift3: &[u8; 32],
) -> Result<(Reader<'a>, EncodationType), DataDecodingError> {
    let mut shift = 0;
    let mut upper_shift = false;
    let mut explicit_unlatch = false;
    while data.len() > 1 {
        let first = data.eat()?;
        if first == UNLATCH {
            explicit_unlatch = true;
            break;
        }
        let (c1, c2, c3) = decode_c40_tuple(first, data.eat()?)?;
        for ch in [c1, c2, c3].iter().copied() {
            if shift == 0 {
                match ch {
                    ch @ 0..=2 => shift = ch + 1,
                    ch @ 3..=39 => {
                        let text = map_base.get(usize::from(ch - 3)).copied().ok_or(
                            DataDecodingError::UnexpectedCharacter(
                                "C40/Text base set içinde geçersiz",
                                ch,
                            ),
                        )?;
                        if upper_shift {
                            out.push(text + 128);
                            upper_shift = false;
                        } else {
                            out.push(text);
                        }
                    }
                    ch => {
                        return Err(DataDecodingError::UnexpectedCharacter(
                            "C40/Text base set içinde geçersiz",
                            ch,
                        ));
                    }
                }
            } else if shift == 1 {
                match ch {
                    ch @ 0..=31 => {
                        if upper_shift {
                            out.push(ch + 128);
                            upper_shift = false;
                        } else {
                            out.push(ch);
                        }
                    }
                    ch => {
                        return Err(DataDecodingError::UnexpectedCharacter(
                            "C40/Text shift1 set içinde geçersiz",
                            ch,
                        ));
                    }
                }
                shift = 0;
            } else if shift == 2 {
                match ch {
                    ch @ 0..=26 => {
                        let text = SHIFT2.get(usize::from(ch)).copied().ok_or(
                            DataDecodingError::UnexpectedCharacter(
                                "C40/Text shift2 set içinde geçersiz",
                                ch,
                            ),
                        )?;
                        if upper_shift {
                            out.push(text + 128);
                            upper_shift = false;
                        } else {
                            out.push(text);
                        }
                    }
                    // 7.2.4.7: C40/Text içindeki FNC1, latch nedeniyle ilk iki
                    // symbol karakteri konumunda olamayacağından her zaman alan
                    // ayırıcıdır ve GS (ASCII 29) olarak iletilir (12.2).
                    27 => out.push(29),
                    30 => upper_shift = true,
                    _ => {
                        return Err(DataDecodingError::UnexpectedCharacter(
                            "C40/Text shift2 set içinde geçersiz",
                            ch,
                        ));
                    }
                }
                shift = 0;
            } else {
                match ch {
                    ch @ 0..=31 => {
                        let text = map_shift3.get(usize::from(ch)).copied().ok_or(
                            DataDecodingError::UnexpectedCharacter(
                                "C40/Text shift3 set içinde geçersiz",
                                ch,
                            ),
                        )?;
                        if upper_shift {
                            out.push(text + 128);
                            upper_shift = false;
                        } else {
                            out.push(text);
                        }
                    }
                    _ => {
                        return Err(DataDecodingError::UnexpectedCharacter(
                            "C40/Text shift3 set içinde geçersiz",
                            ch,
                        ));
                    }
                }
                shift = 0;
            }
        }
    }
    if data.len() == 1 && data.peek(0) == Some(UNLATCH) {
        // End of data noktasında tek UNLATCH
        data.eat()?;
        explicit_unlatch = true;
    }

    // Yalnızca 7.2.5.3 b'nin symbol sonunda bıraktığı Shift 1 dolgu durumu
    // eksik bir shift olarak kabul edilmez. Açık UNLATCH/padding öncesindeki ya
    // da Upper Shift bayrağı bırakan bütün eksik durumlar bozuk veridir.
    let valid_shift1_padding = !explicit_unlatch && data.is_empty() && shift == 1 && !upper_shift;
    if (shift != 0 || upper_shift) && !valid_shift1_padding {
        return Err(DataDecodingError::UnexpectedEnd);
    }
    Ok((data, EncodationType::Ascii))
}

#[test]
fn test_ascii() {
    let mut out = vec![];
    let mut eci = vec![];
    let mut fnc1 = None;
    assert_eq!(
        decode_ascii(Reader(b"BCD\x82\xeb\x26", 0), &mut out, &mut eci, &mut fnc1),
        Ok((Reader(&[], 6), EncodationType::Ascii))
    );
    assert_eq!(&out, b"ABC00\xa5");
    assert_eq!(fnc1, None);
}

#[test]
fn test_c40() {
    assert_eq!(decode_data(&[230, 91, 11]), Ok(vec![b'A', b'I', b'M']));
}

#[test]
fn test_edifact() {
    assert_eq!(
        decode_data(&[240, 16, 21, 1]),
        Ok(vec![b'D', b'A', b'T', b'A'])
    );
}

#[test]
fn test_base256() {
    assert_eq!(
        decode_data(&[231, 44, 108, 59, 226, 126, 1, 104]),
        Ok(vec![0xab, 0xe4, 0xf6, 0xfc, 0xe9, 0xbb])
    );
}

#[test]
fn test_read_eci() -> Result<(), &'static str> {
    use crate::encodation::GenericDataEncoder;

    fn enc_dec(eci: u32) -> Result<u32, &'static str> {
        let symbols = crate::SymbolList::default();
        let mut encoder = GenericDataEncoder::with_size(&[], &symbols, EncodationType::all(), None);
        encoder
            .write_eci(eci)
            .map_err(|_| "ECI codeword yazılamadı")?;
        let (codewords, _) = encoder
            .codewords()
            .map_err(|_| "ECI codeword listesi tamamlanamadı")?;
        let encoded_eci = codewords
            .get(1..)
            .ok_or("ECI latch sonrasında codeword bulunamadı")?;
        Ok(read_eci(Reader(encoded_eci, 0))
            .map_err(|_| "ECI codeword okunamadı")?
            .1)
    }

    for eci in (0..=999999).step_by(31) {
        assert_eq!(enc_dec(eci)?, eci);
    }
    assert_eq!(enc_dec(0)?, 0);
    assert_eq!(enc_dec(126)?, 126);
    assert_eq!(enc_dec(127)?, 127);
    assert_eq!(enc_dec(16382)?, 16382);
    assert_eq!(enc_dec(16383)?, 16383);
    assert_eq!(enc_dec(999999)?, 999999);

    for invalid in [0, 255] {
        assert_eq!(
            read_eci(Reader(&[192, 1, invalid], 0)),
            Err(DataDecodingError::UnexpectedCharacter(
                "ECI sonrasındaki üçüncü codeword",
                invalid
            ))
        );
    }
    Ok(())
}

#[test]
fn test_strict_eot_c40_unlatch() {
    assert_eq!(
        decode_data(&[ascii::LATCH_TEXT, UNLATCH, UNLATCH, 50]),
        Err(DataDecodingError::UnexpectedCharacter(
            "ASCII içinde geçersiz",
            UNLATCH
        )),
    );
    assert_eq!(
        decode_data(&[ascii::LATCH_X12, UNLATCH, UNLATCH, 50]),
        Err(DataDecodingError::UnexpectedCharacter(
            "ASCII içinde geçersiz",
            UNLATCH
        )),
    );
}

#[test]
fn invalid_c40_like_tuples_return_errors_without_panicking() {
    for latch in [ascii::LATCH_C40, ascii::LATCH_TEXT, ascii::LATCH_X12] {
        assert!(decode_data(&[latch, 0, 0]).is_err());
        assert!(decode_data(&[latch, 250, 255]).is_err());
    }
}

#[test]
fn test_decode_macro_string() {
    assert_eq!(
        decode_str(&[MACRO05, b'A' + 1]),
        Ok("[)>\x1e05\x1dA\x1e\x04".into()),
    );
    assert_eq!(
        decode_str(&[MACRO06, b'A' + 1]),
        Ok("[)>\x1e06\x1dA\x1e\x04".into()),
    );
}
