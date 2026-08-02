use alloc::{vec, vec::Vec};

use flagset::FlagSet;

use super::{DataEncodingError, EncodingContext, encodation_type::EncodationType};
use crate::data::encode_data;
use crate::symbol_size::SymbolList;

#[cfg(test)]
use pretty_assertions::assert_eq;

/// Yalnızca testlerde kullanılan encodation context'i.
pub(super) struct TestEncodingContext {
    /// Parser tarafından tüketilen byte'ları içerir.
    pub(super) removed: Vec<u8>,
    pub(super) data: Vec<u8>,
    pub(super) codewords: Vec<u8>,
    pub(super) mode: EncodationType,
    /// Symbol içindeki kullanılabilir alan.
    size: usize,
    /// `maybe_switch_mode` true döndürene kadar çağrılacağı sayı.
    count: isize,
}

impl TestEncodingContext {
    pub fn new(data: Vec<u8>, size: usize, count: isize) -> Self {
        Self {
            data,
            codewords: Vec::new(),
            size,
            count,
            mode: EncodationType::Ascii,
            removed: Vec::new(),
        }
    }
}

impl EncodingContext for TestEncodingContext {
    fn maybe_switch_mode(&mut self) -> bool {
        self.count -= 1;
        self.count == 0
    }

    fn symbol_size_left(&mut self, extra_codewords: usize) -> Option<usize> {
        let needed = self.codewords().len() + extra_codewords;
        if needed > self.size {
            None
        } else {
            Some(self.size - needed)
        }
    }

    fn eat(&mut self) -> Option<u8> {
        if self.data.is_empty() {
            None
        } else {
            let removed = self.data.remove(0);
            self.removed.push(removed);
            Some(removed)
        }
    }

    fn backup(&mut self, steps: usize) {
        if steps > self.removed.len() {
            crate::invariant_violation(
                "test context geri alma miktarı tüketilen veri sayısını aşıyor",
            );
        }
        for i in self.removed.iter().rev().take(steps) {
            self.data.insert(0, *i);
        }
    }

    fn rest(&self) -> &[u8] {
        &self.data
    }

    fn push(&mut self, ch: u8) {
        self.codewords.push(ch);
    }

    fn replace(&mut self, index: usize, ch: u8) {
        let Some(codeword) = self.codewords.get_mut(index) else {
            crate::invariant_violation("test context replace konumu codeword sınırlarının dışında");
        };
        *codeword = ch;
    }

    fn insert(&mut self, index: usize, ch: u8) {
        if index > self.codewords.len() {
            crate::invariant_violation("test context insert konumu codeword sınırlarının dışında");
        }
        self.codewords.insert(index, ch);
    }

    fn set_ascii_until_end(&mut self) {
        self.mode = EncodationType::Ascii;
    }

    fn codewords(&self) -> &[u8] {
        &self.codewords
    }
}

#[cfg(test)]
fn enc(data: &[u8]) -> Vec<u8> {
    enc_mode(data, EncodationType::all())
}

#[cfg(test)]
fn enc_mode(data: &[u8], enabled_modes: impl Into<FlagSet<EncodationType>>) -> Vec<u8> {
    encode_data(
        data,
        &SymbolList::default(),
        None,
        enabled_modes.into(),
        false,
    )
    .map(|(codewords, _)| codewords)
    .unwrap_or_default()
}

#[test]
fn test_ascii_encodation_two_digits() {
    assert_eq!(enc(b"123456"), vec![142, 164, 186]);
}

#[test]
fn test_ascii_encodation_two_digits_with_upper() {
    assert_eq!(enc(b"123456\xa3"), vec![142, 164, 186, 235, 36]);
}

#[test]
fn test_ascii_encodation_example1() {
    assert_eq!(
        enc(b"30Q324343430794<OQQ"),
        vec![160, 82, 162, 173, 173, 173, 137, 224, 61, 80, 82, 82]
    );
}

#[test]
fn test_c40_basic2_3() {
    assert_eq!(
        enc_mode(b"AIMAIMAIM\xcb", EncodationType::C40),
        vec![230, 91, 11, 91, 11, 91, 11, 11, 9, 254],
    );
    // Alternatif çözüm:
    // assert_eq!(words, vec![230, 91, 11, 91, 11, 91, 11, 254, 235, 76]);
    // Açıklama: 230 = C40'a shift, "91, 11" = "AIM",
    // "11, 9" = "\xcb" = "Shift 2, UpperShift, <char>
    // "else" durumu
}

