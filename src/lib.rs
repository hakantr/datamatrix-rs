//! Optimizing encoder içeren Data Matrix (ECC 200) decoding ve encoding kütüphanesi.
//!
//! # Kullanım örneği
//!
//! ```rust
//! # use datamatrix::{DataMatrix, SymbolList};
//! let code = DataMatrix::encode(
//!     b"Hello, World!",
//!     SymbolList::default(),
//! )?;
//! print!("{}", code.bitmap().unicode());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Bu basit örnek Unicode blok karakterleriyle bir Data Matrix yazdırır. Diğer
//! çıktı formatlarını üretme konusunda [Bitmap yapısı](Bitmap) için tanımlanan
//! yardımcı fonksiyonlara veya projenin `examples/` dizinine bakın.
//!
//! Başka symbol size değerleri belirtilebilir; ayrıntılar için [SymbolList]'e bakın.
//!
//! # Data Matrix için character encoding notları
//!
//! > **Kısa özet:** Birçok decoder doğru charset işleme desteğine sahip olmadığından
//! > data yazdırılabilir ASCII olmalıdır. Sonraki en iyi seçenek Latin-1'dir; aksi
//! > halde decoder'ların otomatik algılama yöntemlerine güvenirsiniz. Decoding
//! > sürecini denetliyorsanız veya bu risk sizin için önemli değilse bu uyarı geçerli değildir.
//!
//! Bu bölümün tamamı QR code'lar için de geçerlidir.
//!
//! Yazdırılamayan ASCII karakterleri içeren string'leri encode ederken dikkatli
//! olun. Örneğin UTF-8 encoding'i [ECI](https://en.wikipedia.org/wiki/Extended_Channel_Interpretation)
//! ile belirtmek mümkün olsa da dünyadaki birçok decoder'ın bunu uygulamadığı
//! düşünülmektedir. Bazı decoder'ların klavye kaynağı olarak kullanıldığını da
//! unutmayın. Örneğin el tipi scanner'lar sınırlı Unicode input özelliğine sahip,
//! platform/locale'e özgü klavye düzenleriyle kısıtlanabilir. Bu yüzden encoding
//! ve decoding sürecinin tamamını denetlemiyorsanız _yazdırılabilir_ ASCII
//! karakterleri içinde kalmanız önerilir.
//!
//! Data Matrix specification, standart charset olarak ISO 8859-1'i (Latin-1)
//! tanımlar. Testler bazı decoder'ların (akıllı telefon scanner uygulamaları) buna
//! uymadığını ve üst aralıkta karakterler varsa güvenli bir tercih olarak binary
//! çıktı döndürdüğünü gösteriyor. Ne yazık ki bazı decoder'lar charset'i tahmin
//! etmeye çalışır veya her zaman UTF-8 varsayar.
//!
//! 8-bit aralığın tamamı encode edilebilir ve decoder aynı input'u döndürür. Bu
//! nedenle yukarıdaki sorunlar data'nın _yorumlanmasıyla_ ve el tipi scanner'ların
//! olası input sınırlamalarıyla ilgilidir.
//!
//! # Decoding
//!
//! Bir Data Matrix algılandığını varsayarsak mesaj şu şekilde decode edilebilir:
//!
//! ```rust
//! # use datamatrix::{SymbolSize, placement::MatrixMap, DataMatrix};
//! # let codewords1 = [73, 239, 116, 130, 175, 52, 19, 40, 179, 242, 106, 105, 97, 98, 35, 165, 137, 102, 203, 106, 207, 48, 186, 66];
//! # let map = MatrixMap::new_with_codewords(&codewords1, SymbolSize::Square16)?;
//! # let bitmap = map.bitmap();
//! # let pixels: Vec<bool> = bitmap.bits().into();
//! // let pixels: Vec<bool> = …
//! let width = 16;
//! let data = DataMatrix::decode(&pixels, width)?;
//! assert_eq!(&data, b"Hello, World!");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Geçerli sınırlamalar
//!
//! Görsel algılama henüz uygulanmamıştır ancak decoding backend tamamlanmış ve API
//! üzerinden sunulmuştur. Eksik olan tek parça, görselden true ve false değerlerinden
//! oluşan matrix'i çıkaran detector'dır. Gelecekte genel amaçlı bir detector planlanmaktadır.
//!
//! Diğer sınırlamalar: Şu anda GS1/FNC1 character encoding için [sınırlı destek](DataMatrix::encode_gs1),
//! [sınırlı ECI encoding](DataMatrix::encode_str) vardır; structured append ve
//! reader programming yoktur. ISO/IEC 15424 içinde belirtilen decoding çıktı
//! formatı da (metadata, ECI vb.) uygulanmamıştır. Buna ihtiyacınız varsa issue açın.
//!
//! # Hata ve panik sözleşmesi
//!
//! Kamuya açık fonksiyonlar geçersiz dış girdi, desteklenmeyen seçenek veya normal
//! çalışma hatası nedeniyle kasıtlı olarak paniklemez; bunları yapılandırılmış
//! `Result<T, E>` ile bildirir. Sınır kurucuları girdiyi doğruladıktan sonra private
//! alanlarla korunan geçerli değerler üzerindeki mantıksal olarak hatasız işlemler
//! doğrudan sonuç döndürür.
//!
//! Panik yalnızca kütüphanenin bir iç değişmezinin bozulduğunu gösteren programlama
//! hatalarında kullanılır. Bellek tükenmesi, stack overflow ve bağımlılıkların
//! panikleri bu sözleşmenin dışındadır.

