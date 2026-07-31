use arrayvec::ArrayVec;

use super::{DataEncodingError, EncodingContext, ascii};

pub(crate) const UNLATCH: u8 = 0b01_1111;

#[cfg(test)]
use alloc::vec;

#[cfg(test)]
use pretty_assertions::assert_eq;

#[inline]
pub(crate) fn is_encodable(ch: u8) -> bool {
    matches!(ch, 32..=94)
}

/// 1 ile 4 karakteri EDIFACT kullanarak encode eder ve context'e yazar.
fn write4<T: EncodingContext>(ctx: &mut T, s: &ArrayVec<u8, 4>) {
    let s1 = s.get(1).copied().unwrap_or(0) & 0b11_1111;
    let Some(first) = s.first().copied() else {
        crate::invariant_violation("EDIFACT grubu boşken codeword yazılmak istendi");
    };
    ctx.push((first << 2) | (s1 >> 4));

    if s.len() >= 2 {
        let s2 = s.get(2).copied().unwrap_or(0) & 0b11_1111;
        ctx.push((s1 << 4) | (s2 >> 2));

        if s.len() >= 3 {
            let s3 = s.get(3).copied().unwrap_or(0) & 0b11_1111;
            ctx.push((s2 << 6) | s3);
        }
    }
}

fn handle_end<T: EncodingContext>(
    ctx: &mut T,
    mut symbols: ArrayVec<u8, 4>,
) -> Result<(), DataEncodingError> {
    // "En fazla 2 ASCII ile encoding, UNLATCH yok" durumunu denetler.
    let rest_chars = symbols.len() + ctx.characters_left();
    if rest_chars <= 4 {
        // Standart, symbol içinde en fazla 2 codeword alanı kalmışsa ve kalan
        // veri bu alanda ASCII ile encode edilebiliyorsa UNLATCH olmadan ASCII
        // encoding yapılmasına izin verir.
        let rest: ArrayVec<u8, 4> = symbols
            .iter()
            .copied()
            .chain(ctx.rest().iter().copied())
            .collect();
        let ascii_size = ascii::encoding_size(&rest);
        if ascii_size <= 2 {
            match ctx.symbol_size_left(ascii_size).map(|x| x + ascii_size) {
                Some(space) if space <= 2 && ascii_size <= space => {
                    ctx.backup(symbols.len());
                    ctx.set_ascii_until_end();
                    return Ok(());
                }
                _ => (),
            }
        }
    }
    if symbols.is_empty() {
        if !ctx.has_more_characters() {
            // End of data
            let space_left = ctx
                .symbol_size_left(0)
                .ok_or(DataEncodingError::TooMuchOrIllegalData)?;
            // Padding durumu
            if space_left > 0 {
                // Diğer durum yukarıdaki "özel end of data kuralı" ile yakalanır.
                if space_left <= 2 {
                    crate::invariant_violation("EDIFACT end-of-data space hesabı geçersiz");
                }
                ctx.push(UNLATCH << 2);
                ctx.set_ascii_until_end();
            }
        } else {
            // Mode switch
            ctx.push(UNLATCH << 2);
        }
    } else {
        if symbols.len() > 3 {
            crate::invariant_violation(
                "EDIFACT end-of-data arabelleğinde üçten fazla symbol kaldı",
            );
        }
        if !ctx.has_more_characters() {
            // End of data; alan uygunsa padding için UNLATCH eklenebilir.
            let space_left = ctx
                .symbol_size_left(symbols.len())
                .ok_or(DataEncodingError::TooMuchOrIllegalData)?
                > 0;
            if space_left || symbols.len() == 3 {
                if symbols.try_push(UNLATCH).is_err() {
                    crate::invariant_violation("EDIFACT symbol arabelleği kapasitesini aştı");
                }
                ctx.set_ascii_until_end();
            }
        } else {
            if symbols.try_push(UNLATCH).is_err() {
                crate::invariant_violation("EDIFACT symbol arabelleği kapasitesini aştı");
            }
        }
        write4(ctx, &symbols);
    }
    Ok(())
}

pub(super) fn encode<T: EncodingContext>(ctx: &mut T) -> Result<(), DataEncodingError> {
    let mut symbols = ArrayVec::<u8, 4>::new();
    while let Some(ch) = ctx.eat() {
        if symbols.try_push(ch).is_err() {
            crate::invariant_violation("EDIFACT symbol arabelleği kapasitesini aştı");
        }

        if symbols.len() == 4 {
            write4(ctx, &symbols);
            symbols.clear();
            if ctx.maybe_switch_mode() {
                break;
            }
        } else if ctx.maybe_switch_mode() {
            break;
        }
    }
    handle_end(ctx, symbols)
}

#[test]
fn test_write4_four() {
    use super::tests::TestEncodingContext;
    let mut enc = TestEncodingContext::new(vec![], 3, -1);
    write4(&mut enc, &[0b10_01_00, 0b11_01_10, 0b011010, 1].into());
    assert_eq!(
        enc.codewords,
        vec![0b10_01_00_11, 0b01_10_01_10, 0b10_00_00_01]
    );
}

#[test]
fn test_write4_three() {
    use super::tests::TestEncodingContext;
    let mut enc = TestEncodingContext::new(vec![], 3, -1);
    let mut s = ArrayVec::<u8, 4>::new();
    if s.try_extend_from_slice(&[0b10_01_00, 0b11_01_10, 0b011010])
        .is_err()
    {
        crate::invariant_violation("EDIFACT test arabelleği kapasitesini aştı");
    }
    write4(&mut enc, &s);
    assert_eq!(
        enc.codewords,
        vec![0b10_01_00_11, 0b01_10_01_10, 0b10_00_00_00]
    );
}

#[test]
fn test_write4_two() {
    use super::tests::TestEncodingContext;
    let mut enc = TestEncodingContext::new(vec![], 2, -1);
    let mut s = ArrayVec::<u8, 4>::new();
    if s.try_extend_from_slice(&[0b10_01_00, 0b11_01_10]).is_err() {
        crate::invariant_violation("EDIFACT test arabelleği kapasitesini aştı");
    }
    write4(&mut enc, &s);
    assert_eq!(enc.codewords, vec![0b10_01_00_11, 0b01_10_00_00]);
}

#[test]
fn test_write4_one() {
    use super::tests::TestEncodingContext;
    let mut enc = TestEncodingContext::new(vec![], 1, -1);
    let mut s = ArrayVec::<u8, 4>::new();
    if s.try_extend_from_slice(&[0b10_01_00]).is_err() {
        crate::invariant_violation("EDIFACT test arabelleği kapasitesini aştı");
    }
    write4(&mut enc, &s);
    assert_eq!(enc.codewords, vec![0b10_01_00_00]);
}