#[test]
fn test_c40_spec_example() {
    assert_eq!(
        enc_mode(b"A_2_D_5_G7H_9J_1L2", EncodationType::C40),
        vec![
            230, 87, 195, 37, 195, 106, 131, 56, 131, 126, 206, 10, 94, 144, 3, 35, 47, 254
        ],
        // Alternatifler:
        // vec![66, 96, 51, 96, 69, 96, 230, 56, 131, 126, 206, 10, 94, 144, 3, 35, 47, 254],
        // vec![66, 96, 51, 96, 69, 96, 54, 96, 230, 126, 206, 10, 94, 144, 3, 35, 47, 254],
        // vec![230, 88, 88, 40, 8, 107, 147, 59, 67, 126, 206, 78, 126, 144, 121, 35, 47, 254]
    );
}

#[test]
fn test_c40_special_case_a() {
    // "a" durumu: Unlatch gerekmez.
    assert_eq!(enc(b"lvzvlv"), vec![239, 161, 224, 222, 204]);
}

#[test]
fn test_c40_special_case_b() {
    // "b" durumu: Sona shift 0 eklenir ve Unlatch gerekmez.
    assert_eq!(
        enc(b"\x83)nnnnnnnn\xb8"),
        vec![235, 4, 42, 239, 173, 20, 173, 20, 172, 250, 189, 97]
    );
}

#[test]
fn test_c40_special_case_c() {
    // "c" durumu: Unlatch uygulanır ve son karakter ASCII olarak yazılır.
    assert_eq!(
        enc(b"?      T        \xda  \x10"),
        vec![
            64, 230, 19, 60, 19, 60, 206, 188, 19, 60, 19, 60, 11, 24, 19, 57, 254, 17
        ],
    );
}

#[test]
fn test_c40_special_case_d() {
    // "d" durumu: Unlatch atlanır ve son karakter ASCII olarak yazılır.
    assert_eq!(enc(b"    \x1d    "), vec![230, 19, 60, 18, 222, 19, 60, 33]);
}

#[test]
fn test_c40_special_case2_d() {
    // "d" durumu: Unlatch atlanır ve son iki rakam ASCII olarak yazılır.
    assert_eq!(
        enc(b" 9 aaabbb00"),
        vec![239, 20, 204, 89, 191, 96, 40, 130]
    );
}

#[test]
fn test_c40_special_cases2() {
    // Kullanılabilir alan > 2 ve kalan = 2 ise üçlü, 7.2.5.3 b'deki gibi Shift 1
    // ile doldurulur ve unlatch uygulanır: son çift (a, i, Shift 1) = (90, 241).
    assert_eq!(
        enc(b"aimaimaimaimaimaimai"),
        vec![
            239, 91, 11, 91, 11, 91, 11, 91, 11, 91, 11, 91, 11, 90, 241, 254
        ]
    );
}

#[test]
fn test_c40_rule_c_preempt_no_orphan_shift() {
    // Son karakter iki değerliyse ('!' = Shift 2 + 0) ve üçlü sınırını
    // bölecekse karakter C40'a hiç sokulmaz: kalan iki değer 7.2.5.3 b'deki
    // gibi Shift 1 ile kapatılır ((A, B, Shift1) = 89, 217), UNLATCH ve ASCII
    // izler. Eski davranış shift değerini üçlüde askıda bırakıyordu (89, 218).
    assert_eq!(
        enc_mode(b"AB!", EncodationType::C40),
        vec![230, 89, 217, 254, 34]
    );
    // Üretilen akış sondaki Shift 1 pad'iyle birlikte doğru decode edilir.
    assert_eq!(
        crate::decodation::decode_data(&[230, 89, 217, 254, 34]),
        Ok(b"AB!".to_vec())
    );
}

