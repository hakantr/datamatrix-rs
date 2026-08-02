//! Planner ile encoder arasındaki değişmezleri sabitleyen property test'leri.
//!
//! Planner (`planner::optimize`) ve gerçek encoder'lar, mode başına aynı mantığın
//! iki paralel implementasyonudur: planner codeword cinsinden cost tahmin eder,
//! encoder ise byte'ları üretir. Bu testler aralarındaki sınırları korur:
//!
//! 1. `planner_cost_matches_encoder_length` — planner'ın tahmini cost'u,
//!    encoder'ın padding öncesinde gerçekten ürettiği codeword sayısına eşittir.
//!    Bu temel tutarlılık değişmezidir; fark olması yinelenen end-of-data
//!    mantığının eşzamanlılığını kaybettiğini gösterir.
//! 2. `more_modes_never_increase_size` — daha fazla encodation mode etkinleştirmek
//!    symbol'ü yalnızca aynı boyutta tutabilir veya küçültebilir. İç ayrıntılara
//!    ihtiyaç duymayan düşük maliyetli bir optimality denetimidir.
//! 3. `round_trip` — encode ardından decode işlemi input'u geri döndürür. AFL fuzz
//!    hedefi de bu özelliği denetler.

use alloc::vec::Vec;

use flagset::FlagSet;
use proptest::prelude::*;

use super::EncodationType;
use crate::data::{decode_data, encode_data, encode_data_unpadded_len};
use crate::encodation::planner::optimize_cost;
use crate::symbol_size::{SymbolList, SymbolSize};

/// Bit `i`, `MODES[i]` ile eşleşecek biçimde bildirim sırasındaki bütün encodation mode'ları.
const MODES: [EncodationType; 6] = [
    EncodationType::Ascii,
    EncodationType::C40,
    EncodationType::Text,
    EncodationType::X12,
    EncodationType::Edifact,
    EncodationType::Base256,
];

/// Bit mask'tan etkin mode kümesi oluşturur. Diğer bütün scheme'ler ASCII'den
/// başlatıldığı için ASCII her zaman eklenir (ISO/IEC 16022:2024, 7.2.1).
fn modes_from_mask(mask: u8) -> FlagSet<EncodationType> {
    let mut set = FlagSet::from(EncodationType::Ascii);
    for (i, mode) in MODES.iter().enumerate().skip(1) {
        if mask & (1 << i) != 0 {
            set |= *mode;
        }
    }
    set
}

/// Mode switch'leri ve her scheme'in end-of-data sınır durumlarını çalıştıran
/// rastgele ve yapılandırılmış input karışımı.
fn data_strategy() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        proptest::collection::vec(any::<u8>(), 0..64), // Rastgele byte'lar
        proptest::collection::vec(0x20u8..0x7f, 0..96), // Yazdırılabilir ASCII
        proptest::collection::vec(0x30u8..0x3a, 0..96), // Rakamlar (ASCII/C40/X12 sınırları)
        proptest::collection::vec(0x41u8..0x5b, 0..96), // A-Z (C40/X12 native)
        proptest::collection::vec(0x20u8..0x5f, 0..96), // EDIFACT range
    ]
}

/// Kısıtlı kapasitesi nedeniyle padding'in tek codeword farkını gizleyemediği
/// sabit boyutlar dahil, farklı şekillere sahip symbol listeleri.
fn symbols_strategy() -> impl Strategy<Value = SymbolList> {
    prop_oneof![
        Just(SymbolList::default()),
        Just(SymbolList::with_extended_rectangles()),
        Just(SymbolList::default().enforce_square()),
        any::<prop::sample::Index>().prop_map(|idx| {
            let all: Vec<SymbolSize> = enum_iterator::all::<SymbolSize>().collect();
            all.get(idx.index(all.len()))
                .copied()
                .map(SymbolList::from)
                .unwrap_or_default()
        }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// Encoder başarılı olduğunda planner, padding öncesinde üretilen codeword
    /// sayısını tam olarak tahmin etmelidir.
    ///
    /// Tersi doğrulanmaz: ASCII planning symbol kapasitesiyle sınırlı olmadığından
    /// planner symbol listesine sığmayan bir plan için cost bildirebilir; encoder
    /// ise bunu doğru biçimde reddeder. Yetkili sonuç encoder olduğundan yalnızca
    /// kabul ettiği input'lar karşılaştırılır.
    #[test]
    fn planner_cost_matches_encoder_length(
        data in data_strategy(),
        mask in any::<u8>(),
        symbols in symbols_strategy(),
    ) {
        let modes = modes_from_mask(mask);
        if let Some(actual) = encode_data_unpadded_len(&data, &symbols, modes) {
            let predicted = optimize_cost(&data, 0, EncodationType::Ascii, &symbols, modes);
            prop_assert_eq!(
                predicted, Some(actual),
                "planner {:?} codeword tahmin etti ancak encoder {} üretti; data={:?} mode'lar={:?}",
                predicted, actual, data, modes,
            );
        }
    }

    /// Daha fazla mode etkinleştirmek planner'a daha çok seçenek verdiğinden seçilen
    /// symbol hiçbir zaman büyüyemez.
    #[test]
    fn more_modes_never_increase_size(data in data_strategy(), mask in any::<u8>()) {
        let symbols = SymbolList::default();
        let size = |modes| {
            encode_data(&data, &symbols, None, modes, false)
                .ok()
                .map(|(_, symbol)| symbol.num_data_codewords())
        };
        let all = size(EncodationType::all());
        let subset = size(modes_from_mask(mask));
        if let (Some(all), Some(subset)) = (all, subset) {
            prop_assert!(
                all <= subset,
                "bütün mode'lar {} codeword, {:?} alt kümesi {} codeword üretti; data={:?}",
                all, modes_from_mask(mask), subset, data,
            );
        }
    }

    /// Encoding ardından decoding özgün input'u geri döndürür.
    #[test]
    fn round_trip(data in data_strategy(), mask in any::<u8>()) {
        let modes = modes_from_mask(mask);
        if let Ok((codewords, _)) = encode_data(&data, &SymbolList::default(), None, modes, false) {
            let decoded = decode_data(&codewords);
            if let Ok(decoded) = decoded {
                prop_assert_eq!(decoded, data);
            } else {
                prop_assert!(false, "decode başarısız: {decoded:?}; veri: {data:?}");
            }
        }
    }
}
