use std::io::{self, Read};

use datamatrix::{DataMatrix, placement::MatrixMap};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Önce stdin'den ASCII ile kodlanmış 0 ve 1'lerden oluşan bitmap okunur.
    // Örneğin aşağıdaki girdi bir Data Matrix kodlar:
    //
    //    1010101010
    //    1010101101
    //    1101010000
    //    1010110011
    //    1101011000
    //    1110011001
    //    1011001000
    //    1010010011
    //    1001001000
    //    1111111111
    let mut input = vec![];
    io::stdin().read_to_end(&mut input)?;
    let width = input
        .iter()
        .filter(|x| matches!(*x, b'0' | b'1' | b'\n'))
        .position(|x| *x == b'\n')
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "girdi ilk satırın genişliğini belirleyen bir satır sonu içermiyor",
            )
        })?;
    let pixels = input
        .into_iter()
        .filter_map(|b| match b {
            b'1' => Some(true),
            b'0' => Some(false),
            _ => None,
        })
        .collect::<Vec<_>>();

    let (matrix_map, size) = MatrixMap::try_from_bits(&pixels, width)?;
    let data = DataMatrix::decode(&pixels, width)?;
    println!("{}", matrix_map.bitmap()?.unicode());
    println!("Symbol size: {:?}", size);
    println!("İçerik: {:?}", std::str::from_utf8(&data)?);
    Ok(())
}
