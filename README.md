# datamatrix-rs

[![crates.io](https://img.shields.io/crates/d/datamatrix.svg)](https://crates.io/crates/datamatrix)
[![Dokümantasyon](https://docs.rs/datamatrix/badge.svg)](https://docs.rs/datamatrix)
![Lisans](https://img.shields.io/crates/l/datamatrix)

Optimizing encoder içeren Data Matrix (ECC 200) decoding ve encoding kütüphanesi.

<p align="center">
  <img src="src/datamatrix-rs.png" alt="'datamatrix-rs' Data Matrix encoding">
</p>

Bu kütüphane mümkün olan en az codeword sayısıyla encoding üreten, optimizing ve
doğrusal zamanda çalışan bir encoder içerir.

Data Matrix standardı (ISO/IEC 16022:2006), kullanılacak encoding mode'larını
seçmek için çoğu durumda çalışan bir heuristic içerir. Ancak doğrudan bir
implementasyon doğrusal runtime sunmaz. Bu kütüphane mode seçimini encodation
mode'ları üzerinde shortest path problemi olarak ele alır. Her input karakterinden
sonra mode başına yalnızca en düşük cost'lu planı tutan ileri taramayla (dominance
pruning) problemi çözer. Böylece karakter başına iş sabit bir değerle sınırlanır ve
doğrusal runtime elde edilir.

Bu implementasyonu özel kılan optimizer'dır; çoğu implementasyon heuristic
kullanır. Katkı kaynakları ve diğer açık kaynaklı Data Matrix kütüphaneleri için
aşağıdaki ilgili projeler listesine bakın.

## Örnek

```rust
use datamatrix::{DataMatrix, SymbolList};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let code = DataMatrix::encode(
        b"Hello, World!",
        SymbolList::default(),
    )?;

    // "ASCII art" sürümünü yazdırır.
    print!("{}", code.bitmap().unicode());
    Ok(())
}
```

Kütüphane diğer çıktı formatlarını üretmek için yardımcılar içerir. Örnek kodlar
`examples/` dizinindedir. Son rendering adımının ek maliyeti genellikle düşüktür ve
bu yaklaşım yüksek esneklik sağlar.

## Hata ve panik sözleşmesi

Kamuya açık fonksiyonlar geçersiz dış girdi, desteklenmeyen seçenek veya normal
çalışma hatası nedeniyle kasıtlı olarak paniklemez. Bu durumlar yapılandırılmış
`Result<T, E>` ile bildirilir. `Bitmap::new` ve
`MatrixMap::new_with_codewords` gibi sınır kurucuları girdiyi bir kez doğrular;
private alanlarla korunan geçerli değerler üzerindeki `bitmap`, `codewords`,
`path`, `width` ve `height` gibi işlemler doğrudan sonuç döndürür.

Panik yalnızca kütüphanenin private durumunu koruyan bir iç değişmezin bozulduğunu,
yani bir programlama hatasını bildirir. Bu noktalar tek bir açıklamalı iç-değişmez
mekanizmasında toplanmıştır. Bellek tükenmesi, stack overflow ve bağımlılıkların
panikleri bu sözleşmenin dışındadır.

## Durum

- [x] ASCII, Base256, C40, Text, X12 ve EDIFACT encodation mode'ları.
- [x] Asgari encodation size için encodation mode'ları arasında geçiş yapan optimizer.
- [x] Data bölümü decoding.
- [x] Fuzzing uygulanmış data decoding ve encoding (_48 saat sonunda sorun yok_).
- [x] Diğer implementasyonlardaki açık hata bildirimlerinin denetlenmesi.
- [x] Reed–Solomon decoder/encoder.
- [x] Tile placement encoding.
- [x] Rendering yardımcıları.
- [x] ISO 21471 ile tanımlanan ve yeni rectangular symbol size değerleri ekleyen
      [Extended Rectangular Data Matrix (DMRE)](https://e-d-c.info/projekte/dmre.html).
- [x] Tile placement decoding.
- [x] Sınırlı ECI desteği; `extended_eci` feature ek charset'leri etkinleştirir.
- [ ] Görsellerde visual detection.
- [ ] Ayrıntılı decoder çıktısı.

Bunlardan sonrası için değerlendirilenler:

- "Structured Append"
- "Reader Programming"

## Açıklama

Encoded data, Data Matrix symbol içindeki kalan alanı doldurmak için padded
olduğundan bu kütüphanenin ürettiği symbol çoğu durumda specification içindeki
heuristic tabanlı optimizer'ın sonucundan daha küçük olmaz. Bununla birlikte her
durumda doğrusal encoding süresi sağlar ve heuristic kullanımından kaynaklanabilen
bazı hataları önler (zxing ve OkapiBarcode açık hatalarına bakın). Kapsamlı bir
çalışma yapılmamış olsa da daha küçük symbol döndürdüğü durumlar da vardır.

## İlgili projeler

Aşağıdaki projelerin implementasyonları, test vakaları ve hata bildirimleri çok
değerli kaynaklar olmuştur.

- [zxing](https://github.com/zxing/zxing), Data Matrix dahil çeşitli 1D ve 2D
  code'ları encode/decode eden Google kütüphanesidir. Ana bölümü Java ile yazılmıştır
  ve specification içindeki heuristic'i kullanır.
- [barcode4j](http://barcode4j.sourceforge.net/), zxing'in öncülüdür (?); Data Matrix
  kodu zxing içine fork edilmiştir.
- [libdmtx](https://github.com/dmtx/libdmtx), Data Matrix encoding ve decoding için
  en bilinen açık kaynaklı C kütüphanesidir. Specification'a göre daha sınırlı bir
  optimizer içerir ancak görsellerdeki Data Matrix code'larını da decode edebilir.
- [zxing-cpp](https://github.com/nu-book/zxing-cpp), zxing'in bazı iyileştirmeler de
  içeren C++ port'udur.
- [OkapiBarcode](https://github.com/woo-j/OkapiBarcode), onlarca başka code yanında
  Data Matrix encoding desteği sunan Java kütüphanesidir. Implementasyon standardı
  izliyor görünmektedir.
- OkapiBarcode, [zint](http://zint.org.uk) C kütüphanesinden port edilmiştir (?).
  Web sitesinde Pascal ve C# port'larına referans verilir. Konu dışı bir not: Web
  sitesindeki "Extras" bölümünde güzel vintage code'lar ve kullanımdan kalkmış ticari
  code'lar için encoder'lar vardır.
- [postscriptbarcode](https://github.com/bwipp/postscriptbarcode), yalnızca PostScript
  kullanarak çeşitli 1D ve 2D code'ları encode eder. LaTeX paketi ve
  [JavaScript port'u](https://github.com/metafloor/bwip-js) da vardır.
- Encoding için bir [Perl modülü](https://github.com/mstratman/Barcode-DataMatrix).
- [iec16022](https://github.com/rdoeffinger/iec16022), ilk olarak Andrews & Arnold Ltd.
  tarafından yazılan ve artık Reimar Döffinger tarafından sürdürülen Data Matrix
  encoder'dır. Benzer bir optimizing encoder içerir.
