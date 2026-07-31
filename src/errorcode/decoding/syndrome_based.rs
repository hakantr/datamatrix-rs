//! Reed–Solomon code'larını decode etmek için Peterson–Gorenstein–Zierler
//! algoritmasının implementasyonu.

use super::ErrorDecodingError;
use crate::SymbolSize;
use crate::errorcode::GF;

#[cfg(test)]
use alloc::boxed::Box;
use alloc::{vec, vec::Vec};

#[cfg(test)]
use pretty_assertions::assert_eq;

/// Reed–Solomon code'u syndrome tabanlı decoder ile decode eder.
///
/// Implementasyon ayrıntıları için [modül dokümantasyonuna](crate::errorcode) bakın.
///
/// # Parametreler
///
/// Symbol'ün `codewords` değerleri ve `size` değeri. Hatalar yerinde düzeltilir.
///
/// Büyük symbol'lerde error code'lar belirli bir biçimde interleaved edilir.
/// Decoder bu düzeni dikkate alır; ayrıntılar için specification'a bakın.
pub fn decode(codewords: &mut [u8], size: SymbolSize) -> Result<(), ErrorDecodingError> {
    let setup = size.block_setup();
    let err_len = setup.num_ecc_per_block;
    let stride = setup.num_ecc_blocks;
    let num_data = size.num_data_codewords();

    let Some(num_error) = stride.checked_mul(err_len) else {
        crate::invariant_violation("error codeword sayısı hesaplanırken taşma oluştu");
    };
    let Some(expected) = num_data.checked_add(num_error) else {
        crate::invariant_violation("toplam codeword sayısı hesaplanırken taşma oluştu");
    };
    if codewords.len() != expected {
        return Err(ErrorDecodingError::DataSize {
            expected,
            actual: codewords.len(),
        });
    }
    if stride == 0 {
        crate::invariant_violation("interleaved block sayısı sıfır olamaz");
    }

    // Square144 için ilk 8 block 218 codeword (156 data codeword), son iki block
    // ise 217 codeword (155 data codeword) içerir. Bu durumda stride 10'dur.
    // Yalnızca step_by(10) kullanmak yanlış error codeword'leri seçer; bu nedenle
    // data ve error bölümleri ayrı ilerletilir.
    let (data, error) = codewords.split_at_mut(num_data);
    for block in 0..setup.num_ecc_blocks {
        let Some(data_block) = data.get_mut(block..) else {
            crate::invariant_violation("interleaved data block başlangıcı bulunamadı");
        };
        let Some(error_block) = error.get_mut(block..) else {
            crate::invariant_violation("interleaved error block başlangıcı bulunamadı");
        };
        decode_gen(
            data_block,
            error_block,
            stride,
            err_len,
            find_inv_error_locations_levinson_durbin,
            find_error_values_bp,
        )?;
    }
    Ok(())
}