#[test]
fn test_c40_rule_d_preempt_no_orphan_shift() {
    // Rule d'nin (7.2.5.3 d) tam-sığma çerçevesi: son üçlüden sonra tek yuva
    // kalır. İki değerli son karakter ('!') üçlüyü böleceğine geri alınır; üçlü
    // Shift 1 pad ile kapatılır ((A, B, Shift1) = 89, 217) ve '!' örtük unlatch
    // ile son yuvaya ASCII olarak yazılır. Eski davranış üçlüde Shift 2'yi
    // askıda bırakıyordu (89, 218). Kural, "(data character)" ön koşulunu içerir.
    assert_eq!(
        enc_mode(b"AIMAIMAB!", EncodationType::C40),
        vec![230, 91, 11, 91, 11, 89, 217, 34]
    );
    // Örtük unlatch'li akış doğru decode edilir.
    assert_eq!(
        crate::decodation::decode_data(&[230, 91, 11, 91, 11, 89, 217, 34]),
        Ok(b"AIMAIMAB!".to_vec())
    );
}

#[test]
fn test_base256_length_at_249_boundary() {
    // ISO/IEC 16022:2024 Table 8: 249 byte'a kadar tek byte'lık uzunluk alanı
    // kullanılır. Randomize edilmiş d1 = 249 + ((149*2) mod 255 + 1) - 256 = 37.
    let out = enc(&vec![150; 249]);
    assert_eq!(out.first(), Some(&231));
    assert_eq!(out.get(1), Some(&37));
    // İlk data byte'ı: 150 + ((149*3) mod 255 + 1) = 343 - 256 = 87.
    assert_eq!(out.get(2), Some(&87));
    // 2 + 249 = 251 codeword Square64'e (280) pad'lenir; ilk pad 129'dur.
    assert_eq!(out.get(2 + 249), Some(&129));
    assert_eq!(out.len(), 280);
}

#[test]
fn test_base256_length_at_250_boundary() {
    // 250 byte iki byte'lık uzunluk alanına geçer: d1 = 250, d2 = 0.
    // Randomize edilmiş halleri 38 (294 - 256) ve 193 (0 + 193).
    let out = enc(&vec![150; 250]);
    assert_eq!(out.first(), Some(&231));
    assert_eq!(out.get(1..3), Some(&[38, 193][..]));
    // İlk data byte'ı: 150 + ((149*4) mod 255 + 1) = 237.
    assert_eq!(out.get(3), Some(&237));
    // 3 + 250 = 253 codeword Square64'e (280) pad'lenir; ilk pad 129'dur.
    assert_eq!(out.get(3 + 250), Some(&129));
    assert_eq!(out.len(), 280);
}

#[test]
fn test_text_encoding_1() {
    // 239, Text encodation'a geçer; 254 unlatch uygular.
    let words = encode_data(
        b"aimaimaim",
        &SymbolList::default(),
        None,
        EncodationType::all(),
        false,
    )
    .map(|(codewords, _)| codewords)
    .unwrap_or_default();
    assert_eq!(words, vec![239, 91, 11, 91, 11, 91, 11, 254]);
}

#[test]
fn test_text_encoding_2() {
    assert_eq!(
        enc(b"aimaimaim'"),
        vec![239, 91, 11, 91, 11, 91, 11, 254, 40, 129]
    );
    // Bu bir alternatiftir ancak specification kurallarına tamamen uymaz.
    // assertEquals("239, 91, 11, 91, 11, 91, 11, 7, 49, 254", visualized);
}

#[test]
fn test_text_encoding_3() {
    assert_eq!(enc(b"aimaimaIm"), vec![239, 91, 11, 91, 11, 87, 218, 110]);
}

#[test]
fn test_text_encoding_4() {
    assert_eq!(
        enc(b"aimaimaimB"),
        vec![239, 91, 11, 91, 11, 91, 11, 254, 67, 129]
    );
}

#[test]
fn test_text_encoding_5() {
    assert_eq!(
        enc(b"aimaimaim{txt}\x04"),
        vec![
            239, 91, 11, 91, 11, 91, 11, 16, 218, 236, 107, 181, 69, 254, 129, 237
        ]
    );
}

#[test]
fn test_x12_1() {
    // 238, X12 encodation'a geçer; 254 unlatch uygular.
    assert_eq!(
        enc(b"AB\x0d>ABC123>AB"),
        vec![238, 89, 217, 14, 192, 100, 207, 44, 31, 67,]
    );
}

