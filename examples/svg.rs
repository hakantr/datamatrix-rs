use std::fmt::Write;

use datamatrix::{
    DataMatrix, SymbolList,
    placement::{Bitmap, PathSegment},
};

fn bitmap_to_svg(bitmap: Bitmap<bool>) -> Result<String, Box<dyn std::error::Error>> {
    // ViewBox her yanda standardın istediği bir module'lük quiet zone'u içerir;
    // beyaz zemin transparent çıktının çevre rengine bağımlı kalmasını önler.
    let width = bitmap.width() + 2;
    let height = bitmap.height() + 2;
    let mut svg = String::new();
    write!(
        svg,
        concat!(
            "<?xml version=\"1.0\"?>",
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" ",
            "width=\"{width}\" height=\"{height}\" shape-rendering=\"crispEdges\">",
            "<rect width=\"100%\" height=\"100%\" fill=\"white\"/>",
            "<path fill=\"black\" fill-rule=\"evenodd\" d=\"M1,1",
        ),
        width = width,
        height = height,
    )?;

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