fn decode_gen<F, G>(
    data: &mut [u8],
    error: &mut [u8],
    stride: usize,
    err_len: usize,
    inv_error_locs: F,
    find_err_vals: G,
) -> Result<(), ErrorDecodingError>
where
    F: Fn(&[GF]) -> Result<Vec<GF>, ErrorDecodingError>,
    G: Fn(&mut [GF], &[GF], &mut [GF]) -> Result<(), ErrorDecodingError>,
{
    if stride == 0 {
        crate::invariant_violation("Reed–Solomon stride değeri sıfır olamaz");
    }
    let n_data = data.len().div_ceil(stride);
    let n_error = error.len().div_ceil(stride);
    let Some(n) = n_data.checked_add(n_error) else {
        crate::invariant_violation("block codeword sayısı hesaplanırken taşma oluştu");
    };
    if err_len == 0 {
        crate::invariant_violation("generator polynomial derecesi en az 1 olmalı");
    }
    if n <= err_len {
        crate::invariant_violation("data uzunluğu error codeword son ekinden kısa");
    }

    // Bu kod yazılırken Wikipedia'da klasik algoritmanın iyi bir açıklaması vardı:
    //
    //    https://en.wikipedia.org/wiki/Reed%E2%80%93Solomon_error_correction#Peterson%E2%80%93Gorenstein%E2%80%93Zierler_decoder

    // 1. Syndrome değerlerini hesapla.
    let mut syndromes = vec![GF(0); err_len];
    let received = data
        .iter()
        .copied()
        .step_by(stride)
        .chain(error.iter().copied().step_by(stride));
    let have_non_zero = super::primitive_element_evaluation(received, &mut syndromes);
    if !have_non_zero {
        return Ok(());
    }

    // 2a. Hata konumlarını bul.
    let lambda_coeff = inv_error_locs(&syndromes)?;
    let mut inv_error_locations = super::chien_search(&lambda_coeff);
    let lambda_degree = lambda_coeff
        .len()
        .checked_sub(1)
        .ok_or(ErrorDecodingError::Malfunction)?;
    if inv_error_locations.len() != lambda_degree
        || inv_error_locations.first().copied() == Some(GF(0))
    {
        return Err(ErrorDecodingError::Malfunction);
    }

    // 2b. Malfunction durumunu denetle; bkz.
    // M. Srinivasan and D. V. Sarwate, Malfunction in the Peterson-Gorenstein-Zierler Decoder,
    // IEEE Trans. Inf. Theory.
    let t = err_len / 2;
    let v = lambda_degree;
    let twice_t = t.checked_mul(2).ok_or(ErrorDecodingError::Malfunction)?;
    let last_check = twice_t
        .checked_sub(v)
        .and_then(|value| value.checked_sub(1))
        .ok_or(ErrorDecodingError::Malfunction)?;
    for j in t..=last_check {
        let syndrome_tail = syndromes.get(j..).ok_or(ErrorDecodingError::Malfunction)?;
        if syndrome_tail.len() < lambda_coeff.len() {
            return Err(ErrorDecodingError::Malfunction);
        }
        let t_j: GF = syndrome_tail
            .iter()
            .zip(lambda_coeff.iter())
            .map(|(a, b)| *a * *b)
            .sum();
        if t_j != GF(0) {
            return Err(ErrorDecodingError::Malfunction);
        }
    }

    // 3. Hata değerlerini bul; sonuç `syndromes` içinde yerinde hesaplanır.
    find_err_vals(&mut inv_error_locations, &lambda_coeff, &mut syndromes)?;
    let error_locations = inv_error_locations;

    // 4. Hataları düzelt.
    //
    // `position`, block'un kendi codeword dizisindeki sıradır: önce `n_data` data
    // codeword, ardından `n_error` error codeword gelir. Data ve error bölümleri
    // ayrı ayrı interleaved olduğundan block'un k. data codeword'ü `data` dilimi
    // içinde `k * stride`, k. error codeword'ü ise `error` dilimi içinde yine
    // `k * stride` konumundadır. Error konumu bu nedenle `data` diliminin
    // uzunluğundan değil, block'taki data codeword sayısından türetilmelidir;
    // aksi halde `data.len()` stride'ın katı olmadığında (örneğin Square52'nin
    // ikinci block'u veya Square144) yanlış codeword düzeltilir.
    for (loc, err) in error_locations.iter().zip(syndromes.iter()) {
        let i = loc.log();
        let Some(position) = n.checked_sub(i).and_then(|value| value.checked_sub(1)) else {
            return Err(ErrorDecodingError::ErrorsOutsideRange);
        };
        let target = if position < n_data {
            position
                .checked_mul(stride)
                .and_then(|index| data.get_mut(index))
        } else {
            position
                .checked_sub(n_data)
                .and_then(|block_index| block_index.checked_mul(stride))
                .and_then(|index| error.get_mut(index))
        };
        let Some(target) = target else {
            return Err(ErrorDecodingError::ErrorsOutsideRange);
        };
        *target = (GF(*target) - *err).into();
    }

    Ok(())
}

fn value(values: &[GF], index: usize) -> Result<GF, ErrorDecodingError> {
    values
        .get(index)
        .copied()
        .ok_or(ErrorDecodingError::Malfunction)
}

fn set_value(values: &mut [GF], index: usize, new_value: GF) -> Result<(), ErrorDecodingError> {
    let target = values
        .get_mut(index)
        .ok_or(ErrorDecodingError::Malfunction)?;
    *target = new_value;
    Ok(())
}

fn values(values: &[GF], start: usize, end: usize) -> Result<&[GF], ErrorDecodingError> {
    values
        .get(start..end)
        .ok_or(ErrorDecodingError::Malfunction)
}

fn values_mut(
    values: &mut [GF],
    start: usize,
    end: usize,
) -> Result<&mut [GF], ErrorDecodingError> {
    values
        .get_mut(start..end)
        .ok_or(ErrorDecodingError::Malfunction)
}

fn values_inclusive(source: &[GF], start: usize, end: usize) -> Result<&[GF], ErrorDecodingError> {
    let end = end.checked_add(1).ok_or(ErrorDecodingError::Malfunction)?;
    values(source, start, end)
}

fn checked_add(a: usize, b: usize) -> Result<usize, ErrorDecodingError> {
    a.checked_add(b).ok_or(ErrorDecodingError::Malfunction)
}

fn checked_mul(a: usize, b: usize) -> Result<usize, ErrorDecodingError> {
    a.checked_mul(b).ok_or(ErrorDecodingError::Malfunction)
}

