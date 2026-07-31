//! Bu modül, Data Matrix Reed–Solomon code'larının kullandığı GF(256) aritmetiğinin
//! implementasyonunu içerir.
//!
//! GF(256) içindeki bir öğenin varsayılan gösterimi u8 (8-bit integer) değeridir.
//! Bitleri, en düşük anlamlı bit 1'in katsayısı olacak biçimde 7. derece bir
//! polynomial'ın katsayılarına karşılık gelir. Örneğin:
//!
//! > 242 = 0b11110010 = x^7 + x^6 + x^5 + x^4 + x.
//!
//! Toplama, normal polynomial'larda olduğu gibi katsayı katsayı yapılabilir.
//!
//! İki polynomial'ı çarpmak x'in 7'den büyük kuvvetlerini oluşturabilir. Bu yüzden
//! çarpma sabit bir polynomial'a göre modulo olarak tanımlanır. Data Matrix 301
//! polynomial'ını kullanır.
//!
//! Bu seçimle x'in x^255'e kadarki kuvvetleri, yani 1, x^1, x^2, ..., x^255,
//! GF(256) içindeki 0 dışındaki bütün sayıları verir (multiplicative subgroup).
//! Bu nedenle "x bir generator'dır" denir. Dizi tekrar eder ve x^256 = 1 olur.
//!
//! Böylece GF(256) içindeki 0 dışındaki her öğe x'in i kuvvetiyle tanımlanabilir.
//! Örneğin a ve b çarpılacaksa önce bunların i ve j kuvvetleri bulunur. Ardından
//! a * b = x^i * x^j = x^(i + j) hesaplanır. x^(i + j) için ters lookup sonucu
//! verir. Bu iki lookup table modülde `LOG` ve `ANTI_LOG` olarak adlandırılır.
use core::ops::{Add, Div, Mul, Sub};
use core::{
    convert::From,
    ops::{DivAssign, MulAssign, Neg, SubAssign},
};
use core::{iter::Sum, ops::AddAssign};

#[cfg(test)]
use alloc::vec::Vec;

#[cfg(test)]
use pretty_assertions::assert_eq;

/// GF(256) için iki lookup table hesaplar.
#[expect(
    clippy::indexing_slicing,
    reason = "Döngü sınırları i < 255 ve indirgeme sonrası p < 256 olmasını garanti eder"
)]
const fn compute_alog_log() -> ([u8; 255], [u8; 256]) {
    let mut alog = [0u8; 255];
    let mut log = [0u8; 256];
    let mut p: u16 = 1; // Polynomial gösterimi
    let mut i: u8 = 0; // Kuvvet
    while i < 255 {
        alog[i as usize] = p as u8;
        log[p as usize] = i;

        // Çarpmayı tanımlayan irreducible polynomial 0x12D olduğunda x bir
        // primitive element'tir. Bu nedenle doğrudan x^i hesaplanabilir. Aşağıdaki
        // satırlar bunu yapar; extra/gf.py içindeki Python koduna da bakın.
        p *= 2;
        if p >= 256 {
            p ^= 0x12D;
        }

        i += 1;
    }
    (alog, log)
}

/// GF(256) öğesini generator a'nın i kuvveti gösteriminden 7. derece polynomial'a
/// dönüştüren lookup table.
const ANTI_LOG: [u8; 255] = compute_alog_log().0;

/// GF(256) öğesini 7. derece polynomial gösteriminden generator a'nın i kuvvetine
/// dönüştüren lookup table.
const LOG: [u8; 256] = compute_alog_log().1;

#[derive(Clone, Copy, PartialEq)]
pub struct GF(pub u8);

impl GF {
    // 1, x, x^2, x^3, ... için iterator döndürür.
    pub fn primitive_powers() -> impl Iterator<Item = Self> {
        ANTI_LOG.iter().map(|x| Self(*x)).cycle()
    }

    pub fn primitive_power(i: u8) -> Self {
        let Some(value) = ANTI_LOG.get(usize::from(i)).copied() else {
            crate::invariant_violation("GF(256) primitive power indeksi tablo sınırını aştı");
        };
        GF(value)
    }

    pub fn log(self) -> usize {
        let Some(value) = LOG.get(usize::from(self.0)).copied() else {
            crate::invariant_violation("GF(256) log indeksi tablo sınırını aştı");
        };
        usize::from(value)
    }
}

impl core::fmt::Debug for GF {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> Result<(), core::fmt::Error> {
        f.write_fmt(format_args!("{}₂₅₆", self.0))
    }
}

impl Add<GF> for GF {
    type Output = Self;

    #[allow(clippy::suspicious_arithmetic_impl)]
    fn add(self, rhs: Self) -> Self {
        GF(self.0 ^ rhs.0)
    }
}

impl AddAssign<GF> for GF {
    fn add_assign(&mut self, rhs: GF) {
        *self = *self + rhs;
    }
}

impl Sub<GF> for GF {
    type Output = Self;

    #[allow(clippy::suspicious_arithmetic_impl)]
    fn sub(self, rhs: Self) -> Self {
        self + rhs
    }
}

impl SubAssign<GF> for GF {
    fn sub_assign(&mut self, rhs: GF) {
        *self = *self - rhs;
    }
}