#![no_std]
extern crate alloc;
#[cfg(test)]
extern crate std;

mod decodation;
mod encodation;
pub mod errorcode;
pub mod placement;
mod symbol_size;

pub mod data;

/// Yerel GPUI snapshot'ı için Data Matrix rendering bileşenleri.
#[cfg(feature = "gpui")]
pub mod gpui;

pub use encodation::EncodationType;
pub use symbol_size::{SymbolList, SymbolSize};

/// Private alanlar ve doğrulanmış kurucularla korunan bir iç değişmezin
/// bozulduğunu bildirir.
///
/// Bu fonksiyon kullanıcı girdisini doğrulamak için kullanılmaz. Buraya ulaşmak
/// kütüphanede bir programlama hatası bulunduğu anlamına gelir.
#[cold]
#[track_caller]
#[expect(
    clippy::panic,
    reason = "doğrulanmış private durumun bozulması normal bir çalışma hatası değildir"
)]
pub(crate) fn invariant_violation(message: &'static str) -> ! {
    panic!("datamatrix iç değişmezi bozuldu: {message}")
}

#[cfg(test)]
use alloc::boxed::Box;
use alloc::vec::Vec;
use flagset::FlagSet;

use encodation::DataEncodingError;
use placement::{Bitmap, MatrixMap};

#[cfg(test)]
use pretty_assertions::assert_eq;

/// Encode edilmiş Data Matrix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataMatrix {
    size: SymbolSize,
    codewords: Vec<u8>,
    num_data_codewords: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Data Matrix decode edilirken oluşan hatalar.
pub enum DecodingError {
    /// Eşleşen boyutlarda symbol bulunamadığı veya alignment pattern geçersiz olduğu
    /// için piksellerin bir [symbol size](SymbolSize) ile eşlenemediğini belirtir.
    PixelConversion(placement::BitmapConversionError),
    /// Data Matrix encode edilirken [error correction](errorcode) işleminin doğru
    /// yapılmadığını veya çok fazla algılama hatası (yanlış siyah/beyaz kare) olduğunu belirtir.
    ErrorCorrection(errorcode::ErrorDecodingError),
    DataDecoding(decodation::DataDecodingError),
}

impl core::fmt::Display for DecodingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::PixelConversion(error) => write!(f, "bitmap çözümlenemedi: {error}"),
            Self::ErrorCorrection(error) => write!(f, "error correction başarısız: {error}"),
            Self::DataDecoding(error) => write!(f, "Data Matrix verisi çözümlenemedi: {error}"),
        }
    }
}

impl core::error::Error for DecodingError {}

impl DataMatrix {
    /// Encode edilmiş Data Matrix'in symbol size değerini döndürür.
    pub fn size(&self) -> SymbolSize {
        self.size
    }