/// Syndrome matrix'in Hankel matrix olmasından yararlanarak hata konumlarını bulur.
///
/// Schmidt ve Fettweis'in "Levinson-Durbin Algorithm Used For Fast BCH Decoding" makalesine bakın.
fn find_inv_error_locations_levinson_durbin(syn: &[GF]) -> Result<Vec<GF>, ErrorDecodingError> {
    let t = syn.len() / 2;
    if t == 0 {
        return Err(ErrorDecodingError::TooManyErrors);
    }

    // H_v nonsingular olacak şekilde en küçük v değerini bul.
    let mut v = syn.iter().take_while(|s| **s == GF(0)).count() + 1;
    if v > t {
        return Err(ErrorDecodingError::TooManyErrors);
    }

    // y = [1/b_v, 0, ..., 0] başlangıç değerini oluştur.
    let mut y = Vec::with_capacity(t);
    let v_minus_one = v.checked_sub(1).ok_or(ErrorDecodingError::Malfunction)?;
    let initial_pivot = value(syn, v_minus_one)?;
    if initial_pivot == GF(0) {
        return Err(ErrorDecodingError::TooManyErrors);
    }
    y.push(GF(1) / initial_pivot);
    y.extend(core::iter::repeat_n(GF(0), v_minus_one));

    // w başlangıç değerini oluştur ve H_v w = h_v alt-sağ üçgensel sistemini çöz.
    let mut w = Vec::<GF>::with_capacity(t);
    let initial_end = checked_mul(2, v)?
        .checked_sub(1)
        .ok_or(ErrorDecodingError::Malfunction)?;
    w.extend(values_inclusive(syn, v, initial_end)?.iter().rev());
    for i in 0..v {
        let target_index = v
            .checked_sub(1)
            .and_then(|value| value.checked_sub(i))
            .ok_or(ErrorDecodingError::Malfunction)?;
        let start = v.checked_sub(i).ok_or(ErrorDecodingError::Malfunction)?;
        for j in start..v {
            let wj = value(&w, j)?;
            let syndrome = value(syn, checked_add(i, j)?)?;
            let updated = value(&w, target_index)? - syndrome * wj;
            set_value(&mut w, target_index, updated)?;
        }
        let pivot = value(syn, v_minus_one)?;
        if pivot == GF(0) {
            return Err(ErrorDecodingError::TooManyErrors);
        }
        let updated = value(&w, target_index)? / pivot;
        set_value(&mut w, target_index, updated)?;
    }

    fn dot(a: &[GF], b: &[GF]) -> Result<GF, ErrorDecodingError> {
        if a.len() != b.len() {
            return Err(ErrorDecodingError::Malfunction);
        }
        Ok(a.iter().zip(b.iter()).map(|(p, q)| *p * *q).sum())
    }

    let mut tmp = Vec::with_capacity(t);
    let mut sigma = Vec::with_capacity(t);
    let mut gamma = Vec::with_capacity(t);

    while v < t {
        // tmp = [w_v, -1] değerini hesapla.
        tmp.clear();
        tmp.extend_from_slice(&w);
        tmp.push(-GF(1));

        let twice_v = checked_mul(2, v)?;
        let eps_v = dot(values_inclusive(syn, v, twice_v)?, &tmp)?;
        if eps_v != GF(0) {
            // Düzenli durum.

            // 1. w = [0, w]; sonraki adımlar denklem (6)'nın parçalarıdır.
            w.insert(0, GF(0));

            // 2. w[..v] -= eps_v * y
            for (wi, yi) in values_mut(&mut w, 0, v)?.iter_mut().zip(y.iter()) {
                *wi -= eps_v * *yi;
            }

            let beta_start = checked_add(v, 1)?;
            let beta_end = checked_add(twice_v, 1)?;
            let beta = dot(values_inclusive(syn, beta_start, beta_end)?, &tmp)? / eps_v;
            let gamma_end = twice_v
                .checked_sub(1)
                .ok_or(ErrorDecodingError::Malfunction)?;
            let gamma_value = dot(values_inclusive(syn, v, gamma_end)?, &y)?;
            // 3. w -= (beta - gamma) * eps * y_{v+1}
            for (wi, ti) in w.iter_mut().zip(tmp.iter()) {
                *wi -= (beta - gamma_value) * *ti;
            }

            // 4. y = [w_v, -1] / eps_v, eq. (5)
            let eps_inv = GF(1) / eps_v;
            for (yi, ti) in y.iter_mut().zip(tmp.iter()) {
                *yi = *ti * eps_inv;
            }
            y.push(-eps_inv);
            v += 1;
        } else {
            // Tekil durum; istatistiksel olarak seyrektir.

            // Denklem (7)'deki m değerini bul; çoğunlukla m = 1 olur.
            let search_end = t.checked_sub(v).ok_or(ErrorDecodingError::Malfunction)?;
            let mut found = None;
            for i in 1..search_end {
                let start = checked_add(v, i)?;
                let end = checked_add(twice_v, i)?;
                let sigma_i = dot(values_inclusive(syn, start, end)?, &tmp)?;
                if sigma_i == GF(0) {
                    continue;
                } else {
                    found = Some((i, sigma_i));
                    break;
                }
            }
            let (m, sigma_m) = if let Some((m, sigma_m)) = found {
                (m, sigma_m)
            } else {
                break;
            };
            let n = checked_add(m, v)?;

            // Daha sonra kullanılacak sigma_i değerlerini hesapla (denklem 7).
            sigma.clear();
            sigma.push(sigma_m);
            let sigma_start = checked_add(m, 1)?;
            let sigma_end = checked_mul(2, m)?;
            for k in sigma_start..=sigma_end {
                let start = checked_add(v, k)?;
                let end = checked_add(twice_v, k)?;
                sigma.push(dot(values_inclusive(syn, start, end)?, &tmp)?);
            }
            if sigma.len() != checked_add(m, 1)? {
                return Err(ErrorDecodingError::Malfunction);
            }

            // w^k değerleri üzerinde ilerle ve sonucu tmp içinde tut (denklem 8).
            tmp.pop().ok_or(ErrorDecodingError::Malfunction)?; // tmp = w_v
            for k in 0..=m {
                let rho_index = checked_add(twice_v, k)?;
                let dot_end = twice_v
                    .checked_sub(1)
                    .ok_or(ErrorDecodingError::Malfunction)?;
                let rho = value(syn, rho_index)? - dot(values_inclusive(syn, v, dot_end)?, &tmp)?;
                let eta_index = v.checked_sub(1).ok_or(ErrorDecodingError::Malfunction)?;
                let eta = value(&tmp, eta_index)?;
                // 1. w^k = U * w_k; değerleri sağa kaydır.
                tmp.pop().ok_or(ErrorDecodingError::Malfunction)?;
                tmp.insert(0, GF(0));
                // 2. w^k += rho * y + eta * w_v
                for (wki, (yi, wi)) in tmp.iter_mut().zip(y.iter().zip(w.iter())) {
                    *wki += rho * *yi + eta * *wi;
                }
            }

            // y değerini güncelle (denklem 11).
            y.clear();
            y.resize(checked_add(n, 1)?, GF(0));
            let sigma_m_inv = GF(1) / sigma_m;
            for (yi, w) in y.iter_mut().zip(w.iter()) {
                *yi = *w * sigma_m_inv;
            }
            set_value(&mut y, w.len(), -sigma_m_inv)?;

            // gamma değerini hesapla (denklem 10).
            gamma.clear();
            for i in 0..=m {
                let target_index = checked_add(checked_add(checked_add(n, v)?, 1)?, i)?;
                let start = checked_add(v, i)?;
                let end = checked_add(
                    twice_v
                        .checked_sub(1)
                        .ok_or(ErrorDecodingError::Malfunction)?,
                    i,
                )?;
                gamma.push(
                    value(syn, target_index)? - dot(values_inclusive(syn, start, end)?, &tmp)?,
                );
            }
            for i in 0..=m {
                for j in 0..i {
                    let gj = value(&gamma, j)?;
                    let sigma_value = value(
                        &sigma,
                        i.checked_sub(j).ok_or(ErrorDecodingError::Malfunction)?,
                    )?;
                    let updated = value(&gamma, i)? - sigma_value * gj;
                    set_value(&mut gamma, i, updated)?;
                }
                let divisor = value(&sigma, 0)?;
                if divisor == GF(0) {
                    return Err(ErrorDecodingError::Malfunction);
                }
                let updated = value(&gamma, i)? / divisor;
                set_value(&mut gamma, i, updated)?;
            }

            if cfg!(debug_assertions) {
                for i in 0..=m {
                    let mut row = GF(0);
                    for j in 0..=i {
                        row += value(
                            &sigma,
                            i.checked_sub(j).ok_or(ErrorDecodingError::Malfunction)?,
                        )? * value(&gamma, j)?;
                    }
                    let target_index = checked_add(checked_add(checked_add(n, v)?, 1)?, i)?;
                    let start = checked_add(v, i)?;
                    let end = checked_add(
                        twice_v
                            .checked_sub(1)
                            .ok_or(ErrorDecodingError::Malfunction)?,
                        i,
                    )?;
                    let target =
                        value(syn, target_index)? - dot(values_inclusive(syn, start, end)?, &tmp)?;
                    if row != target {
                        return Err(ErrorDecodingError::Malfunction);
                    }
                }
            }

            // w değerini güncelle; önce sonucu tmp içinde hesapla (denklem 9).
            tmp.resize(checked_add(n, 1)?, GF(0)); // tmp = I_{n+1,v}^0 w^{m+1}_v
            for (i, gamma_i) in gamma.iter().enumerate() {
                let start = m.checked_sub(i).ok_or(ErrorDecodingError::Malfunction)?;
                let tmp_len = tmp.len();
                for (ti, wj) in values_mut(&mut tmp, start, tmp_len)?
                    .iter_mut()
                    .zip(w.iter())
                {
                    *ti += *gamma_i * *wj;
                }
                let target_index = checked_add(start, v)?;
                let updated = value(&tmp, target_index)? - *gamma_i;
                set_value(&mut tmp, target_index, updated)?;
            }
            core::mem::swap(&mut tmp, &mut w);

            v = checked_add(n, 1)?;
        }

        if cfg!(debug_assertions) {
            if w.len() != v || y.len() != v {
                return Err(ErrorDecodingError::Malfunction);
            }

            // Denklem (3)'ü doğrula.
            for i in 0..v {
                let mut row = GF(0);
                for j in 0..v {
                    row += value(syn, checked_add(i, j)?)? * value(&y, j)?;
                }
                let target = if i == v - 1 { GF(1) } else { GF(0) };
                if row != target {
                    return Err(ErrorDecodingError::Malfunction);
                }
            }
            // Denklem (4)'ü doğrula.
            for i in 0..v {
                let mut row = GF(0);
                for j in 0..v {
                    row += value(syn, checked_add(i, j)?)? * value(&w, j)?;
                }
                if row != value(syn, checked_add(v, i)?)? {
                    return Err(ErrorDecodingError::Malfunction);
                }
            }
        }
    }

    w.push(GF(1));
    Ok(w)
}

