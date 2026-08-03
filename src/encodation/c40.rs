use arrayvec::ArrayVec;

#[cfg(test)]
use alloc::{vec, vec::Vec};

#[cfg(test)]
use pretty_assertions::assert_eq;

use super::{DataEncodingError, EncodingContext, ascii};

const SHIFT1: u8 = 0;
const SHIFT2: u8 = 1;
const SHIFT3: u8 = 2;
const UPPER_SHIFT: u8 = 30;

#[inline(always)]
pub(super) fn low_ascii_to_c40_symbols(ctx: &mut ArrayVec<u8, 6>, ch: u8) {
    let mut push = |value| {
        if ctx.try_push(value).is_err() {
            crate::invariant_violation("C40 geçici symbol arabelleği kapasitesini aştı");
        }
    };
    match ch {
        // Temel set
        b' ' => push(3),
        ch @ b'0'..=b'9' => push(ch - b'0' + 4),
        ch @ b'A'..=b'Z' => push(ch - b'A' + 14),
        // Shift 1 set
        ch @ 0..=31 => {
            push(SHIFT1);
            push(ch);
        }
        // Shift 2 set
        ch @ 33..=47 => {
            push(SHIFT2);
            push(ch - 33);
        }
        ch @ 58..=64 => {
            push(SHIFT2);
            push(ch - 58 + 15);
        }
        ch @ 91..=95 => {
            push(SHIFT2);
            push(ch - 91 + 22);
        }
        // Shift 3
        ch @ 96..=127 => {
            push(SHIFT3);
            push(ch - 96);
        }
        _ => {
            crate::invariant_violation("C40 dönüşümüne 7-bit aralığı dışında karakter verildi");
        }
    }
}

pub(super) fn in_base_set(ch: u8) -> bool {
    matches!(ch, b' ' | b'0'..=b'9' | b'A'..=b'Z')
}

pub(super) fn val_size(ch: u8) -> u8 {
    match ch {
        b' ' | b'0'..=b'9' | b'A'..=b'Z' => 1,
        0..=31 | 33..=47 | 58..=64 | 91..=127 => 2,
        ch => 2 + val_size(ch - 128),
    }
}

/// Üç C40 değerini iki codeword olarak encode eder.
pub(super) fn write_three_values<T: EncodingContext>(ctx: &mut T, c1: u8, c2: u8, c3: u8) {
    let enc = 1600 * c1 as u16 + 40 * c2 as u16 + c3 as u16 + 1;
    ctx.push((enc >> 8) as u8);
    ctx.push((enc & 0xFF) as u8);
}

pub(super) fn handle_end<T>(
    ctx: &mut T,
    last_ch: u8,
    buf: ArrayVec<u8, 6>,
    last_char_value_count: usize,
) -> Result<(), DataEncodingError>
where
    T: EncodingContext,
{
    if buf.len() > 2 {
        crate::invariant_violation("C40 end-of-data arabelleğinde ikiden fazla symbol kaldı");
    }

    let mode_switch = ctx.has_more_characters();

    if mode_switch {
        // Planner yalnızca tamamlanmış C40/Text üçlülerinin ardından mode
        // değiştirebilir. Eksik grup burada doldurulamaz.
        if !buf.is_empty() {
            return Err(DataEncodingError::TooMuchOrIllegalData);
        }
        ctx.push(super::UNLATCH);
        return Ok(());
    }

    // 7.2.5.3 b: iki değer ve tam iki codeword yuvası kaldığında Shift 1
    // üçüncü değer olarak eklenir; final UNLATCH kullanılmaz.
    if buf.len() == 2 && ctx.symbol_size_left(2) == Some(0) {
        let mut values = buf.iter().copied();
        let Some(c1) = values.next() else {
            crate::invariant_violation("C40 end-of-data arabelleğinde ilk symbol bulunamadı");
        };
        let Some(c2) = values.next() else {
            crate::invariant_violation("C40 end-of-data arabelleğinde ikinci symbol bulunamadı");
        };
        // Kural b'nin tanımladığı tek dolgu değeri.
        write_three_values(ctx, c1, c2, SHIFT1);
        return Ok(());
    }

    // 7.2.5.3 c/d yalnızca tek, shiftsiz C40/Text değeri kalan bir veri
    // karakterine uygulanabilir. Son karakteri geri alıp ASCII ile yazarız.
    if buf.len() == 1 && last_char_value_count == 1 && ascii::encoding_size(&[last_ch]) == 1 {
        let implicit_unlatch = ctx.symbol_size_left(1) == Some(0);
        if !implicit_unlatch {
            ctx.symbol_size_left(2)
                .ok_or(DataEncodingError::TooMuchOrIllegalData)?;
            ctx.push(super::UNLATCH);
        }
        ctx.set_ascii_until_end();
        ctx.backup(1);
        return Ok(());
    }

    if !buf.is_empty() {
        // Bu bitiş standardın a-d durumlarından biri değildir. Geçerli planner
        // alternatifi daha önce ASCII'ye dönmeli veya daha büyük symbol seçmelidir.
        return Err(DataEncodingError::TooMuchOrIllegalData);
    }

    if ctx
        .symbol_size_left(0)
        .ok_or(DataEncodingError::TooMuchOrIllegalData)?
        > 0
    {
        ctx.push(super::UNLATCH);
        ctx.set_ascii_until_end();
    }
    Ok(())
}

