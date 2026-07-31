mod syndrome_based;

use super::galois::GF;

use alloc::{vec, vec::Vec};

#[cfg(test)]
use pretty_assertions::assert_eq;

/// Hatalar düzeltilirken oluşan başarısızlığı belirtir.
///
/// Belirli variant pratik kullanım açısından önemli değildir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorDecodingError {
    TooManyErrors,
    /// Hata konumları codeword dışında bulundu.
    ///
    /// Bu genellikle düzeltilemeyecek kadar çok aktarım hatası olduğu anlamına gelir.
    ErrorsOutsideRange,
    Malfunction,
    DataSize {
        expected: usize,
        actual: usize,
    },
}

impl core::fmt::Display for ErrorDecodingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooManyErrors => {
                f.write_str("düzeltilebilecek sayıdan fazla codeword hatası var")
            }
            Self::ErrorsOutsideRange => {
                f.write_str("hesaplanan hata konumları codeword aralığının dışında")
            }
            Self::Malfunction => f.write_str("Reed–Solomon decoder tutarlı bir çözüm bulamadı"),
            Self::DataSize { expected, actual } => write!(
                f,
                "Reed–Solomon decoder {expected} codeword bekliyordu, {actual} verildi"
            ),
        }
    }
}

impl core::error::Error for ErrorDecodingError {}

pub use syndrome_based::decode;

/// Katsayıları `c` ile verilen polynomial'ı x, x^2, x^3, ... noktalarında
/// değerlendirir ve sonuçları bu sırayla `out` içine yazar.
fn primitive_element_evaluation<T, I>(c: I, out: &mut [GF]) -> bool
where
    T: Into<GF> + Copy,
    I: Iterator<Item = T> + DoubleEndedIterator,
{
    if out.is_empty() {
        return false;
    }
    let mut gamma: Vec<GF> = c.rev().map(Into::into).collect();
    let mut errors = false;
    for o in out.iter_mut() {
        for (g, alpha) in gamma.iter_mut().zip(GF::primitive_powers()) {
            *g *= alpha;
        }
        *o = gamma.iter().copied().sum();
        errors = errors || (*o != GF(0));
    }
    errors
}

/// Katsayıları `c` içinde verilen polynomial'ın sıfırlarını bulur.
fn chien_search<T: Into<GF> + Copy>(c: &[T]) -> Vec<GF> {
    let mut out = vec![];
    if c.is_empty() {
        return out;
    }
    if c.last().copied().is_some_and(|value| value.into() == GF(0)) {
        out.push(GF(0));
    }
    if let [c0, c1] = c {
        if (*c1).into() != GF(0) {
            out.push(-(*c1).into() / (*c0).into());
        }
        return out;
    }
    let mut gamma: Vec<GF> = c.iter().rev().map(|x| (*x).into()).collect();
    for i in 0..=254 {
        let val: GF = gamma.iter().copied().sum();
        if val == GF(0) {
            out.push(GF::primitive_power(i));
        }
        for (g, alpha) in gamma.iter_mut().zip(GF::primitive_powers()) {
            *g *= alpha;
        }
    }
    out
}

/// `matrix` * x = `b` doğrusal sistemini "pivoted" LU decomposition kullanarak x için çözer.
///
/// Matrix kare olmalıdır.
///
/// Çözüm bulunursa true döndürür.
#[allow(unused)]
fn solve(mat: &mut [GF], b: &mut [GF], row_stride: usize) -> bool {
    let n = b.len();
    if n == 0 || row_stride < n {
        return false;
    }
    let Some(required) = n.checked_mul(row_stride) else {
        return false;
    };
    if mat.len() < required {
        return false;
    }
    let c = |i: usize, j: usize| i * row_stride + j;
    for i in 0..(n - 1) {
        // Sıfır olmayan pivot girdisini bul.
        if let Some(i_nz) = (i..n).find(|k| {
            mat.get(c(*k, i))
                .copied()
                .is_some_and(|value| value != GF(0))
        }) {
            // Satırları değiştir.
            if i_nz != i {
                b.swap(i, i_nz);
                for j in 0..n {
                    mat.swap(c(i, j), c(i_nz, j));
                }
            }
        } else {
            return false;
        };

        for k in i + 1..n {
            // L değerini hesapla.
            let Some(entry) = mat.get(c(k, i)).copied() else {
                return false;
            };
            let Some(pivot) = mat.get(c(i, i)).copied() else {
                return false;
            };
            if pivot == GF(0) {
                return false;
            }
            let factor = entry / pivot;
            let Some(target) = mat.get_mut(c(k, i)) else {
                return false;
            };
            *target = factor;

            // U değerini hesapla.
            for j in i + 1..n {
                let Some(current) = mat.get(c(k, j)).copied() else {
                    return false;
                };
                let Some(upper) = mat.get(c(i, j)).copied() else {
                    return false;
                };
                let Some(target) = mat.get_mut(c(k, j)) else {
                    return false;
                };
                *target = current - factor * upper;
            }
        }
    }

    let last = n - 1;
    let Some(last_pivot) = mat.get(c(last, last)).copied() else {
        return false;
    };
    if last_pivot == GF(0) {
        return false;
    }

    // Lx = b sistemini çöz.
    for i in 0..n {
        for j in 0..i {
            let Some(b_j) = b.get(j).copied() else {
                return false;
            };
            let Some(coefficient) = mat.get(c(i, j)).copied() else {
                return false;
            };
            let Some(target) = b.get_mut(i) else {
                return false;
            };
            *target -= coefficient * b_j;
        }
    }
    // Ux = b sistemini çöz.
    for i in (0..n).rev() {
        for j in i + 1..n {
            let Some(b_j) = b.get(j).copied() else {
                return false;
            };
            let Some(coefficient) = mat.get(c(i, j)).copied() else {
                return false;
            };
            let Some(target) = b.get_mut(i) else {
                return false;
            };
            *target -= coefficient * b_j;
        }
        let Some(pivot) = mat.get(c(i, i)).copied() else {
            return false;
        };
        if pivot == GF(0) {
            return false;
        }
        let Some(target) = b.get_mut(i) else {
            return false;
        };
        *target /= pivot;
    }
    true
}

