use datamatrix::{DataMatrix, SymbolList};
use std::io::{self, Read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = vec![];
    io::stdin().read_to_end(&mut buffer)?;

    let code = DataMatrix::encode(&buffer, SymbolList::default().enforce_square())?;
    print!("{}", code.bitmap().unicode());
    Ok(())
}
