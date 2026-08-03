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
//! Görsel algılama henüz uygulanmamıştır. Decoding backend, finder/alignment
//! pattern'leri dahil ve quiet zone hariç, satır ve sütun sınırları belirlenmiş
//! düzenli bir symbol module matrix'i bekler. Çeyrek dönüşler ve reflectance
//! reversal bu sınırda otomatik çözülür; fotoğraftan örnekleme ve perspektif/grid
//! bulma ise detector katmanının işidir.
//! Gelecekte genel amaçlı bir detector planlanmaktadır.
//!
//! ISO/IEC 16022:2024 içinde opsiyonel olan Structured Append uygulanmamıştır;
//! decoder bu codeword'ü standardın istediği gibi veri iletmeden, yapılandırılmış
//! hata ile reddeder. Reader Programming encode edilir ve [DecodedMessage]
//! içinde host verisinden ayrılmış metadata olarak raporlanır.
//! FNC1 ilk veya ikinci konumda encode edilebilir ([DataMatrixBuilder::with_fnc1]),
//! sonraki konum FNC1 ile birden çok/mesaj-içi ECI [DataMatrixSegment] üzerinden
//! üretilebilir. Decode tarafında ISO/IEC 15424 symbology identifier ile Clause 12
//! iletim formatı [DecodedMessage] üzerinden üretilebilir.
//! Eksik bir şeye ihtiyacınız varsa issue açın.
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

pub use data::DataMatrixSegment;
pub use decodation::DecodedMessage;
pub use encodation::{EncodationType, Fnc1Position};
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