pub(super) fn encode_generic<T, F>(ctx: &mut T, low_ascii_write: F) -> Result<(), DataEncodingError>
where
    T: EncodingContext,
    F: Fn(&mut ArrayVec<u8, 6>, u8),
{
    let mut buf = ArrayVec::new();
    let mut last_ch = 0;
    let mut last_char_value_count = 0;
    while let Some(ch) = ctx.eat() {
        let mut vals = ArrayVec::new();
        last_char_value_count = to_vals(&mut vals, ch, &low_ascii_write);
        for val in vals {
            if buf.try_push(val).is_err() {
                crate::invariant_violation("C40 geçici symbol arabelleği kapasitesini aştı");
            }
        }
        last_ch = ch;
        while buf.len() >= 3 {
            let mut values = buf.iter().copied();
            let Some(c1) = values.next() else {
                crate::invariant_violation("C40 arabelleğinde ilk symbol bulunamadı");
            };
            let Some(c2) = values.next() else {
                crate::invariant_violation("C40 arabelleğinde ikinci symbol bulunamadı");
            };
            let Some(c3) = values.next() else {
                crate::invariant_violation("C40 arabelleğinde üçüncü symbol bulunamadı");
            };
            write_three_values(ctx, c1, c2, c3);
            buf.drain(0..3).for_each(core::mem::drop);
        }
        if ctx.maybe_switch_mode() {
            break;
        }
    }
    handle_end(ctx, last_ch, buf, last_char_value_count)
}

fn to_vals<F>(buf: &mut ArrayVec<u8, 6>, ch: u8, low_ascii_write: F) -> usize
where
    F: Fn(&mut ArrayVec<u8, 6>, u8),
{
    let len_before = buf.len();
    match ch {
        ch @ 0..=127 => low_ascii_write(buf, ch),
        ch @ 128..=255 => {
            if buf.try_push(SHIFT2).is_err() || buf.try_push(UPPER_SHIFT).is_err() {
                crate::invariant_violation("C40 geçici symbol arabelleği kapasitesini aştı");
            }
            low_ascii_write(buf, ch - 128);
        }
    };
    let Some(written) = buf.len().checked_sub(len_before) else {
        crate::invariant_violation("C40 arabellek uzunluğu beklenmedik biçimde azaldı");
    };
    written
}

pub(super) fn encode<T: EncodingContext>(ctx: &mut T) -> Result<(), DataEncodingError> {
    encode_generic(ctx, low_ascii_to_c40_symbols)
}

#[cfg(test)]
fn vals(data: &[u8]) -> Vec<u8> {
    let mut vals = Vec::new();
    for ch in data.iter().cloned() {
        let mut buf = ArrayVec::new();
        let _written = to_vals(&mut buf, ch, low_ascii_to_c40_symbols);
        vals.extend(buf.iter());
    }
    vals
}

#[test]
fn test_enc_basic_set() {
    let vals = vals(b" 0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ");
    let out: Vec<u8> = (3..=39).collect();
    assert_eq!(vals, out);
}

#[test]
fn test_enc_shift1_set() {
    let input: Vec<u8> = (0..=31).collect();
    let vals = vals(&input);
    assert_eq!(
        vals,
        vec![
            0, 0, 0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0, 8, 0, 9, 0, 10, 0, 11, 0, 12, 0, 13,
            0, 14, 0, 15, 0, 16, 0, 17, 0, 18, 0, 19, 0, 20, 0, 21, 0, 22, 0, 23, 0, 24, 0, 25, 0,
            26, 0, 27, 0, 28, 0, 29, 0, 30, 0, 31
        ]
    );
}

#[test]
fn test_enc_shift2_set() {
    let vals = vals(b"!\"#$%&'()*+,-./:;<=>?@[\\]^_");
    assert_eq!(
        vals,
        vec![
            1, 0, 1, 1, 1, 2, 1, 3, 1, 4, 1, 5, 1, 6, 1, 7, 1, 8, 1, 9, 1, 10, 1, 11, 1, 12, 1, 13,
            1, 14, 1, 15, 1, 16, 1, 17, 1, 18, 1, 19, 1, 20, 1, 21, 1, 22, 1, 23, 1, 24, 1, 25, 1,
            26
        ]
    );
}

#[test]
fn test_enc_shift3_set() {
    let vals = vals(b"`abcdefghijklmnopqrstuvwxyz{|}~\x7f");
    let test = vec![
        2, 0, 2, 1, 2, 2, 2, 3, 2, 4, 2, 5, 2, 6, 2, 7, 2, 8, 2, 9, 2, 10, 2, 11, 2, 12, 2, 13, 2,
        14, 2, 15, 2, 16, 2, 17, 2, 18, 2, 19, 2, 20, 2, 21, 2, 22, 2, 23, 2, 24, 2, 25, 2, 26, 2,
        27, 2, 28, 2, 29, 2, 30, 2, 31,
    ];
    assert_eq!(vals, test);
}

#[test]
fn test_shift_upper() {
    let vals = vals(b"\x80\xFF\xa0");
    // İlk değer 1, 30, 0, 0'dır.
    // İkinci değer 1, 30, 2, 31'dir.
    // Üçüncü değer 1, 30, 3'tür.
    assert_eq!(vals, vec![1, 30, 0, 0, 1, 30, 2, 31, 1, 30, 3]);
}
