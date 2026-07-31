use std::fmt::Write;

use datamatrix::{
    DataMatrix, SymbolList,
    placement::{Bitmap, PathSegment},
};

fn bitmap_to_svg(bitmap: Bitmap<bool>) -> Result<String, Box<dyn std::error::Error>> {
    // SVG başlığı; path (1, 1) koordinatında başlar.
    let mut svg: String = concat!(
        "<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\">",
        "<path fill-rule=\"evenodd\" d=\"M1,1",
    )
    .to_owned();

    // Path segment öğeleri SVG path söz dizimine doğrudan karşılık gelir.
    // Boyutu değiştirmek için bütün değerler sabit bir ölçekle çarpılabilir.
    for part in bitmap.path() {
        match part {
            PathSegment::Horizontal(n) => write!(svg, "h{}", n),
            PathSegment::Vertical(n) => write!(svg, "v{}", n),
            PathSegment::Move(dx, dy) => write!(svg, "m{},{}", dx, dy),
            PathSegment::Close => write!(svg, "z"),
        }?;
    }
    svg.push_str("\"/></svg>");
    Ok(svg)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bitmap =
        DataMatrix::encode(b"Hello, SVG!", SymbolList::default().enforce_rectangular())?.bitmap();
    println!("{}", bitmap_to_svg(bitmap)?);
    Ok(())
}
