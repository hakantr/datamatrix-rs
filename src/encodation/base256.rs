use super::{DataEncodingError, EncodingContext};

/// Standartta tanımlanan 255 state randomization işlemini uygular.
///
/// `pos`, yazılacak byte'ın codeword vektörünün tamamındaki 1 tabanlı konumu
/// olmalıdır.
fn randomize_255_state(ch: u8, pos: usize) -> u8 {
    let pseudo_random = ((149 * pos) % 255) + 1;
    let tmp = ch as u16 + pseudo_random as u16;
    if tmp <= 255 {
        tmp as u8
    } else {
        (tmp - 256) as u8
    }
}

/// Bu encodation'ın uzunluk "header" değerini yazar.
fn write_length<T: EncodingContext>(ctx: &mut T, start: usize) -> Result<(), DataEncodingError> {
    let space_left = ctx
        .symbol_size_left(0)
        .ok_or(DataEncodingError::TooMuchOrIllegalData)?;
    let Some(mut data_written) = ctx.codewords().len().checked_sub(start) else {
        crate::invariant_violation("Base256 başlangıç konumu codeword uzunluğunu aşıyor");
    };
    if ctx.has_more_characters() || space_left > 0 {
        let Some(data_count) = data_written.checked_sub(1) else {
            crate::invariant_violation("Base256 uzunluk codeword'ü bulunamadı");
        };
        if data_count <= 249 {
            ctx.replace(start, data_count as u8);
        } else if data_count <= 1555 {
            ctx.replace(start, ((data_count / 250) + 249) as u8);
            ctx.insert(start + 1, (data_count % 250) as u8);
            data_written += 1;
        } else {
            crate::invariant_violation("Base256 planı izin verilen 1555 byte sınırını aştı");
        }
    }
    for i in 0..data_written {
        let Some(index) = start.checked_add(i) else {
            crate::invariant_violation("Base256 codeword konumu hesaplanırken taşma oluştu");
        };
        let Some(ch) = ctx.codewords().get(index).copied() else {
            crate::invariant_violation("Base256 codeword konumu bulunamadı");
        };
        let Some(position) = index.checked_add(1) else {
            crate::invariant_violation("Base256 randomization konumu hesaplanırken taşma oluştu");
        };
        ctx.replace(index, randomize_255_state(ch, position));
    }
    Ok(())
}

pub(super) fn encode<T: EncodingContext>(ctx: &mut T) -> Result<(), DataEncodingError> {
    let start = ctx.codewords().len();
    ctx.push(0);

    loop {
        if let Some(ch) = ctx.eat() {
            ctx.push(ch);
        }
        if !ctx.has_more_characters() || ctx.maybe_switch_mode() {
            write_length(ctx, start)?;
            if !ctx.has_more_characters() {
                ctx.set_ascii_until_end();
            }
            return Ok(());
        }
    }
}
