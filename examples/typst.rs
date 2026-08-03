use std::fmt::Write;

use datamatrix::{
    DataMatrix, SymbolList,
    placement::{Bitmap, PathSegment},
};

fn bitmap_to_typst(bitmap: Bitmap<bool>) -> Result<String, Box<dyn std::error::Error>> {
    let mut img: String = String::new();
    // Sabit dış kutu ve beyaz zemin, path'in her yanında bir module'lük quiet
    // zone bırakır. Curve kutunun sol üstüne mutlak olarak yerleştirilir.
    writeln!(
        img,
        "#box(width: {}pt, height: {}pt, fill: white, inset: 0pt)[",
        bitmap.width() + 2,
        bitmap.height() + 2,
    )?;
    img.push_str("  #place(top + left)[\n");
    img.push_str("    #curve(\n");
    img.push_str("      fill: black,\n");
    img.push_str("      fill-rule: \"even-odd\",\n");
    img.push_str("      curve.move((1pt, 1pt)),\n");
    for part in bitmap.path() {
        match part {
            PathSegment::Horizontal(n) => {
                writeln!(img, "      curve.line(({n}pt, 0pt), relative: true),")
            }
            PathSegment::Vertical(n) => {
                writeln!(img, "      curve.line((0pt, {n}pt), relative: true),")
            }
            PathSegment::Move(dx, dy) => {
                writeln!(img, "      curve.move(({dx}pt, {dy}pt), relative: true),")
            }
            PathSegment::Close => writeln!(img, "      curve.close(),"),
        }?;
    }
    img.push_str("    )\n  ]\n]\n");
    Ok(img)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bitmap = DataMatrix::encode(
        b"Hello, typst!",
        SymbolList::default().enforce_rectangular(),
    )?
    .bitmap();
    println!("{}", bitmap_to_typst(bitmap)?);
    Ok(())
}