/// Katsayı matrix sistemini Björck–Pereyra algoritmasıyla çözerek hata değerlerini bulur.
///
/// O(t^2) karmaşıklığında çalışır.
fn find_error_values_bp(
    x_loc: &mut [GF],
    _lambda: &[GF],
    syn: &mut [GF],
) -> Result<(), ErrorDecodingError> {
    let e = x_loc.len();
    if e == 0 || syn.len() < e {
        return Err(ErrorDecodingError::Malfunction);
    }
    // Hata konumlarını elde etmek için sıfırların tersini hesapla.
    for z in x_loc.iter_mut() {
        // coeff içindeki sabit katsayı 1 olduğundan z sıfır olamaz.
        if *z == GF(0) {
            return Err(ErrorDecodingError::Malfunction);
        }
        *z = GF(1) / *z;
    }
    // Katsayı matrisi, diagonal matrix ile Vandermonde matrix çarpımıdır.
    // Önce Vandermonde sistemini Björck-Pereyra algoritmasıyla çöz.
    for (k, x_loc_k) in x_loc.iter().enumerate().take(e - 1) {
        for j in (k + 1..e).rev() {
            let previous = j.checked_sub(1).ok_or(ErrorDecodingError::Malfunction)?;
            let tmp = value(syn, previous)?;
            let updated = value(syn, j)? - *x_loc_k * tmp;
            set_value(syn, j, updated)?;
        }
    }
    for k in (0..e - 1).rev() {
        for j in k + 1..e {
            let previous = j
                .checked_sub(k)
                .and_then(|value| value.checked_sub(1))
                .ok_or(ErrorDecodingError::Malfunction)?;
            let divisor = value(x_loc, j)? - value(x_loc, previous)?;
            if divisor == GF(0) {
                return Err(ErrorDecodingError::Malfunction);
            }
            let updated = value(syn, j)? / divisor;
            set_value(syn, j, updated)?;
        }
        for j in k..e - 1 {
            let next = j.checked_add(1).ok_or(ErrorDecodingError::Malfunction)?;
            let updated = value(syn, j)? - value(syn, next)?;
            set_value(syn, j, updated)?;
        }
    }
    // Son olarak diagonal sistemi çöz.
    for i in 0..e {
        let divisor = value(x_loc, i)?;
        if divisor == GF(0) {
            return Err(ErrorDecodingError::Malfunction);
        }
        let updated = value(syn, i)? / divisor;
        set_value(syn, i, updated)?;
    }
    Ok(())
}