#[test]
fn test_x12_2a() {
    assert_eq!(
        enc(b"AB\x0d>ABC123>ABC"),
        // BC, X12 içinde eksik bir üçlü olarak kalır; end kuralı uygulanmaz.
        vec![238, 89, 217, 14, 192, 100, 207, 44, 31, 254, 67, 68]
    );
}

#[test]
fn test_x12_2b() {
    assert_eq!(
        enc(b"AB\x0d>ABC123>A00"),
        // 00, X12 içinde eksik bir üçlü olarak kalır; end kuralı uygulanır ve
        // tek ASCII (130) olarak encode edilebilir.
        vec![238, 89, 217, 14, 192, 100, 207, 44, 31, 130]
    );
}

#[test]
fn test_x12_3() {
    assert_eq!(
        enc(b"AB\x0d>ABC123>ABCD"),
        vec![238, 89, 217, 14, 192, 100, 207, 44, 31, 96, 82, 254]
    );
}

#[test]
fn test_x12_4() {
    assert_eq!(
        enc(b"ABC>ABC123>ABCDE"),
        vec![
            238, // UNLATCH
            89, 233, // ABC
            14, 192, // >AB
            100, 207, // C12
            44, 31, // 3>A
            96, 82, // BCD
            70  // E (ASCII)
        ]
    );
}

#[test]
fn test_x12_5() {
    assert_eq!(
        enc_mode(b"ABC>ABC123>ABCDEF", EncodationType::X12),
        vec![
            238, 89, 233, 14, 192, 100, 207, 44, 31, 96, 82, 254, 70, 71, 129, 237
        ],
        // Bütün mode'lar etkinken optimizer artık eşit uzunluktaki (16 codeword)
        // EDIFACT tabanlı encoding'i seçer:
        // vec![240, 4, 32, 254, 4, 32, 241, 203, 63, 129, 8, 49, 5, 25, 240, 129],
    );
}

#[test]
fn test_edifact_1() {
    // 240, EDIFACT encodation'a geçer.
    assert_eq!(
        enc(b".A.C1.3.DATA.123DATA.123DATA"),
        vec![
            240, 184, 27, 131, 198, 236, 238, 16, 21, 1, 187, 28, 179, 16, 21, 1, 187, 28, 179, 16,
            21, 1
        ]
    );
}

#[test]
fn test_edifact_2() {
    assert_eq!(
        enc(b".A.C1.3.X.X2.."),
        vec![240, 184, 27, 131, 198, 236, 238, 98, 230, 50, 47, 47]
    );
}

#[test]
fn test_edifact_3() {
    assert_eq!(
        enc(b".A.C1.3.X.X2."),
        vec![240, 184, 27, 131, 198, 236, 238, 98, 230, 50, 47, 129]
    );
}

#[test]
fn test_edifact_4() {
    assert_eq!(
        enc(b".A.C1.3.X.X2"),
        vec![240, 184, 27, 131, 198, 236, 238, 98, 230, 50]
    );
}

#[test]
fn test_edifact_5() {
    assert_eq!(
        enc(b".A.C1.3.X.X"),
        vec![240, 184, 27, 131, 198, 236, 238, 98, 230, 31]
    );
}

#[test]
fn test_edifact_6() {
    assert_eq!(
        enc_mode(b".A.C1.3.X.", EncodationType::Edifact),
        // 240 LATCH
        // ".A.C" 184 27 131
        // "1.3." 198 236 238
        // X." 98 231 192
        vec![240, 184, 27, 131, 198, 236, 238, 98, 231, 192]
    );
}

#[test]
fn test_edifact_7() {
    assert_eq!(
        enc(b".A.C1.3.X"),
        vec![240, 184, 27, 131, 198, 236, 238, 89]
    );
}

#[test]
fn test_edifact_8() {
    // EDIFACT'ten geçici unlatch işlemini denetler.
    assert_eq!(
        enc(b".XXX.XXX.XXX.XXX.XXX.XXX.\xFCXX.XXX.XXX.XXX.XXX.XXX.XXX"),
        vec![
            240, 185, 134, 24, 185, 134, 24, 185, 134, 24, 185, 134, 24, 185, 134, 24, 185, 134,
            24, 185, 240, 235, 125, 240, 97, 139, 152, 97, 139, 152, 97, 139, 152, 97, 139, 152,
            97, 139, 152, 97, 139, 152, 89, 89
        ]
    );
}