    /// Data Matrix'i piksel gösteriminden decode eder.
    ///
    /// Alignment pattern dahil edilmelidir. `width` argümanı tek satırdaki piksel sayısını belirtir.
    ///
    /// Piksellerin row-major sırada verilmesi beklenir; önce en üst satır, ardından
    /// ikinci satır ve diğerleri gelir.
    ///
    /// Data Matrix, GS1 Data Matrix olduğunu belirten `FNC1` codeword ile başlayabilir.
    /// ISO standardı bu durumda scanner'ın symbology identifier `]d2` değerini
    /// başa eklemesini ister. Bu davranış burada uygulanmamıştır; decoder başlangıçtaki
    /// `FNC1` codeword'ü şimdilik yalnızca yok sayar. Talep olursa daha ayrıntılı
    /// decoder çıktısı uygulanabilir.
    pub fn decode(pixels: &[bool], width: usize) -> Result<Vec<u8>, DecodingError> {
        let (matrix_map, size) =
            MatrixMap::try_from_bits(pixels, width).map_err(DecodingError::PixelConversion)?;
        let mut codewords = matrix_map.codewords();
        errorcode::decode_error(&mut codewords, size).map_err(DecodingError::ErrorCorrection)?;
        let Some(data_codewords) = codewords.get(..size.num_data_codewords()) else {
            invariant_violation(
                "decode edilen data codeword aralığı toplam codeword sayısını aştı",
            );
        };
        decodation::decode_data(data_codewords).map_err(DecodingError::DataDecoding)
    }

    /// Data'yı encoded biçimde döndürür.
    ///
    /// Error correction dahildir. Yalnızca data gerekiyorsa
    /// [data_codewords()](Self::data_codewords) yöntemine bakın.
    pub fn codewords(&self) -> &[u8] {
        &self.codewords
    }

    /// Data'yı encode eden codeword'leri döndürür.
    ///
    /// Bu değer [codewords()](Self::codewords) tarafından döndürülen codeword'lerin prefix'idir.
    pub fn data_codewords(&self) -> &[u8] {
        let Some(codewords) = self.codewords.get(..self.num_data_codewords) else {
            invariant_violation("data codeword sayısı toplam codeword sayısını aştı");
        };
        codewords
    }

    /// Data Matrix'i temsil eden soyut bitmap oluşturur.
    pub fn bitmap(&self) -> Bitmap<bool> {
        let Ok(map) = MatrixMap::new_with_codewords(&self.codewords, self.size) else {
            invariant_violation(
                "encode edilmiş Data Matrix codeword sayısı symbol size ile uyuşmuyor",
            );
        };
        map.bitmap()
    }

    /// Data'yı Data Matrix (ECC200) olarak encode eder.
    ///
    /// [DataMatrixBuilder::encode] için wrapper'dır.
    pub fn encode<I: Into<SymbolList>>(
        data: &[u8],
        symbol_list: I,
    ) -> Result<DataMatrix, DataEncodingError> {
        DataMatrixBuilder::new()
            .with_symbol_list(symbol_list)
            .encode(data)
    }

    /// String'i Data Matrix (ECC200) olarak encode eder.
    ///
    /// [DataMatrixBuilder::encode_str] için wrapper'dır.
    pub fn encode_str<I: Into<SymbolList>>(
        text: &str,
        symbol_list: I,
    ) -> Result<DataMatrix, DataEncodingError> {
        DataMatrixBuilder::new()
            .with_symbol_list(symbol_list)
            .encode_str(text)
    }