/// Hata konumlarını bulmak için Berlekamp–Massey (BM) algoritması.
#[allow(unused)]
fn find_inv_error_locations_bm(syn: &[GF]) -> Result<Vec<GF>, ErrorDecodingError> {
    let mut len_lfsr = 0; // LFSR'nin geçerli uzunluğu
    let mut cur = vec![GF(1)]; // Geçerli connection polynomial
    let mut prev = vec![GF(1)]; // Son uzunluk değişiminden önceki connection polynomial
    let mut l = 1; // l = k - m; güncellemedeki shift miktarı
    let mut discrepancy_m = GF(1); // Önceki discrepancy
    for k in 0..syn.len() {
        // Discrepancy değerini hesapla.
        let cur_tail = values(&cur, 1, cur.len())?;
        let syn_prefix = values(syn, 0, k)?;
        let discrepancy = value(syn, k)?
            + cur_tail
                .iter()
                .zip(syn_prefix.iter().rev())
                .map(|(a, b)| *a * *b)
                .sum();
        if discrepancy == GF(0) {
            l += 1;
        } else if 2 * len_lfsr > k {
            // Uzunluğu değiştirmeden günceller.
            let tmp = discrepancy / discrepancy_m;
            let cur_len = cur.len();
            for (ci, pj) in values_mut(&mut cur, l, cur_len)?
                .iter_mut()
                .zip(prev.iter())
            {
                *ci -= tmp * *pj;
            }
            l += 1;
        } else {
            // cur değerini uzunluk değişimiyle günceller.
            let cur_clone_before = cur.clone();
            let tmp = discrepancy / discrepancy_m;
            cur.resize(checked_add(l, prev.len())?, GF(0));
            let cur_len = cur.len();
            for (ci, pj) in values_mut(&mut cur, l, cur_len)?
                .iter_mut()
                .zip(prev.iter())
            {
                *ci -= tmp * *pj;
            }
            len_lfsr = k + 1 - len_lfsr;
            prev = cur_clone_before;
            discrepancy_m = discrepancy;
            l = 1;
        }
    }

    if cur
        .len()
        .checked_sub(1)
        .ok_or(ErrorDecodingError::Malfunction)?
        > syn.len() / 2
    {
        Err(ErrorDecodingError::TooManyErrors)
    } else {
        cur.reverse();
        Ok(cur)
    }
}