#[cfg(test)]
fn create_binary_test_message(len: usize) -> Vec<u8> {
    let mut vec = vec![171, 228, 246, 252, 233, 224, 225, 45];
    vec.resize(len - 1, b'\xB7');
    vec.push(b'\xBB');
    vec
}

#[test]
fn test_base256_1() {
    // 231, Base256 encodation'a geçer.
    assert_eq!(
        enc(b"\xab\xe4\xf6\xfc\xe9\xbb"),
        vec![231, 44, 108, 59, 226, 126, 1, 104]
    );
}

#[test]
fn test_base256_2() {
    assert_eq!(
        enc(b"\xab\xe4\xf6\xfc\xe9\xe0\xbb"),
        vec![231, 51, 108, 59, 226, 126, 1, 141, 254, 129]
    );
}

#[test]
fn test_base256_3() {
    assert_eq!(
        enc(b"\xab\xe4\xf6\xfc\xe9\xe0\xe1\xbb"),
        vec![231, 44, 108, 59, 226, 126, 1, 141, 36, 147]
    );
}

#[test]
fn test_base256_4() {
    // Yalnızca ASCII (referans için)
    assert_eq!(enc(b" 23\xa3"), vec![33, 153, 235, 36, 129]);
}

#[test]
fn test_base256_5() {
    // Karma Base256 + ASCII
    assert_eq!(
        enc(b"\xab\xe4\xf6\xfc\xe9\xbb 234"),
        vec![231, 51, 108, 59, 226, 126, 1, 104, 99, 153, 53, 129],
        // Alternatif:
        // vec![231, 50, 108, 59, 226, 126, 1, 104, 33, 153, 53, 129]
    );
}

#[test]
fn test_base256_6() {
    assert_eq!(
        enc(b"\xab\xe4\xf6\xfc\xe9\xbb 23\xa3 1234567890123456789"),
        vec![
            231, 55, 108, 59, 226, 126, 1, 104, 99, 10, 161, 167, 185, 142, 164, 186, 208, 220,
            142, 164, 186, 208, 58, 129, 59, 209, 104, 254, 150, 45
        ],
        // Alternatif:
        // vec![
        //     231, 51, 108, 59, 226, 126, 1, 104, 99, 153, 235, 36, 33, 142, 164,
        //     186, 208, 220, 142, 164, 186, 208, 58, 129, 59, 209, 104, 254, 150, 45,
        // ]
    );
}

#[test]
fn test_base256_7() {
    // Sonda padding gerekir.
    assert_eq!(
        enc(&create_binary_test_message(20)),
        vec![
            231, 44, 108, 59, 226, 126, 1, 141, 36, 5, 37, 187, 80, 230, 123, 17, 166, 60, 210,
            103, 253, 150
        ]
    );
}

#[test]
fn test_base256_8() {
    assert_eq!(
        enc(&create_binary_test_message(19)),
        vec![
            231, 63, 108, 59, 226, 126, 1, 141, 36, 5, 37, 187, 80, 230, 123, 17, 166, 60, 210,
            103, 1, 129
        ],
    );
}

#[test]
fn test_base256_9() {
    let words = enc(&create_binary_test_message(276));
    let start = vec![231, 38, 219, 2, 208, 120, 20, 150, 35];
    assert_eq!(words.get(..start.len()), Some(start.as_slice()));
    let end = vec![146, 40, 194, 129];
    assert_eq!(
        words
            .len()
            .checked_sub(end.len())
            .and_then(|start| words.get(start..)),
        Some(end.as_slice())
    );
}

#[test]
fn test_base256_10() {
    let words = enc(&create_binary_test_message(277));
    let start = vec![231, 38, 220, 2, 208, 120, 20, 150, 35];
    assert_eq!(words.get(..start.len()), Some(start.as_slice()));
    let end = vec![146, 40, 190, 87];
    assert_eq!(
        words
            .len()
            .checked_sub(end.len())
            .and_then(|start| words.get(start..)),
        Some(end.as_slice())
    );
}

