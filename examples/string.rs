use datamatrix::{DataMatrix, SymbolList};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let text = "Doppelgänger";
    // Bu örnekte Latin-1 encoding kullanmak için `encode` yerine `encode_str` çağrılır.
    let encoded = DataMatrix::encode_str(text, SymbolList::default().enforce_square())?;
    print!("{}", encoded.bitmap()?.unicode());
    Ok(())
}