/// Syndrome matrix denklemini v, v-1, ..., 1 için LU decomposition kullanarak çözer.
#[allow(unused)]
fn find_inv_error_locations_lu(syndomes: &[GF]) -> Result<Vec<GF>, ErrorDecodingError> {
    let v = syndomes.len() / 2;

    // 1. adım: Error locator polynomial değerini bulur.

    // Syndrome matrix'i oluşturur.
    let mut matrix = vec![GF(0); checked_mul(v, v)?]; // Row-major sıra
    for i in 0..v {
        for j in 0..v {
            let matrix_index = checked_add(checked_mul(i, v)?, j)?;
            let syndrome_index = checked_add(i, j)?;
            set_value(&mut matrix, matrix_index, value(syndomes, syndrome_index)?)?;
        }
    }

    // Azalan v değerleri için çözmeyi dener.
    let mut coeff = vec![];
    for vi in (1..=v).rev() {
        let mut m = matrix.clone();
        let end = checked_mul(2, vi)?;
        let mut b: Vec<GF> = values(syndomes, vi, end)?.into();
        if b.len() != vi {
            return Err(ErrorDecodingError::Malfunction);
        }
        if super::solve(&mut m, &mut b, v) {
            b.truncate(vi);
            coeff = b;
            break;
        }
    }
    if coeff.is_empty() {
        // Bütün syndrome değerleri sıfırsa bu yöntem çağrılmaz; yine de güvenli
        // biçimde hata döndürür.
        return Err(ErrorDecodingError::TooManyErrors);
    }
    coeff.push(GF(1));
    Ok(coeff)
}

/// Forney algoritmasıyla hata değerlerini bulur.
///
/// # Parametreler
///
/// - `inv_x_locs`, hata konumlarının terslerinin listesidir.
/// - `lambda`, error locator polynomial katsayılarının en yüksekten başlayan listesidir.
/// - `syn`, syndrome değerleridir.
#[allow(unused)]
fn find_error_values_forney(
    inv_x_locs: &mut [GF],
    lambda: &[GF],
    syn: &mut [GF],
) -> Result<(), ErrorDecodingError> {
    let n = syn.len();
    // Lambda(x) * S(x) mod x^n değerini hesapla.
    let mut omega = vec![GF(0); n];
    for (i, si) in syn.iter().copied().enumerate() {
        // si, i kuvvetli x teriminin katsayısıdır.
        let remaining = n.checked_sub(i).ok_or(ErrorDecodingError::Malfunction)?;
        for (j, lj) in lambda.iter().rev().take(remaining).copied().enumerate() {
            let offset = checked_add(j, i)?;
            let index = n
                .checked_sub(1)
                .and_then(|value| value.checked_sub(offset))
                .ok_or(ErrorDecodingError::Malfunction)?;
            let updated = value(&omega, index)? + lj * si;
            set_value(&mut omega, index, updated)?;
        }
    }

    for (x_inv, out) in inv_x_locs.iter().copied().zip(syn.iter_mut()) {
        let mut omega_x = GF(0);
        for o in omega.iter().copied() {
            omega_x = omega_x * x_inv + o;
        }

        let mut lambda_der_x = GF(0);
        let lambda_end = lambda
            .len()
            .checked_sub(1)
            .ok_or(ErrorDecodingError::Malfunction)?;
        for (k, lk) in values(lambda, 0, lambda_end)?.iter().copied().enumerate() {
            // lk değerinin usize ile çarpıldığına dikkat edin. Bu GF içinde çarpma
            // değildir; GF için Mul<usize> implementasyonuna bakın.
            lambda_der_x = lambda_der_x * x_inv + lk * (lambda.len() - k - 1);
        }

        if lambda_der_x == GF(0) {
            return Err(ErrorDecodingError::Malfunction);
        }
        *out = -omega_x / lambda_der_x;
    }

    // Hata konumlarını elde etmek için x_inv değerinin tersini hesaplar.
    for z in inv_x_locs.iter_mut() {
        // coeff içindeki sabit katsayı 1 olduğundan z hiçbir zaman 0 değildir;
        // dolayısıyla 0, polynomial'ın sıfırı değildir.
        if *z == GF(0) {
            return Err(ErrorDecodingError::Malfunction);
        }
        *z = GF(1) / *z;
    }
    Ok(())
}