#[test]
fn test_evaluate_primitive() {
    let c = &[GF(90), GF(0), GF(23), GF(0), GF(1)];
    let mut out = vec![GF(0); 3];
    primitive_element_evaluation(c.iter().cloned(), &mut out);
    assert_eq!(out, vec![GF(100), GF(187), GF(131)]);
}

#[test]
fn test_solve_1x1() {
    let mut mat = vec![GF(5)];
    let mut b = [GF(88)];
    let solved = solve(&mut mat, &mut b[..], 1);
    assert!(solved);
    assert_eq!(b, [GF(170)]);
}

#[test]
fn test_solve_2x2() {
    let mut mat = vec![GF(2), GF(1), GF(5), GF(2)];
    let mut b = [GF(56), GF(23)];
    let solved = solve(&mut mat, &mut b[..], 2);
    assert!(solved);
    assert_eq!(GF(2) * b[0] + GF(1) * b[1], GF(56));
    assert_eq!(GF(5) * b[0] + GF(2) * b[1], GF(23));
}

#[test]
fn test_solve_3x3_permute() {
    let mut mat = vec![
        GF(0),
        GF(0),
        GF(8),
        GF(89),
        GF(0),
        GF(2),
        GF(45),
        GF(10),
        GF(5),
    ];
    let mut b = [GF(126), GF(23), GF(99)];
    let solved = solve(&mut mat, &mut b[..], 3);
    assert!(solved);
    assert_eq!(GF(0) * b[0] + GF(0) * b[1] + GF(8) * b[2], GF(126));
    assert_eq!(GF(89) * b[0] + GF(0) * b[1] + GF(2) * b[2], GF(23));
    assert_eq!(GF(45) * b[0] + GF(10) * b[1] + GF(5) * b[2], GF(99));
}

#[test]
fn test_solve_2x2_singular() {
    let mut mat = vec![GF(2), GF(1), GF(4), GF(2)];
    let mut b = [GF(56), GF(23)];
    let solved = solve(&mut mat, &mut b[..], 2);
    assert!(!solved);
}

#[test]
fn test_primitive_element_evaluation() {
    let x = [GF(128), GF(52), GF(33), GF(83), GF(33)];
    let mut syndromes = vec![GF(0); 5];
    primitive_element_evaluation(x.iter().cloned(), &mut syndromes);
    assert_eq!(&syndromes, &[GF(203), GF(50), GF(3), GF(247), GF(100),]);
}

#[test]
fn test_error_code() -> Result<(), super::ErrorEncodingError> {
    let mut data = vec![1, 2, 3];
    let ecc = super::encode_error(&data, crate::SymbolSize::Square10)?;
    data.extend_from_slice(&ecc);
    let mut syndromes = vec![GF(0); 5];
    primitive_element_evaluation(data.iter().cloned(), &mut syndromes);
    assert_eq!(&syndromes, &[GF(0), GF(0), GF(0), GF(0), GF(0)]);
    Ok(())
}

#[test]
fn test_chien() {
    let c = [GF(135), GF(239), GF(132), GF(21), GF(58), GF(77)];
    let zeros = chien_search(&c);
    assert_eq!(&zeros, &[GF(228), GF(78), GF(43)]);
}

#[test]
fn test_chien2() {
    let c = [GF(1), GF(211)];
    let zeros = chien_search(&c);
    assert_eq!(&zeros, &[GF(211)]);
}

#[test]
fn test_chien3() {
    let c = [GF(1), GF(0)];
    let zeros = chien_search(&c);
    assert_eq!(&zeros, &[GF(0)]);
}

#[test]
fn test_chien4() {
    let c = [GF(2), GF(1)];
    let zeros = chien_search(&c);
    assert_eq!(&zeros, &[-GF(1) / GF(2)]);
}
