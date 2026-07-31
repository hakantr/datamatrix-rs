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

/// Encode 1 to 4 characters using EDIFACT and write it to the context.
fn write4<T: EncodingContext>(ctx: &mut T, s: &ArrayVec<u8, 4>) -> Result<(), DataEncodingError> {
    let s1 = s.get(1).copied().unwrap_or(0) & 0b11_1111;
    let first = s.first().copied().ok_or(DataEncodingError::InternalError(
        "EDIFACT grubu boşken codeword yazılmak istendi",
    ))?;
    ctx.push((first << 2) | (s1 >> 4));

    if s.len() >= 2 {
        let s2 = s.get(2).copied().unwrap_or(0) & 0b11_1111;
        ctx.push((s1 << 4) | (s2 >> 2));

        if s.len() >= 3 {
            let s3 = s.get(3).copied().unwrap_or(0) & 0b11_1111;
            ctx.push((s2 << 6) | s3);
        }
    }
    Ok(())
}

fn handle_end<T: EncodingContext>(
    ctx: &mut T,
    mut symbols: ArrayVec<u8, 4>,
) -> Result<(), DataEncodingError> {
    // check case "encoding with <= 2 ASCII, no UNLATCH"
    let rest_chars = symbols.len() + ctx.characters_left();
    if rest_chars <= 4 {
        // The standard allows ASCII encoding without UNLATCH if there
        // are <= 2 words of space left in the symbol and
        // we can encode the rest with ASCII in this space.
        let rest: ArrayVec<u8, 4> = symbols
            .iter()
            .copied()
            .chain(ctx.rest().iter().copied())
            .collect();
        let ascii_size = ascii::encoding_size(&rest);
        if ascii_size <= 2 {
            match ctx.symbol_size_left(ascii_size).map(|x| x + ascii_size) {
                Some(space) if space <= 2 && ascii_size <= space => {
                    ctx.backup(symbols.len())?;
                    ctx.set_ascii_until_end();
                    return Ok(());
                }
                _ => (),
            }
        }
    }
    if symbols.is_empty() {
        if !ctx.has_more_characters() {
            // eod
            let space_left = ctx
                .symbol_size_left(0)
                .ok_or(DataEncodingError::TooMuchOrIllegalData)?;
            // padding case
            if space_left > 0 {
                // the other case is caught in the "special end of data rule" above
                if space_left <= 2 {
                    return Err(DataEncodingError::InternalError(
                        "EDIFACT end-of-data space hesabı geçersiz",
                    ));
                }
                ctx.push(UNLATCH << 2);
                ctx.set_ascii_until_end();
            }
        } else {
            // mode switch
            ctx.push(UNLATCH << 2);
        }
    } else {
        if symbols.len() > 3 {
            return Err(DataEncodingError::InternalError(
                "EDIFACT end-of-data arabelleğinde üçten fazla symbol kaldı",
            ));
        }
        if !ctx.has_more_characters() {
            // eod, maybe add UNLATCH for padding if space allows
            let space_left = ctx
                .symbol_size_left(symbols.len())
                .ok_or(DataEncodingError::TooMuchOrIllegalData)?
                > 0;
            if space_left || symbols.len() == 3 {
                symbols.try_push(UNLATCH).map_err(|_| {
                    DataEncodingError::InternalError("EDIFACT symbol arabelleği kapasitesini aştı")
                })?;
                ctx.set_ascii_until_end();
            }
        } else {
            symbols.try_push(UNLATCH).map_err(|_| {
                DataEncodingError::InternalError("EDIFACT symbol arabelleği kapasitesini aştı")
            })?;
        }
        write4(ctx, &symbols)?;
    }
    Ok(())
}

pub(super) fn encode<T: EncodingContext>(ctx: &mut T) -> Result<(), DataEncodingError> {
    let mut symbols = ArrayVec::<u8, 4>::new();
    while let Some(ch) = ctx.eat() {
        symbols.try_push(ch).map_err(|_| {
            DataEncodingError::InternalError("EDIFACT symbol arabelleği kapasitesini aştı")
        })?;

        if symbols.len() == 4 {
            write4(ctx, &symbols)?;
            symbols.clear();
            if ctx.maybe_switch_mode()? {
                break;
            }
        } else if ctx.maybe_switch_mode()? {
            break;
        }
    }
    handle_end(ctx, symbols)
}

#[test]
fn test_write4_four() -> Result<(), DataEncodingError> {
    use super::tests::TestEncodingContext;
    let mut enc = TestEncodingContext::new(vec![], 3, -1);
    write4(&mut enc, &[0b10_01_00, 0b11_01_10, 0b011010, 1].into())?;
    assert_eq!(
        enc.codewords,
        vec![0b10_01_00_11, 0b01_10_01_10, 0b10_00_00_01]
    );
    Ok(())
}

#[test]
fn test_write4_three() -> Result<(), DataEncodingError> {
    use super::tests::TestEncodingContext;
    let mut enc = TestEncodingContext::new(vec![], 3, -1);
    let mut s = ArrayVec::<u8, 4>::new();
    s.try_extend_from_slice(&[0b10_01_00, 0b11_01_10, 0b011010])
        .map_err(|_| DataEncodingError::InternalError("EDIFACT test buffer kapasitesi aşıldı"))?;
    write4(&mut enc, &s)?;
    assert_eq!(
        enc.codewords,
        vec![0b10_01_00_11, 0b01_10_01_10, 0b10_00_00_00]
    );
    Ok(())
}

#[test]
fn test_write4_two() -> Result<(), DataEncodingError> {
    use super::tests::TestEncodingContext;
    let mut enc = TestEncodingContext::new(vec![], 2, -1);
    let mut s = ArrayVec::<u8, 4>::new();
    s.try_extend_from_slice(&[0b10_01_00, 0b11_01_10])
        .map_err(|_| DataEncodingError::InternalError("EDIFACT test buffer kapasitesi aşıldı"))?;
    write4(&mut enc, &s)?;
    assert_eq!(enc.codewords, vec![0b10_01_00_11, 0b01_10_00_00]);
    Ok(())
}

#[test]
fn test_write4_one() -> Result<(), DataEncodingError> {
    use super::tests::TestEncodingContext;
    let mut enc = TestEncodingContext::new(vec![], 1, -1);
    let mut s = ArrayVec::<u8, 4>::new();
    s.try_extend_from_slice(&[0b10_01_00])
        .map_err(|_| DataEncodingError::InternalError("EDIFACT test buffer kapasitesi aşıldı"))?;
    write4(&mut enc, &s)?;
    assert_eq!(enc.codewords, vec![0b10_01_00_00]);
    Ok(())
}