#[test]
fn solve_vandermonde_diag() -> Result<(), ErrorDecodingError> {
    let mut x = [GF(28), GF(181), GF(59), GF(129), GF(189)];
    for tmp in x.iter_mut() {
        *tmp = GF(1) / *tmp;
    }
    let mut rhs = [GF(66), GF(27), GF(189), GF(255), GF(206)];
    let mut y1 = rhs;
    find_error_values_bp(&mut x[..], &[], &mut y1[..])?;

    let mut mat = [
        GF(28),
        GF(181),
        GF(59),
        GF(129),
        GF(189),
        GF(125),
        GF(250),
        GF(220),
        GF(115),
        GF(186),
        GF(85),
        GF(18),
        GF(99),
        GF(40),
        GF(254),
        GF(66),
        GF(37),
        GF(133),
        GF(22),
        GF(175),
        GF(251),
        GF(255),
        GF(1),
        GF(36),
        GF(15),
    ];
    super::solve(&mut mat, &mut rhs, x.len());

    assert_eq!(&rhs[..], &y1);
    Ok(())
}

#[cfg(test)]
fn set_test_codeword(
    codewords: &mut [u8],
    index: usize,
    value: u8,
) -> Result<(), ErrorDecodingError> {
    let codeword = codewords
        .get_mut(index)
        .ok_or(ErrorDecodingError::Malfunction)?;
    *codeword = value;
    Ok(())
}

/// Verilen block'un içerdiği data codeword sayısını döndürür.
#[cfg(test)]
fn data_codewords_in_block(size: SymbolSize, block: usize) -> usize {
    size.num_data_codewords()
        .saturating_sub(block)
        .div_ceil(size.block_setup().num_ecc_blocks)
}

/// Verilen block'un `index`. codeword'ünün interleaved dizideki konumunu döndürür.
///
/// Data ve error bölümleri ayrı ayrı interleaved edilir; bu nedenle error
/// codeword'lerinin konumu data bölümünün uzunluğuna göre kaydırılır.
#[cfg(test)]
fn interleaved_position(size: SymbolSize, block: usize, index: usize) -> usize {
    let setup = size.block_setup();
    let num_data = size.num_data_codewords();
    let data_per_block = data_codewords_in_block(size, block);
    if index < data_per_block {
        block + index * setup.num_ecc_blocks
    } else {
        num_data + block + (index - data_per_block) * setup.num_ecc_blocks
    }
}

/// Her symbol size ve her interleaved block için tek codeword hatası
/// düzeltilebilmelidir.
///
/// Regresyon: interleaved boyutlarda (Square52 ve üstü) error codeword
/// bölgesindeki hatalar yanlış konuma eşleniyor, bu da ya başka bir codeword'ün
/// bozulmasına ya da `ErrorsOutsideRange` ile symbol'ün tümden reddedilmesine
/// yol açıyordu.
#[test]
fn test_single_error_in_every_block_is_corrected() -> Result<(), Box<dyn std::error::Error>> {
    let mut random_data = crate::test::random_data();
    for size in crate::SymbolList::all() {
        let setup = size.block_setup();
        let num_data = size.num_data_codewords();
        let data = random_data(num_data);
        let ecc = crate::errorcode::encode_error(&data, size)?;
        let mut correct = data.clone();
        correct.extend_from_slice(&ecc);

        for block in 0..setup.num_ecc_blocks {
            // Block'un ilk data, son data, ilk error ve son error codeword'ü.
            let block_data = data_codewords_in_block(size, block);
            let block_len = block_data + setup.num_ecc_per_block;
            for index in [
                0,
                block_data.saturating_sub(1),
                block_data,
                block_len.saturating_sub(1),
            ] {
                let position = interleaved_position(size, block, index);
                let mut received = correct.clone();
                let original = received
                    .get(position)
                    .copied()
                    .ok_or(ErrorDecodingError::Malfunction)?;
                set_test_codeword(&mut received, position, original ^ 0x5A)?;
                decode(&mut received, size)?;
                assert_eq!(
                    received, correct,
                    "{size:?}: block {block}, codeword {index} (konum {position})"
                );
            }
        }
    }
    Ok(())
}