impl Mul<GF> for GF {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        if self.0 == 0 || rhs.0 == 0 {
            return GF(0);
        }
        let Some(ia) = LOG.get(usize::from(self.0)).copied() else {
            crate::invariant_violation("GF(256) sol çarpanının log değeri bulunamadı");
        };
        let Some(ib) = LOG.get(usize::from(rhs.0)).copied() else {
            crate::invariant_violation("GF(256) sağ çarpanının log değeri bulunamadı");
        };
        let i = (ia as u16 + ib as u16) % 255;
        let Some(value) = ANTI_LOG.get(usize::from(i)).copied() else {
            crate::invariant_violation("GF(256) çarpımının anti-log değeri bulunamadı");
        };
        GF(value)
    }
}

impl Mul<usize> for GF {
    type Output = Self;

    fn mul(self, rhs: usize) -> Self {
        // usize ile çarpma n kez toplama olarak yorumlanır. Öğeler kendi additive
        // inverse değerleri olduğundan yalnızca toplama sayısının tek veya çift
        // olduğu denetlenir.
        GF(self.0 * (rhs % 2) as u8)
        // cmov kullanan ancak mul kullanmayan alternatif:
        // if rhs % 2 == 0 {
        //     Self(0)
        // } else {
        //     self
        // }
    }
}

impl MulAssign<GF> for GF {
    fn mul_assign(&mut self, rhs: GF) {
        *self = *self * rhs;
    }
}

impl Div<GF> for GF {
    type Output = Self;

    fn div(self, rhs: Self) -> Self {
        if self.0 == 0 || rhs.0 == 0 {
            return GF(0);
        }
        let Some(ia) = LOG.get(usize::from(self.0)).copied() else {
            crate::invariant_violation("GF(256) bölüneninin log değeri bulunamadı");
        };
        let Some(ib) = LOG.get(usize::from(rhs.0)).copied() else {
            crate::invariant_violation("GF(256) böleninin log değeri bulunamadı");
        };
        let mut i = ia as i16 - ib as i16;
        if i < 0 {
            i += 255;
        }
        let Ok(index) = usize::try_from(i) else {
            crate::invariant_violation("GF(256) bölüm anti-log indeksi negatif kaldı");
        };
        let Some(value) = ANTI_LOG.get(index).copied() else {
            crate::invariant_violation("GF(256) bölümünün anti-log değeri bulunamadı");
        };
        GF(value)
    }
}

impl DivAssign<GF> for GF {
    fn div_assign(&mut self, rhs: GF) {
        *self = *self / rhs;
    }
}

impl Neg for GF {
    type Output = Self;

    fn neg(self) -> Self {
        Self(self.0)
    }
}

impl From<GF> for u8 {
    fn from(gf: GF) -> u8 {
        gf.0
    }
}

impl From<u8> for GF {
    fn from(i: u8) -> Self {
        GF(i)
    }
}

impl Sum for GF {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(GF(0), |a, b| a + b)
    }
}

#[test]
fn sanity_check_tables() {
    use alloc::collections::BTreeSet;

    let anti_log: BTreeSet<u8> = ANTI_LOG.iter().cloned().collect();
    assert_eq!(anti_log.len(), ANTI_LOG.len());

    let log: BTreeSet<u8> = LOG.get(1..).unwrap_or_default().iter().cloned().collect();
    assert_eq!(log.len(), LOG.len() - 1);

    for (i, anti_log) in ANTI_LOG.iter().copied().enumerate() {
        assert_eq!(
            Some(i),
            LOG.get(usize::from(anti_log))
                .map(|value| usize::from(*value))
        );
    }
    for (i, log) in LOG.iter().copied().enumerate().skip(1) {
        assert_eq!(
            Some(i),
            ANTI_LOG
                .get(usize::from(log))
                .map(|value| usize::from(*value))
        );
    }
}

#[test]
fn gf256_mul() {
    assert_eq!(GF(123) * GF(1), GF(123));
    assert_eq!(GF(234) * GF(0), GF(0));
    assert_eq!(GF(0) * GF(23), GF(0));
    assert_eq!(GF(2) * GF(4) * GF(8) * GF(16) * GF(32), GF(228));
}

#[test]
fn gf256_div_mul() {
    for a in 0..=255 {
        for b in 1..=255 {
            let a_div_b = GF(a) / GF(b);
            assert_eq!(a_div_b * GF(b), GF(a));
        }
    }
}

#[test]
fn test_gf256_power_iterator() {
    let powers: Vec<GF> = GF::primitive_powers().take(500).collect();
    let mut power_direct = Vec::with_capacity(500);
    let mut a = GF(1);
    for i in 0..500 {
        power_direct.push(a);
        assert_eq!(GF::primitive_power((i % 255) as u8), a);
        a *= GF(2);
    }
    assert_eq!(powers, power_direct);
}

#[test]
fn test_neg() {
    for a in 0..255 {
        let a = GF(a);
        let ma = -a;
        assert_eq!(a + ma, GF(0), "{:?}, {:?}", a, ma);
    }
}

#[test]
#[allow(clippy::identity_op)]
fn test_mul_usize() {
    assert_eq!(GF(5) * 1, GF(5));
    assert_eq!(GF(5) * 2, GF(5) + GF(5));
}
