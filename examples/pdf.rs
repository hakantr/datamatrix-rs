use std::io::Write;

use datamatrix::{DataMatrix, SymbolList, placement::PathSegment};
use krilla::Document;
use krilla::color::rgb;
use krilla::geom::PathBuilder;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let s = concat!(
        "Shall I compare thee to a summer's day?\n",
        "Thou art more lovely and more temperate.\n",
        "Rough winds do shake the darling buds of May,\n",
        "And summer's lease hath all too short a date.\n",
        "Sometime too hot the eye of heaven shines,\n",
        "And often is his gold complexion dimmed;\n",
        "And every fair from fair sometime declines,\n",
        "By chance, or nature's changing course, untrimmed;\n",
        "But thy eternal summer shall not fade,\n",
        "Nor lose possession of that fair thou ow'st,\n",
        "Nor shall death brag thou wand'rest in his shade,\n",
        "When in eternal lines to Time thou grow'st.\n",
        "So long as men can breathe, or eyes can see,\n",
        "So long lives this, and this gives life to thee.",
    );
    let bitmap = DataMatrix::encode(s.as_bytes(), SymbolList::default())?.bitmap()?;

    // Tek bir siyah karenin PDF point (1/72 inç) cinsinden boyutu. Burada bir
    // module 1 mm genişliğindedir; değer bitmap boyutlarından ve kullanılabilir
    // alandan da türetilebilir.
    const SIZE: f32 = 72.0 / 25.4;

    // krilla, y ekseni aşağı bakan sol üst origin kullanır. Bu sistem
    // Bitmap::path() ile eşleştiğinden relative adımlar doğrudan uygulanabilir.
    // Quiet zone için sol üst köşeden bir module içeride başlanır.
    let mut x = SIZE;
    let mut y = SIZE;
    let mut start = (x, y);

    let mut pb = PathBuilder::new();
    // İlk subpath örtük başlar; path() başlangıçta Move üretmez.
    pb.move_to(x, y);
    for segment in bitmap.path()? {
        match segment {
            PathSegment::Move(dx, dy) => {
                x += SIZE * (dx as f32);
                y += SIZE * (dy as f32);
                start = (x, y);
                pb.move_to(x, y);
            }
            PathSegment::Horizontal(dx) => {
                x += SIZE * (dx as f32);
                pb.line_to(x, y);
            }
            PathSegment::Vertical(dy) => {
                y += SIZE * (dy as f32);
                pb.line_to(x, y);
            }
            PathSegment::Close => {
                pb.close();
                x = start.0;
                y = start.1;
            }
        };
    }
    let path = pb.finish().ok_or_else(|| {
        std::io::Error::other("Data Matrix için geçerli bir PDF path oluşturulamadı")
    })?;

    // Data Matrix'i ve çevresinde bir module genişliğinde quiet zone'u içeren
    // tek sayfalık bir PDF oluşturur.
    let mut document = Document::new();
    let mut page = document.start_page_with(
        PageSettings::from_wh(
            SIZE * (bitmap.width() + 2) as f32,
            SIZE * (bitmap.height() + 2) as f32,
        )
        .ok_or_else(|| std::io::Error::other("PDF sayfa boyutu geçersiz"))?,
    );
    let mut surface = page.surface();
    surface.set_fill(Some(Fill {
        paint: rgb::Color::black().into(),
        rule: FillRule::EvenOdd,
        ..Default::default()
    }));
    surface.draw_path(&path);
    surface.finish();
    page.finish();

    let pdf = document.finish()?;
    std::io::stdout().write_all(&pdf)?;
    Ok(())
}