/// Block başına düzeltilebilir azami hata sayısı gerçekten düzeltilebilmelidir.
#[test]
fn test_maximum_errors_per_block_are_corrected() -> Result<(), Box<dyn std::error::Error>> {
    let mut random_data = crate::test::random_data();
    for size in crate::SymbolList::all() {
        let setup = size.block_setup();
        let num_data = size.num_data_codewords();
        let data = random_data(num_data);
        let ecc = crate::errorcode::encode_error(&data, size)?;
        let mut correct = data.clone();
        correct.extend_from_slice(&ecc);

        let correctable = setup.num_ecc_per_block / 2;
        for block in 0..setup.num_ecc_blocks {
            let block_len = data_codewords_in_block(size, block) + setup.num_ecc_per_block;
            let mut received = correct.clone();
            for error in 0..correctable {
                // Hataları block içinde eşit aralıklarla dağıt.
                let index = (error * block_len) / correctable.max(1);
                let position = interleaved_position(size, block, index);
                let original = received
                    .get(position)
                    .copied()
                    .ok_or(ErrorDecodingError::Malfunction)?;
                set_test_codeword(&mut received, position, original ^ 0xA5)?;
            }
            decode(&mut received, size)?;
            assert_eq!(
                received, correct,
                "{size:?}: block {block} içinde {correctable} hata düzeltilemedi"
            );
        }
    }
    Ok(())
}

#[test]
fn test_recovery() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![1, 2, 3];
    let ecc = crate::errorcode::encode_error(&data, SymbolSize::Square10)?;
    data.extend_from_slice(&ecc);
    assert_eq!(data.len(), 3 + 5);
    let mut received = data.clone();
    // İki codeword'ü bozar.
    set_test_codeword(&mut received, 0, 230)?;
    set_test_codeword(&mut received, 7, 32)?;
    decode(&mut received, SymbolSize::Square10)?;
    assert_eq!(&data, &received);
    Ok(())
}

#[test]
fn test_recovery1() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![
        255, 255, 255, 72, 52, 38, 52, 52, 52, 52, 52, 52, 52, 52, 52, 72, 0, 0, 72, 0, 0, 10,
    ];
    let ecc = crate::errorcode::encode_error(&data, SymbolSize::Square20)?;
    data.extend_from_slice(&ecc);
    let mut received = data.clone();
    set_test_codeword(&mut received, 0, 52)?;
    set_test_codeword(&mut received, 8, 144)?;
    decode(&mut received, SymbolSize::Square20)?;
    assert_eq!(&data, &received);
    Ok(())
}

#[test]
fn test_recovery2() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![
        144, 144, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        255, 255, 255, 255,
    ];
    let ecc = crate::errorcode::encode_error(&data, SymbolSize::Square20)?;
    data.extend_from_slice(&ecc);
    let mut received = data.clone();
    set_test_codeword(&mut received, 1, 32)?;
    set_test_codeword(&mut received, 0, 0)?;
    set_test_codeword(&mut received, 12, 0)?;
    set_test_codeword(&mut received, 16, 144)?;
    decode(&mut received, SymbolSize::Square20)?;
    assert_eq!(&data, &received);
    Ok(())
}

#[test]
fn test_recovery3() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![
        255, 23, 189, 54, 189, 189, 189, 189, 255, 255, 255, 255, 255, 255, 255, 67, 4, 0, 255,
        189, 48, 37,
    ];
    let ecc = crate::errorcode::encode_error(&data, SymbolSize::Square20)?;
    data.extend_from_slice(&ecc);
    let mut received = data.clone();
    set_test_codeword(&mut received, 0, 247)?;
    set_test_codeword(&mut received, 1, 49)?;
    set_test_codeword(&mut received, 5, 255)?;
    set_test_codeword(&mut received, 6, 0)?;
    set_test_codeword(&mut received, 8, 49)?;
    set_test_codeword(&mut received, 10, 0)?;
    set_test_codeword(&mut received, 12, 65)?;
    set_test_codeword(&mut received, 15, 177)?;
    set_test_codeword(&mut received, 16, 32)?;
    decode(&mut received, SymbolSize::Square20)?;
    assert_eq!(&data, &received);
    Ok(())
}

#[test]
fn test_recovery4() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = vec![
        49, 95, 49, 44, 49, 49, 0, 0, 0, 32, 255, 247, 255, 254, 189, 189, 189, 189, 189, 189, 189,
        189,
    ];
    let ecc = crate::errorcode::encode_error(&data, SymbolSize::Square20)?;
    data.extend_from_slice(&ecc);
    let mut received = data.clone();
    set_test_codeword(&mut received, 1, 49)?;
    set_test_codeword(&mut received, 0, 44)?;
    set_test_codeword(&mut received, 13, 49)?;
    set_test_codeword(&mut received, 10, 0)?;
    set_test_codeword(&mut received, 8, 101)?;
    set_test_codeword(&mut received, 15, 54)?;
    set_test_codeword(&mut received, 6, 206)?;
    set_test_codeword(&mut received, 21, 191)?;
    set_test_codeword(&mut received, 5, 50)?;
    decode(&mut received, SymbolSize::Square20)?;
    assert_eq!(&data, &received);
    Ok(())
}
