use datamatrix::{DataMatrix, SymbolList};
use image::{GrayImage, Luma};
use std::io;

/// Yalnızca bir Data Matrix içeren görsel üretir.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Görseldeki tek bir siyah karenin piksel cinsinden genişliği ve yüksekliği.
    // Alan kısıtları tam sayı olmayan bir boyuta yol açıyorsa daha küçük bir
    // görsel üretip ardından interpolation (rescale) uygulanabilir.
    const N: usize = 5;

    // "Hello, World!" verisini sığabildiği en küçük kareye encode eder.
    let bitmap =
        DataMatrix::encode(b"Hello, World!", SymbolList::default().enforce_square())?.bitmap()?;

    // Data Matrix ve quiet zone içeren bir görsel oluşturur.
    let width = bitmap
        .width()
        .checked_add(2)
        .and_then(|value| value.checked_mul(N))
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| io::Error::other("görsel genişliği u32 sınırını aşıyor"))?;
    let height = bitmap
        .height()
        .checked_add(2)
        .and_then(|value| value.checked_mul(N))
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| io::Error::other("görsel yüksekliği u32 sınırını aşıyor"))?;
    let mut image = GrayImage::from_pixel(width, height, Luma([255]));
    for (x, y) in bitmap.pixels() {
        // (x, y) konumundaki siyah kareyi N×N siyah pikselle yazar.
        for i in 0..N {
            for j in 0..N {
                let x_i = x
                    .checked_add(1)
                    .and_then(|value| value.checked_mul(N))
                    .and_then(|value| value.checked_add(j))
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| io::Error::other("piksel x koordinatında taşma oluştu"))?;
                let y_j = y
                    .checked_add(1)
                    .and_then(|value| value.checked_mul(N))
                    .and_then(|value| value.checked_add(i))
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| io::Error::other("piksel y koordinatında taşma oluştu"))?;
                let pixel = image.get_pixel_mut_checked(x_i, y_j).ok_or_else(|| {
                    io::Error::other("hesaplanan piksel koordinatı görsel sınırlarının dışında")
                })?;
                *pixel = Luma([0]);
            }
        }
    }

    image.save("data_matrix.png")?;
    Ok(())
}