    /// Data'yı GS1 Data Matrix olarak encode eder.
    ///
    /// [encode()](Self::encode) yönteminden tek farkı, ilk konuma `FNC1` codeword eklenmesidir.
    ///
    /// `FNC1` değerini sonraki konumlarda encoding henüz uygulanmamıştır.
    ///
    /// ```rust
    /// # use datamatrix::{DataMatrix, SymbolList, data::DataEncodingError};
    /// // Element string'lerini birleştirmek için "\x1D" (ASCII GS control sequence) kullanır.
    /// let data = b"01034531200000111719112510ABCD1234\x1D2110";
    /// let data_matrix = DataMatrix::encode_gs1(data, SymbolList::default())?;
    /// let bitmap = data_matrix.bitmap();
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn encode_gs1<I: Into<SymbolList>>(
        data: &[u8],
        symbol_list: I,
    ) -> Result<DataMatrix, DataEncodingError> {
        DataMatrixBuilder::new()
            .with_symbol_list(symbol_list)
            .with_fnc1_start(true)
            .encode(data)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Data Matrix encoding üzerinde daha fazla denetim sağlayan builder.
pub struct DataMatrixBuilder {
    encodation_types: FlagSet<EncodationType>,
    symbol_list: SymbolList,
    use_macros: bool,
    fnc1_start: bool,
}

impl DataMatrixBuilder {
    pub fn new() -> Self {
        Self {
            encodation_types: EncodationType::all(),
            symbol_list: SymbolList::default(),
            use_macros: true,
            fnc1_start: false,
        }
    }

    /// Kullanılabilecek encodation türlerini belirtir.
    ///
    /// Varsayılan olarak bütün encodation türleri etkindir.
    ///
    /// # Örnek
    ///
    /// ```rust
    /// # use datamatrix::{DataMatrixBuilder, data::EncodationType};
    /// let datamatrix = DataMatrixBuilder::new()
    ///     .with_encodation_types(EncodationType::Base256 | EncodationType::Edifact)
    ///     .encode(b"\xFAaaa")?;
    /// Ok::<(), datamatrix::data::DataEncodingError>(())
    /// ```
    pub fn with_encodation_types(self, types: impl Into<FlagSet<EncodationType>>) -> Self {
        Self {
            encodation_types: types.into(),
            ..self
        }
    }

    /// Data Matrix'in GS1 Data Matrix olduğunu belirten `FNC1` codeword ile başlayıp
    /// başlamayacağını belirtir.
    pub fn with_fnc1_start(self, fnc1_start: bool) -> Self {
        Self { fnc1_start, ..self }
    }

    /// Macro kullanılıp kullanılmayacağını belirtir.
    ///
    /// Varsayılan olarak etkindir.
    pub fn with_macros(self, use_macros: bool) -> Self {
        Self { use_macros, ..self }
    }

    /// İzin verilen symbol size listesini belirtir.
    ///
    /// Varsayılan olarak [SymbolList::default()] kullanır.
    pub fn with_symbol_list<I: Into<SymbolList>>(self, symbol_list: I) -> Self {
        Self {
            symbol_list: symbol_list.into(),
            ..self
        }
    }

    /// Data'yı Data Matrix (ECC200) olarak encode eder.
    ///
    /// Charset notları için [modül dokümantasyonunu](crate) okuyun. Input Latin-1
    /// charset ile gösterilebiliyorsa [data modülündeki](crate::data) dönüşüm
    /// fonksiyonu kullanılabilir. Yalnızca yazdırılabilir ASCII kullanılıyorsa data
    /// doğrudan geçirilebilir.
    ///
    /// Data verilen size içine sığmazsa encoding başarısız olur. Encoder data'nın
    /// sığdığı en küçük size değerini otomatik seçebilir ([SymbolList]'e bakın) ancak
    /// bir üst sınır vardır.
    pub fn encode(self, data: &[u8]) -> Result<DataMatrix, DataEncodingError> {
        self.encode_eci(data, None)
    }

    /// String'i Data Matrix (ECC200) olarak encode eder.
    ///
    /// String Latin-1'e dönüştürülebiliyorsa ECI kullanılmaz; aksi halde başlangıca
    /// UTF-8 ECI eklenir. Decoder'ınızın bunu desteklediğini doğrulayın. Ayrıntılar
    /// için [modül dokümantasyonundaki](crate) notlara bakın.
    pub fn encode_str(self, text: &str) -> Result<DataMatrix, DataEncodingError> {
        if let Some(data) = data::utf8_to_latin1(text) {
            // String Latin-1'dir.
            self.encode_eci(&data, None)
        } else {
            // UTF-8 ECI ile encode eder.
            self.encode_eci(text.as_bytes(), Some(decodation::ECI_UTF8))
        }
    }

    #[doc(hidden)]
    pub fn encode_eci(
        self,
        data: &[u8],
        eci: Option<u32>,
    ) -> Result<DataMatrix, DataEncodingError> {
        let (mut codewords, size) = data::encode_data_internal(
            data,
            &self.symbol_list,
            eci,
            self.encodation_types,
            self.use_macros,
            self.fnc1_start,
        )?;
        let Ok(ecc) = errorcode::encode_error(&codewords, size) else {
            invariant_violation(
                "encoder çıktısının data codeword sayısı symbol size ile uyuşmuyor",
            );
        };
        let num_data_codewords = codewords.len();
        codewords.extend_from_slice(&ecc);
        Ok(DataMatrix {
            size,
            codewords,
            num_data_codewords,
        })
    }
}

impl Default for DataMatrixBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[test]
fn utf8_eci_test() -> Result<(), Box<dyn std::error::Error>> {
    let data = "🥸";
    let code = DataMatrix::encode_str(data, SymbolList::default())?;
    let decoded = data::decode_str(code.data_codewords())?;
    assert_eq!(decoded, data);
    Ok(())
}

#[test]
fn test_tile_placement_forth_and_back() -> Result<(), Box<dyn std::error::Error>> {
    let mut rnd_data = test::random_data();
    for size in SymbolList::all() {
        let data = rnd_data(size.num_codewords());
        let map = MatrixMap::new_with_codewords(&data, size)?;
        assert_eq!(map.codewords(), data);
        let bitmap = map.bitmap();
        let (matrix_map, _size) = MatrixMap::try_from_bits(bitmap.bits(), bitmap.width())?;
        assert_eq!(matrix_map.codewords(), data);
    }
    Ok(())
}

#[test]
fn test_macro_str() -> Result<(), Box<dyn std::error::Error>> {
    let data = "[)>\x1E05\x1D🤘\x1E\x04";
    let map = DataMatrix::encode_str(data, SymbolList::default())?;
    let codewords = map.data_codewords();
    assert_eq!(
        codewords,
        &[
            encodation::MACRO05,
            encodation::ascii::ECI,
            decodation::ECI_UTF8 as u8 + 1,
            // Dört byte UTF-8 karakterin Base256 encoding'i ve padding
            231,
            240,
            114,
            183,
            81,
            219,
            129,
        ]
    );
    let out = data::decode_str(codewords)?;
    assert_eq!(data, out);
    Ok(())
}

#[test]
fn test_too_much_data() {
    let mut rnd_data = test::random_data();
    let data = rnd_data(5000);
    let result = DataMatrix::encode(&data, SymbolList::default());
    assert_eq!(result, Err(DataEncodingError::TooMuchOrIllegalData));
}

#[cfg(test)]
mod test {
    use crate::placement::MatrixMap;
    use crate::symbol_size::SymbolSize;
    use alloc::vec::Vec;

    /// Test data üretimi için basit LCG random generator.
    pub fn random_maps() -> impl FnMut(SymbolSize) -> MatrixMap<bool> {
        let mut rnd = random_bytes();
        move |size| {
            let mut map = MatrixMap::new(size);
            map.traverse_mut(|_, bits| {
                for bit in bits {
                    *bit = rnd() > 127;
                }
            });
            map.write_padding();
            map
        }
    }

    pub fn random_bytes() -> impl FnMut() -> u8 {
        let mut seed = 0;
        move || {
            let modulus = 2u64.pow(31);
            let a = 1103515245u64;
            let c = 12345u64;
            seed = (a * seed + c) % modulus;
            (seed % 256) as u8
        }
    }

    pub fn random_data() -> impl FnMut(usize) -> Vec<u8> {
        let mut rnd = random_bytes();
        move |len| {
            let mut v = Vec::with_capacity(len);
            for _ in 0..len {
                v.push(rnd());
            }
            v
        }
    }
}

#[test]
fn test_simple_gs1() -> Result<(), Box<dyn std::error::Error>> {
    let data = b"01034531200000111719112510ABCD1234\x1D2110";
    let result = DataMatrix::encode_gs1(data, SymbolList::default())?;
    let codewords = result.codewords();
    assert_eq!(codewords.first(), Some(&crate::encodation::ascii::FNC1));

    let decoded = data::decode_data(result.data_codewords())?;
    assert_eq!(decoded, data);
    Ok(())
}