fn finish_encoding(mut codewords: Vec<u8>, size: SymbolSize) -> DataMatrix {
    let Ok(ecc) = errorcode::encode_error(&codewords, size) else {
        invariant_violation("encoder çıktısının data codeword sayısı symbol size ile uyuşmuyor");
    };
    let num_data_codewords = codewords.len();
    codewords.extend_from_slice(&ecc);
    DataMatrix {
        size,
        codewords,
        num_data_codewords,
    }
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
    /// Finder/alignment pattern'leri dahil edilmeli, quiet zone çıkarılmış olmalıdır.
    /// `width` argümanı tek satırdaki module sayısını belirtir. Dört çeyrek dönüş
    /// ile koyu-açık reflectance reversal otomatik olarak denenir.
    ///
    /// Piksellerin row-major sırada verilmesi beklenir; önce en üst satır, ardından
    /// ikinci satır ve diğerleri gelir.
    ///
    /// Ham veri byte'larını döndürür. İlk veya ikinci konumdaki format bayrağı
    /// `FNC1` veri olarak iletilmez; symbology identifier (`]d2` vb.) ve ECI
    /// bilgisiyle birlikte ISO/IEC 16022:2024 Clause 12 iletim çıktısı için
    /// [decode_message()](Self::decode_message) yöntemine bakın. ECI içeren
    /// veya Reader Programming işaretli symbol'lerde bu fonksiyon hata döndürür.
    pub fn decode(pixels: &[bool], width: usize) -> Result<Vec<u8>, DecodingError> {
        let data_codewords = Self::data_codewords_from_pixels(pixels, width)?;
        decodation::decode_data(&data_codewords).map_err(DecodingError::DataDecoding)
    }

    /// Data Matrix'i piksel gösteriminden iletim metadata'sıyla birlikte decode eder.
    ///
    /// [decode()](Self::decode) yönteminden farklı olarak ECI içeren symbol'leri
    /// reddetmez; FNC1 konumunu, ECI bölümlerini, Reader Programming işaretini,
    /// Annex H symbology identifier değerini ve Clause 12 iletim byte'larını
    /// [DecodedMessage] üzerinden sunar.
    pub fn decode_message(pixels: &[bool], width: usize) -> Result<DecodedMessage, DecodingError> {
        let data_codewords = Self::data_codewords_from_pixels(pixels, width)?;
        decodation::decode_message(&data_codewords).map_err(DecodingError::DataDecoding)
    }

    /// Piksellerden error correction uygulanmış data codeword'lerini çıkarır.
    fn data_codewords_from_pixels(pixels: &[bool], width: usize) -> Result<Vec<u8>, DecodingError> {
        if width == 0 {
            return Err(DecodingError::PixelConversion(
                placement::BitmapConversionError::ZeroWidth,
            ));
        }
        if !pixels.len().is_multiple_of(width) {
            return Err(DecodingError::PixelConversion(
                placement::BitmapConversionError::DataSize,
            ));
        }

        let mut first_conversion_error = None;
        let mut first_ecc_error = None;
        // 6.1: yön bağımsızlığı ve 6.2 a: reflectance reversal. Görsel detector
        // module grid'ini çıkardıktan sonra kalan ayrık dönüşümler burada denenir.
        for inverted in [false, true] {
            for quarter_turns in 0..4 {
                let Some((candidate, candidate_width)) =
                    transform_modules(pixels, width, quarter_turns, inverted)
                else {
                    return Err(DecodingError::PixelConversion(
                        placement::BitmapConversionError::ArithmeticOverflow,
                    ));
                };
                let (matrix_map, size) = match MatrixMap::try_from_bits(&candidate, candidate_width)
                {
                    Ok(value) => value,
                    Err(error) => {
                        if first_conversion_error.is_none() {
                            first_conversion_error = Some(error);
                        }
                        continue;
                    }
                };
                let mut codewords = matrix_map.codewords();
                if let Err(error) = errorcode::decode_error(&mut codewords, size) {
                    if first_ecc_error.is_none() {
                        first_ecc_error = Some(error);
                    }
                    continue;
                }
                let Some(data_codewords) = codewords.get(..size.num_data_codewords()) else {
                    invariant_violation(
                        "decode edilen data codeword aralığı toplam codeword sayısını aştı",
                    );
                };
                return Ok(data_codewords.to_vec());
            }
        }

        if let Some(error) = first_ecc_error {
            Err(DecodingError::ErrorCorrection(error))
        } else {
            Err(DecodingError::PixelConversion(
                first_conversion_error.unwrap_or(placement::BitmapConversionError::SymbolSize),
            ))
        }
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
    /// [encode()](Self::encode) yönteminden tek farkı, ilk konuma `FNC1` codeword
    /// eklenmesidir (ISO/IEC 16022:2024, 12.2). İkinci konumdaki FNC1 (endüstri
    /// formatı, 12.3) için [DataMatrixBuilder::with_fnc1] kullanılabilir.
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
            .with_fnc1(Some(Fnc1Position::First))
            .encode(data)
    }

    /// ECI, sonraki-konum FNC1 veya Reader Programming içeren bölümlü bir
    /// mesajı encode eder.
    ///
    /// ```rust
    /// # use datamatrix::{DataMatrix, DataMatrixSegment, SymbolList};
    /// let code = DataMatrix::encode_segments(
    ///     &[
    ///         DataMatrixSegment::Data(b"A"),
    ///         DataMatrixSegment::Eci(7),
    ///         DataMatrixSegment::Data(&[182]),
    ///         DataMatrixSegment::Fnc1,
    ///         DataMatrixSegment::Data(b"B"),
    ///     ],
    ///     SymbolList::default(),
    /// )?;
    /// let message = datamatrix::data::decode_message(code.data_codewords())?;
    /// assert_eq!(message.data(), &[b'A', 182, 29, b'B']);
    /// assert_eq!(message.eci_spans(), &[(1, 7)]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn encode_segments<I: Into<SymbolList>>(
        segments: &[DataMatrixSegment<'_>],
        symbol_list: I,
    ) -> Result<DataMatrix, DataEncodingError> {
        DataMatrixBuilder::new()
            .with_symbol_list(symbol_list)
            .encode_segments(segments)
    }
}