#[test]
fn test_c40_unlatching() {
    let codewords = enc(b"AIMAIMAIMAIMaimaimaim");
    // C40 veya X12 encoded olabilir.
    assert!(
        codewords
            .first()
            .is_some_and(|value| matches!(value, 230 | 238))
    );
    assert_eq!(
        codewords.get(1..),
        Some(
            [
                91, 11, 91, 11, 91, 11, 91, 11, 254, 239, 91, 11, 91, 11, 91, 11, 254
            ]
            .as_slice()
        )
    );
}

#[test]
fn test_unlatching_from_text() {
    assert_eq!(
        enc(b"aimaimaimaim12345678"),
        vec![
            239, 91, 11, 91, 11, 91, 11, 91, 11, 254, 142, 164, 186, 208, 129, 237
        ]
    );
}

#[test]
fn test_hello_world() {
    // Sondaki iki değerlik kalıntı Shift 1 pad ile üçlüye tamamlanır (bkz. 7.2.5.3 b).
    let out = enc(b"Hello World!");
    assert_eq!(
        out,
        vec![73, 239, 116, 130, 175, 123, 148, 64, 158, 233, 254, 34]
    );
}

#[test]
fn test_edifact_short() {
    assert_eq!(enc(b"CR%X-----"), vec![240, 13, 41, 88, 182, 219, 109, 46]);
}

#[test]
fn test_ascii_short() {
    assert_eq!(
        // EDIFACT kullanmaya gerek yoktur; ASCII de 5 codeword kullanır.
        enc(b"CR%X-"),
        vec![68, 83, 38, 89, 46]
    );
}

#[test]
fn test_bug_1664266_1() {
    assert_eq!(
        enc(b"CREX-TAN:h"),
        vec![68, 83, 70, 89, 46, 85, 66, 79, 59, 105],
        // Alternatif: C40
        // vec![230, 104, 235, 231, 117, 208, 140, 8, 155, 105],
        // Alternatif: EDIFACT
        // vec![240, 13, 33, 88, 181, 64, 78, 124, 59, 105]
    );
}

#[test]
fn test_bug_1664266_2() {
    assert_eq!(
        enc(b"CREX-TAN:hh"),
        vec![68, 83, 70, 89, 46, 85, 66, 79, 59, 105, 105, 129],
        // Alternatif: EDIFACT
        // vec![230, 104, 235, 231, 117, 208, 140, 8, 155, 50, 89, 254],
        // vec![240, 13, 33, 88, 181, 64, 78, 124, 59, 105, 105, 129]
    );
}

#[test]
fn test_bug_1664266_3() {
    assert_eq!(
        enc(b"CREX-TAN:hhh"),
        vec![68, 83, 70, 89, 46, 85, 66, 79, 59, 105, 105, 105],
        // Alternatif
        // vec![68, 83, 70, 89, 46, 85, 66, 79, 59, 239, 134, 158],
        // vec![240, 13, 33, 88, 181, 64, 78, 124, 59, 105, 105, 105]
    );
}

#[test]
fn test_x12_unlatch_ascii() {
    assert_eq!(
        enc(b"*\x0d*******00"),
        vec![238, 6, 66, 6, 106, 6, 106, 130]
    );
}

#[test]
fn test_x12_unlatch_2() {
    assert_eq!(enc(b"*\x0dTCP0"), vec![238, 6, 98, 104, 141]);
}

#[test]
fn test_bug_3048549() {
    // Java kaynak kodundaki 0x0060 karakterinin encoding sorunu nedeniyle burada
    // geçersiz karakter için IllegalArgumentException oluşuyordu.
    assert_eq!(
        enc_mode(b"fiykmj*Rh2`,e6", EncodationType::Text),
        vec![
            239, 122, 87, 154, 40, 7, 171, 115, 207, 12, 130, 71, 155, 254, 129, 237
        ]
    );
}

#[test]
fn test_only_base256() {
    assert_eq!(
        enc_mode(b"01", EncodationType::Base256),
        vec![231, 46, 241, 136, 129],
    );
}

#[test]
fn test_only_edifact() {
    assert_eq!(
        enc_mode(b"01", EncodationType::Edifact),
        vec![240, 131, 129],
    );
}

#[test]
fn test_only_edifact_impossible() {
    let code = encode_data(
        b"aaa",
        &SymbolList::default(),
        None,
        EncodationType::Edifact,
        false,
    );
    assert_eq!(code, Err(DataEncodingError::TooMuchOrIllegalData),);
}
