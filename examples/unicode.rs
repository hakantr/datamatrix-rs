//! Unicode "octant" karakterlerini kullanarak Data Matrix'i terminalde gösterir;
//! her karakter hücresine 2×4 piksellik bir blok yerleştirir.
//!
//! `ascii.rs` gibi mesajı stdin üzerinden okur:
//!
//! ```sh
//! echo -n "Hello" | cargo run --example unicode
//! ```

use datamatrix::{DataMatrix, SymbolList};
use std::io::{self, Read};

/// 2×4 piksel ızgarasını gösteren Unicode "octant" karakterini döndürür.
///
/// `bits`, bit başına bir piksel tutar; bit `i`, okuma sırasındaki (yukarıdan
/// aşağıya ve soldan sağa) `i` pikselidir:
///
/// ```text
///   bit0 bit1
///   bit2 bit3
///   bit4 bit5
///   bit6 bit7
/// ```
fn octant_char(bits: u8) -> Result<char, io::Error> {
    if bits & 0x33 == bits >> 2 & 0x33 {
        // Quadrant durumları sıkıştırılmış 4-bit code ile indekslenir.
        let chars = [
            ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
        ];
        let index = usize::from(bits & 3 | bits >> 2 & 12);
        return chars
            .get(index)
            .copied()
            .ok_or_else(|| io::Error::other("Unicode quadrant indeksi geçersiz"));
    }
    match [1, 2, 3, 20, 40, 63, 64, 128, 192, 252].binary_search(&bits) {
        // Düzensiz 10 adet çeyrek/üç çeyrek blok.
        Ok(index) => ['𜺨', '𜺫', '🮂', '🯦', '🯧', '🮅', '𜺣', '𜺠', '▂', '▆']
            .get(index)
            .copied()
            .ok_or_else(|| io::Error::other("Unicode octant indeksi geçersiz")),
        Err(below) => {
            let skip = below
                + (0..16u16)
                    .filter(|c| 80 * (c >> 2) + 5 * (c & 3) < bits as u16)
                    .count();
            let codepoint = 0x1CD00u32
                .checked_add(u32::from(bits))
                .and_then(|value| value.checked_sub(u32::try_from(skip).ok()?))
                .ok_or_else(|| io::Error::other("Unicode octant codepoint hesabı taştı"))?;
            char::from_u32(codepoint)
                .ok_or_else(|| io::Error::other("hesaplanan Unicode octant codepoint geçersiz"))
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = vec![];
    io::stdin().read_to_end(&mut buffer)?;

    let code = DataMatrix::encode(&buffer, SymbolList::default().enforce_square())?;
    let bitmap = code.bitmap();

    // Her yanda bir piksellik quiet zone bulunan padded boyut. Bir octant
    // karakteri 2×4 piksel kapladığından bölmeler yukarı yuvarlanır.
    let (w, h) = (bitmap.width() + 2, bitmap.height() + 2);

    // `pixels()` siyah pikselleri x önce gelecek biçimde sıralar. Böylece veriler
    // bant bant gezilir ve bellekte yalnızca bir octant hücre satırı tutulur.
    let mut pixels = bitmap.pixels().peekable();
    for band in 0..h.div_ceil(4) {
        let mut line = vec![0u8; w.div_ceil(2)];
        while let Some(&(x, y)) = pixels.peek().filter(|&&(_, y)| y + 1 < 4 * (band + 1)) {
            // Quiet zone kadar kaydırıp hücrede piksele karşılık gelen biti ayarlar.
            let (px, py) = (x + 1, y + 1);
            let cell = line
                .get_mut(px / 2)
                .ok_or_else(|| io::Error::other("octant satırı indeksi sınırların dışında"))?;
            *cell |= 1 << (2 * (py % 4) + px % 2);
            pixels.next();
        }
        let row: String = line
            .iter()
            .map(|&bits| octant_char(bits))
            .collect::<Result<_, _>>()?;
        println!("{row}");
    }
    Ok(())
}