/// Row-major module matrix'ine saat yönünde çeyrek dönüş ve isteğe bağlı
/// reflectance reversal uygular.
fn transform_modules(
    pixels: &[bool],
    width: usize,
    quarter_turns: usize,
    inverted: bool,
) -> Option<(Vec<bool>, usize)> {
    if width == 0 || !pixels.len().is_multiple_of(width) {
        return None;
    }
    let height = pixels.len() / width;
    let turns = quarter_turns % 4;
    let output_width = if turns.is_multiple_of(2) {
        width
    } else {
        height
    };
    let mut output = alloc::vec![false; pixels.len()];
    for y in 0..height {
        for x in 0..width {
            let input_index = y.checked_mul(width)?.checked_add(x)?;
            let (out_x, out_y) = match turns {
                0 => (x, y),
                1 => (height.checked_sub(y + 1)?, x),
                2 => (width.checked_sub(x + 1)?, height.checked_sub(y + 1)?),
                3 => (y, width.checked_sub(x + 1)?),
                _ => return None,
            };
            let output_index = out_y.checked_mul(output_width)?.checked_add(out_x)?;
            *output.get_mut(output_index)? = pixels.get(input_index).copied()? ^ inverted;
        }
    }
    Some((output, output_width))
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Data Matrix encoding üzerinde daha fazla denetim sağlayan builder.
pub struct DataMatrixBuilder {
    encodation_types: FlagSet<EncodationType>,
    symbol_list: SymbolList,
    use_macros: bool,
    fnc1: Option<Fnc1Position>,
    reader_programming: bool,
}

impl DataMatrixBuilder {
    pub fn new() -> Self {
        Self {
            encodation_types: EncodationType::all(),
            symbol_list: SymbolList::default(),
            use_macros: true,
            fnc1: None,
            reader_programming: false,
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
    ///
    /// [with_fnc1](Self::with_fnc1) yönteminin ilk konumla sınırlı kısaltmasıdır.
    pub fn with_fnc1_start(self, fnc1_start: bool) -> Self {
        Self {
            fnc1: fnc1_start.then_some(Fnc1Position::First),
            ..self
        }
    }

    /// `FNC1` codeword'ünün konumunu belirtir (ISO/IEC 16022:2024, 7.2.4.7).
    ///
    /// [First](Fnc1Position::First), verinin GS1 formatına uyduğunu (12.2);
    /// [Second](Fnc1Position::Second), AIM tarafından yetkilendirilen bir endüstri
    /// formatına uyduğunu (12.3) bildirir. İkinci konum için verinin ilk karakteri
    /// tek codeword'lük ASCII (tek karakter veya rakam çifti) olmalıdır; Reader
    /// Programming etkinse ilk codeword zaten 234 olduğundan FNC1 doğrudan ikinci
    /// konuma gelir. Aksi halde encoding hata döndürür. FNC1 belirtildiğinde
    /// macro'lar kullanılmaz.
    pub fn with_fnc1(self, fnc1: Option<Fnc1Position>) -> Self {
        Self { fnc1, ..self }
    }

    /// Symbol'ü bir Reader Programming mesajı olarak işaretler (7.2.4.10).
    ///
    /// Codeword 234 ilk konuma yazılır ve macro otomatik olarak devre dışı kalır.
    /// İlk-konum FNC1 aynı konumu istediği için birlikte kullanılırsa encoding
    /// hata döndürür; ikinci-konum FNC1 doğrudan ikinci codeword'e yazılabilir.
    pub fn with_reader_programming(self, enabled: bool) -> Self {
        Self {
            reader_programming: enabled,
            ..self
        }
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

    /// Denetim codeword'leri içeren bölümlü mesajı encode eder.
    ///
    /// Birden çok veya mesaj-içi ECI, sonraki konumlarda alan ayırıcı FNC1 ve
    /// Reader Programming için [`DataMatrixSegment`] kullanılır. Segment verisi
    /// ASCII encodation scheme ile yazılır; `with_encodation_types` ve macro
    /// seçimi bu özel yolda kullanılmaz.
    pub fn encode_segments(
        self,
        segments: &[DataMatrixSegment<'_>],
    ) -> Result<DataMatrix, DataEncodingError> {
        let (codewords, size) = data::encode_data_segments_internal(
            segments,
            &self.symbol_list,
            self.fnc1,
            self.reader_programming,
        )?;
        Ok(finish_encoding(codewords, size))
    }

    #[doc(hidden)]
    pub fn encode_eci(
        self,
        data: &[u8],
        eci: Option<u32>,
    ) -> Result<DataMatrix, DataEncodingError> {
        let (codewords, size) = data::encode_data_internal(
            data,
            &self.symbol_list,
            eci,
            self.encodation_types,
            self.use_macros,
            self.fnc1,
            self.reader_programming,
        )?;
        Ok(finish_encoding(codewords, size))
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
fn segmented_controls_round_trip_with_multiple_eci_and_later_fnc1()
-> Result<(), Box<dyn std::error::Error>> {
    let code = DataMatrix::encode_segments(
        &[
            DataMatrixSegment::Data(&[182]),
            DataMatrixSegment::Eci(7),
            DataMatrixSegment::Data(&[182]),
            DataMatrixSegment::Eci(3),
            DataMatrixSegment::Data(b"A"),
            DataMatrixSegment::Fnc1,
            DataMatrixSegment::Data(b"B"),
        ],
        SymbolList::default(),
    )?;
    assert_eq!(
        code.data_codewords().get(..11),
        Some([235, 55, 241, 8, 235, 55, 241, 4, 66, 232, 67].as_slice())
    );

    let message = data::decode_message(code.data_codewords())?;
    assert_eq!(message.data(), &[182, 182, b'A', 29, b'B']);
    assert_eq!(message.eci_spans(), &[(1, 7), (2, 3)]);
    assert_eq!(message.fnc1(), None);
    assert_eq!(message.symbology_identifier(), "]d4");
    Ok(())
}

#[test]
fn reader_programming_is_encoded_and_reported_as_control_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    let code = DataMatrixBuilder::new()
        .with_reader_programming(true)
        .encode(b"ABC")?;
    assert_eq!(
        code.data_codewords().first(),
        Some(&encodation::READER_PROGRAMMING)
    );

    let message = data::decode_message(code.data_codewords())?;
    assert!(message.is_reader_programming());
    assert_eq!(message.data(), b"ABC");
    assert!(message.transmission().is_empty());

    let code = DataMatrixBuilder::new()
        .with_reader_programming(true)
        .with_fnc1(Some(Fnc1Position::Second))
        .encode(b"A")?;
    let message = data::decode_message(code.data_codewords())?;
    assert!(message.is_reader_programming());
    assert_eq!(message.fnc1(), Some(Fnc1Position::Second));

    let conflict = DataMatrixBuilder::new()
        .with_reader_programming(true)
        .with_fnc1(Some(Fnc1Position::First))
        .encode(b"A");
    assert!(matches!(
        conflict,
        Err(DataEncodingError::InvalidControlPosition(_))
    ));

    let misplaced = DataMatrix::encode_segments(
        &[
            DataMatrixSegment::Data(b"A"),
            DataMatrixSegment::ReaderProgramming,
        ],
        SymbolList::default(),
    );
    assert!(matches!(
        misplaced,
        Err(DataEncodingError::InvalidControlPosition(_))
    ));
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
fn decode_accepts_all_quarter_turns_and_reflectance_reversal()
-> Result<(), Box<dyn std::error::Error>> {
    let input = b"rotate";
    let code = DataMatrix::encode(input, SymbolSize::Rect8x32)?;
    let bitmap = code.bitmap();
    let canonical: Vec<bool> = bitmap.bits().into();

    for inverted in [false, true] {
        for quarter_turns in 0..4 {
            let (transformed, width) =
                transform_modules(&canonical, bitmap.width(), quarter_turns, inverted)
                    .ok_or("module dönüşümü başarısız")?;
            assert_eq!(
                DataMatrix::decode(&transformed, width),
                Ok(input.to_vec()),
                "quarter_turns={quarter_turns}, inverted={inverted}",
            );
        }
    }
    Ok(())
}

#[test]
fn test_annex_i_example() -> Result<(), Box<dyn std::error::Error>> {
    // ISO/IEC 16022:2024 Annex I: "123456" verisi 10x10 symbol'e, codeword
    // dizisi 142 164 186 | 114 25 5 88 102 olacak biçimde encode edilir ve
    // mapping bölgesi Figure I.1 ile birebir aynıdır.
    let code = DataMatrix::encode(b"123456", SymbolList::default())?;
    assert_eq!(code.size(), SymbolSize::Square10);
    assert_eq!(code.codewords(), &[142, 164, 186, 114, 25, 5, 88, 102]);

    let bitmap = code.bitmap();
    assert_eq!(bitmap.width(), 10);
    let bits: Vec<bool> = bitmap.bits().into();
    assert_eq!(bits.len(), 100);
    // Figure I.1 — 8x8 mapping matrisi (1 = koyu modül).
    const FIGURE_I1: [&str; 8] = [
        "10010110", "10000010", "10001110", "10000100", "00000111", "11011000", "11101100",
        "00111010",
    ];
    for (row, modules) in FIGURE_I1.iter().enumerate() {
        for (col, module) in modules.bytes().enumerate() {
            assert_eq!(
                bits.get((row + 1) * 10 + col + 1).copied(),
                Some(module == b'1'),
                "Figure I.1 modülü ({row}, {col})"
            );
        }
    }
    // Finder pattern: sol sütun ve alt satır koyu; üst satır ve sağ sütun alternatif.
    for i in 0..10 {
        assert_eq!(bits.get(i * 10).copied(), Some(true), "sol sütun {i}");
        assert_eq!(bits.get(90 + i).copied(), Some(true), "alt satır {i}");
        assert_eq!(bits.get(i).copied(), Some(i % 2 == 0), "üst satır {i}");
        assert_eq!(
            bits.get(i * 10 + 9).copied(),
            Some(i % 2 == 1),
            "sağ sütun {i}"
        );
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

#[test]
fn automatic_size_selection_reaches_compacted_square144_capacities() -> Result<(), DataEncodingError>
{
    for (byte, len) in [(b'A', 1_556), (b'A', 2_335), (b'0', 3_116)] {
        let input = alloc::vec![byte; len];
        let encoded = DataMatrix::encode(&input, SymbolList::default())?;
        assert_eq!(data::decode_data(encoded.data_codewords()), Ok(input));
    }

    assert_eq!(
        DataMatrix::encode(&alloc::vec![b'0'; 3_117], SymbolList::default()),
        Err(DataEncodingError::TooMuchOrIllegalData)
    );
    Ok(())
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

    // İlk konumdaki FNC1, piksellerden decode edilirken ]d2 symbology
    // identifier değerini üretir (ISO/IEC 16022:2024, 12.2 ve Annex H).
    let bitmap = result.bitmap();
    let pixels: Vec<bool> = bitmap.bits().into();
    let message = DataMatrix::decode_message(&pixels, bitmap.width())?;
    assert_eq!(message.data(), data);
    assert_eq!(message.fnc1(), Some(Fnc1Position::First));
    assert_eq!(message.symbology_identifier(), "]d2");
    Ok(())
}

#[test]
fn test_fnc1_second_position_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    // ISO/IEC 16022:2024, 7.2.4.7 ve 12.3: FNC1 ikinci symbol karakteri
    // konumundadır; ilk konum tek codeword'lük ASCII verisi taşır.
    let data = b"A1234567\x1D99";
    let code = DataMatrixBuilder::new()
        .with_fnc1(Some(Fnc1Position::Second))
        .encode(data)?;
    assert_eq!(code.data_codewords().get(..2), Some(&[66, 232][..]));
    let message = data::decode_message(code.data_codewords())?;
    assert_eq!(message.data(), data);
    assert_eq!(message.fnc1(), Some(Fnc1Position::Second));
    assert_eq!(message.symbology_identifier(), "]d3");

    // İlk iki karakter rakamsa ilk konum rakam çifti codeword'ü olur; FNC1
    // yine ikinci konumdadır.
    let code = DataMatrixBuilder::new()
        .with_fnc1(Some(Fnc1Position::Second))
        .encode(b"1712345")?;
    assert_eq!(code.data_codewords().get(..2), Some(&[147, 232][..]));
    let message = data::decode_message(code.data_codewords())?;
    assert_eq!(message.data(), b"1712345");
    assert_eq!(message.fnc1(), Some(Fnc1Position::Second));
    Ok(())
}

#[test]
fn test_fnc1_second_position_requires_single_ascii_codeword() {
    // İlk karakter extended ASCII ise iki codeword gerekir ve FNC1 ikinci
    // konuma yerleştirilemez; boş veri için de ilk konum doldurulamaz.
    for data in [&b"\xFFabc"[..], &b""[..]] {
        assert_eq!(
            DataMatrixBuilder::new()
                .with_fnc1(Some(Fnc1Position::Second))
                .encode(data),
            Err(DataEncodingError::TooMuchOrIllegalData)
        );
    }
}
