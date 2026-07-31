//! Data Matrix symbol içindeki bitlerin yerleşimi.
//!
//! Modül, son symbol içindeki her codeword'ün bit konumlarını dolaşmak için
//! kullanılabilen [MatrixMap] yapısını içerir. Başka bir deyişle siyah karelerin
//! byte olarak encoded data ile nasıl eşlendiğini tanımlar. Encoded data'yı bitmap'e
//! yazmak ve bitmap'ten okumak için kullanılır.
//!
//! Soyut bitmap yapısı [Bitmap], encoding işleminin son çıktısı ve decoding
//! işleminin input'udur. Rendering yardımcılarını da içerir.
use alloc::{string::String, vec, vec::Vec};

use crate::symbol_size::{SymbolList, SymbolSize};

#[cfg(test)]
use pretty_assertions::assert_eq;

mod path;

pub use path::PathSegment;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BitmapConversionError {
    /// Alignment pattern geçersizdir.
    Alignment,
    /// Padding pattern geçersizdir.
    Padding,
    /// Genişlik sıfırdır.
    ZeroWidth,
    /// Verilen data belirtilen genişliğe uymaz.
    DataSize,
    /// Data boyutuyla eşleşen symbol size bulunamadı.
    SymbolSize,
    /// Codeword sayısı seçilen symbol size ile eşleşmiyor.
    CodewordCount { expected: usize, actual: usize },
    /// Boyut hesabı hedef mimarinin sınırlarını aştı.
    ArithmeticOverflow,
}

impl core::fmt::Display for BitmapConversionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Alignment => f.write_str("Data Matrix alignment deseni geçersiz"),
            Self::Padding => f.write_str("Data Matrix padding deseni geçersiz"),
            Self::ZeroWidth => f.write_str("bitmap genişliği sıfır olamaz"),
            Self::DataSize => f.write_str("bitmap verisi belirtilen genişlikle uyuşmuyor"),
            Self::SymbolSize => f.write_str("bitmap boyutlarına uyan bir symbol size bulunamadı"),
            Self::CodewordCount { expected, actual } => write!(
                f,
                "codeword sayısı geçersiz: {expected} bekleniyordu, {actual} verildi"
            ),
            Self::ArithmeticOverflow => f.write_str("bitmap boyut hesabında taşma oluştu"),
        }
    }
}

impl core::error::Error for BitmapConversionError {}

/// [MatrixMap] içinde kullanılan soyut "bit" türü.
pub trait Bit: Clone + Copy + PartialEq + core::fmt::Debug {
    const LOW: Self;
    const HIGH: Self;
}

/// Data Matrix symbol bitlerinin alignment pattern olmadan gösterimi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixMap<B: Bit> {
    entries: Vec<B>,
    width: usize,
    height: usize,
    extra_vertical_alignments: usize,
    extra_horizontal_alignments: usize,
    has_padding: bool,
}

impl<M: Bit> MatrixMap<M> {
    /// Verilen symbol size için yeni ve boş bir matrix oluşturur.
    pub fn new(size: SymbolSize) -> Self {
        let setup = size.block_setup();
        let w = setup.content_width();
        let h = setup.content_height();
        let Some(len) = w.checked_mul(h) else {
            crate::invariant_violation("symbol içerik boyutu hedef mimarinin sınırlarını aştı");
        };
        Self {
            entries: vec![M::LOW; len],
            width: w,
            height: h,
            extra_vertical_alignments: setup.extra_vertical_alignments,
            extra_horizontal_alignments: setup.extra_horizontal_alignments,
            has_padding: size.has_padding_modules(),
        }
    }

    /// Data'yı bitmap'ten okur.
    ///
    /// `bits` argümanı, sol üst köşeden başlayarak row-major sırada numaralanmış
    /// dikdörtgen bir görseli temsil etmelidir. Alignment pattern dahil olmalıdır.
    ///
    /// # Hatalar
    ///
    /// Genişlik sıfırsa, data genişlikle uyuşmuyorsa, symbol size bulunamıyorsa
    /// veya alignment/padding pattern geçersizse hata döndürür.
    pub fn try_from_bits(
        bits: &[M],
        width: usize,
    ) -> Result<(Self, SymbolSize), BitmapConversionError>
    where
        M: PartialEq,
    {
        if width == 0 {
            return Err(BitmapConversionError::ZeroWidth);
        }
        if !bits.len().is_multiple_of(width) {
            return Err(BitmapConversionError::DataSize);
        }
        let height = bits.len() / width;
        let size = SymbolList::all()
            .iter()
            .find(|s| {
                let bs = s.block_setup();
                bs.width == width && bs.height == height
            })
            .ok_or(BitmapConversionError::SymbolSize)?;
        let setup = size.block_setup();
        let w = setup.content_width();
        let h = setup.content_height();
        let content_len = w
            .checked_mul(h)
            .ok_or(BitmapConversionError::ArithmeticOverflow)?;
        let mut entries = Vec::with_capacity(content_len);

        let blk_h = h / (setup.extra_horizontal_alignments + 1);
        let blk_w = w / (setup.extra_vertical_alignments + 1);

        let row_chunk_width = blk_h
            .checked_add(2)
            .and_then(|rows| rows.checked_mul(width))
            .ok_or(BitmapConversionError::ArithmeticOverflow)?;
        for row_chunk in bits.chunks(row_chunk_width) {
            if row_chunk.len() != row_chunk_width {
                return Err(BitmapConversionError::DataSize);
            }

            // İlk satır dönüşümlü, ondan önceki satırın tamamı HIGH olmalıdır.
            let first_row = row_chunk
                .get(..width)
                .ok_or(BitmapConversionError::DataSize)?;
            let last_start = blk_h
                .checked_add(1)
                .and_then(|row| row.checked_mul(width))
                .ok_or(BitmapConversionError::ArithmeticOverflow)?;
            let last_row = row_chunk
                .get(last_start..)
                .ok_or(BitmapConversionError::DataSize)?;
            if last_row.len() != width {
                return Err(BitmapConversionError::DataSize);
            }
            let alignment_ok = last_row.iter().all(|b| *b == M::HIGH)
                && first_row
                    .iter()
                    .zip([M::HIGH, M::LOW].into_iter().cycle())
                    .all(|(a, b)| *a == b);
            if !alignment_ok {
                return Err(BitmapConversionError::Alignment);
            }

            let rows = row_chunk
                .get(width..last_start)
                .ok_or(BitmapConversionError::DataSize)?;
            let expected_rows_len = blk_h
                .checked_mul(width)
                .ok_or(BitmapConversionError::ArithmeticOverflow)?;
            if rows.len() != expected_rows_len || !width.is_multiple_of(blk_w + 2) {
                return Err(BitmapConversionError::DataSize);
            }
            let mut alignment_bit = M::LOW;
            for (j, row) in rows.chunks(blk_w + 2).enumerate() {
                if row.len() != blk_w + 2 {
                    return Err(BitmapConversionError::DataSize);
                }
                if j % (setup.extra_vertical_alignments + 1) == 0 {
                    alignment_bit = if alignment_bit == M::LOW {
                        M::HIGH
                    } else {
                        M::LOW
                    };
                }
                let alignment_ok = row.first().copied() == Some(M::HIGH)
                    && row.get(blk_w + 1).copied() == Some(alignment_bit);
                if !alignment_ok {
                    return Err(BitmapConversionError::Alignment);
                }
                let content = row
                    .get(1..blk_w + 1)
                    .ok_or(BitmapConversionError::DataSize)?;
                entries.extend_from_slice(content);
            }
        }
        if entries.len() != content_len {
            return Err(BitmapConversionError::DataSize);
        }

        if size.has_padding_modules() {
            let bottom_start = entries
                .len()
                .checked_sub(2)
                .ok_or(BitmapConversionError::DataSize)?;
            let top_start = entries
                .len()
                .checked_sub(
                    w.checked_add(2)
                        .ok_or(BitmapConversionError::ArithmeticOverflow)?,
                )
                .ok_or(BitmapConversionError::DataSize)?;
            let top_end = top_start
                .checked_add(2)
                .ok_or(BitmapConversionError::ArithmeticOverflow)?;
            let padding_ok = entries.get(bottom_start..) == Some([M::LOW, M::HIGH].as_slice())
                && entries.get(top_start..top_end) == Some([M::HIGH, M::LOW].as_slice());
            if !padding_ok {
                return Err(BitmapConversionError::Padding);
            }
        }

        let matrix_map = Self {
            entries,
            width: w,
            height: h,
            extra_vertical_alignments: setup.extra_vertical_alignments,
            extra_horizontal_alignments: setup.extra_horizontal_alignments,
            has_padding: size.has_padding_modules(),
        };
        Ok((matrix_map, size))
    }

    /// Gerekiyorsa sağ alt köşeye 4×4 padding pattern yazar.
    pub fn write_padding(&mut self) {
        if self.has_padding {
            let Some(first_row) = self.height.checked_sub(2) else {
                crate::invariant_violation("padding için matrix yüksekliği yetersiz");
            };
            let Some(first_col) = self.width.checked_sub(2) else {
                crate::invariant_violation("padding için matrix genişliği yetersiz");
            };
            let Some(last_row) = self.height.checked_sub(1) else {
                crate::invariant_violation("padding için matrix yüksekliği yetersiz");
            };
            let Some(last_col) = self.width.checked_sub(1) else {
                crate::invariant_violation("padding için matrix genişliği yetersiz");
            };
            self.set_bit(first_row, first_col, M::HIGH);
            self.set_bit(last_row, last_col, M::HIGH);
        }
    }

    /// Matrix içeriğini alignment pattern eklenmiş bitmap olarak döndürür.
    pub fn bitmap(&self) -> Bitmap<M> {
        let Some(h) = self
            .extra_horizontal_alignments
            .checked_mul(2)
            .and_then(|extra| self.height.checked_add(2)?.checked_add(extra))
        else {
            crate::invariant_violation("bitmap yüksekliği hesaplanırken taşma oluştu");
        };
        let Some(w) = self
            .extra_vertical_alignments
            .checked_mul(2)
            .and_then(|extra| self.width.checked_add(2)?.checked_add(extra))
        else {
            crate::invariant_violation("bitmap genişliği hesaplanırken taşma oluştu");
        };
        let Some(len) = h.checked_mul(w) else {
            crate::invariant_violation("bitmap alanı hesaplanırken taşma oluştu");
        };
        let mut bits = vec![M::LOW; len];

        // Yatay alignment öğelerini çizer.
        let extra_hor = self.extra_horizontal_alignments;
        let blk_h = (h - 2 * (extra_hor + 1)) / (extra_hor + 1);
        for i in 0..extra_hor {
            let rows_before = 1 + (blk_h + 2) * i + blk_h;
            for j in 0..w {
                set_bitmap_bit(&mut bits, w, rows_before, j, M::HIGH);
            }
            for j in (0..w).step_by(2) {
                set_bitmap_bit(&mut bits, w, rows_before + 1, j, M::HIGH);
            }
        }

        // Dikey alignment öğelerini çizer.
        let extra_ver = self.extra_vertical_alignments;
        let blk_w = (w - 2 * (extra_ver + 1)) / (extra_ver + 1);
        for j in 0..extra_ver {
            let cols_before = 1 + (blk_w + 2) * j + blk_w;
            for i in 1..h {
                set_bitmap_bit(&mut bits, w, i, cols_before + 1, M::HIGH);
            }
            for i in (1..h).step_by(2) {
                set_bitmap_bit(&mut bits, w, i, cols_before, M::HIGH);
            }
        }

        for j in 0..w {
            // Alt alignment öğesini çizer.
            set_bitmap_bit(&mut bits, w, h - 1, j, M::HIGH);
        }
        for j in (0..w).step_by(2) {
            // Üst alignment öğesini çizer.
            set_bitmap_bit(&mut bits, w, 0, j, M::HIGH);
        }
        for i in 0..h {
            // Sol alignment öğesini çizer.
            set_bitmap_bit(&mut bits, w, i, 0, M::HIGH);
        }
        for i in (1..h).step_by(2) {
            // Sağ alignment öğesini çizer.
            set_bitmap_bit(&mut bits, w, i, w - 1, M::HIGH);
        }

        // Data'yı kopyalar.
        for (b_i, b) in self.entries.iter().enumerate() {
            let mut i = b_i / self.width;
            i += 1 + (i / blk_h) * 2;
            let mut j = b_i % self.width;
            j += 1 + (j / blk_w) * 2;
            set_bitmap_bit(&mut bits, w, i, j, *b);
        }

        Bitmap { width: w, bits }
    }

    /// Symbol'ü codeword sırasında dolaşır ve her konum için fonksiyonu çağırır.
    ///
    /// Codeword indeksi `visit` fonksiyonuna ilk argüman olarak verilir.
    ///
    /// `visit` fonksiyonunun ikinci argümanı, en anlamlı bit önce olacak biçimde
    /// codeword bitlerini içerir.
    pub fn traverse_mut<F>(&mut self, mut visit_fn: F)
    where
        F: FnMut(usize, [&mut M; 8]),
    {
        IndexTraversal {
            width: self.width,
            height: self.height,
        }
        .run(|idx, indices| visit_fn(idx, self.bits_mut(indices)))
    }

    /// [traverse_mut](Self::traverse_mut) yönteminin immutable sürümü.
    pub fn traverse<F>(&self, mut visit_fn: F)
    where
        F: FnMut(usize, [M; 8]),
    {
        IndexTraversal {
            width: self.width,
            height: self.height,
        }
        .run(|idx, indices| {
            let [i0, i1, i2, i3, i4, i5, i6, i7] = indices;
            let values = [
                self.entry(i0),
                self.entry(i1),
                self.entry(i2),
                self.entry(i3),
                self.entry(i4),
                self.entry(i5),
                self.entry(i6),
                self.entry(i7),
            ];
            visit_fn(idx, values)
        })
    }

    fn entry(&self, index: usize) -> M {
        let Some(value) = self.entries.get(index).copied() else {
            crate::invariant_violation("codeword yerleşim konumu matrix sınırlarının dışında");
        };
        value
    }

    fn set_bit(&mut self, row: usize, column: usize, value: M) {
        let index = bitmap_index(self.width, row, column);
        let Some(bit) = self.entries.get_mut(index) else {
            crate::invariant_violation("matrix konumu sınırların dışında");
        };
        *bit = value;
    }

    /// `indices` içinde belirtilen indekslere mutable referanslar döndürür.
    fn bits_mut(&mut self, indices: [usize; 8]) -> [&mut M; 8] {
        let Ok(bits) = self.entries.get_disjoint_mut(indices) else {
            crate::invariant_violation("codeword bit konumları geçersiz veya birbiriyle çakışıyor");
        };
        bits
    }
}

fn bitmap_index(width: usize, row: usize, column: usize) -> usize {
    let Some(index) = row
        .checked_mul(width)
        .and_then(|start| start.checked_add(column))
    else {
        crate::invariant_violation("bitmap konumu hesaplanırken taşma oluştu");
    };
    index
}

fn set_bitmap_bit<M: Bit>(bits: &mut [M], width: usize, row: usize, column: usize, value: M) {
    let index = bitmap_index(width, row, column);
    let Some(bit) = bits.get_mut(index) else {
        crate::invariant_violation("bitmap konumu sınırların dışında");
    };
    *bit = value;
}

struct IndexTraversal {
    width: usize,
    height: usize,
}

impl IndexTraversal {
    fn run<F>(&self, mut visit_fn: F)
    where
        F: FnMut(usize, [usize; 8]),
    {
        let Ok(nrow) = isize::try_from(self.height) else {
            crate::invariant_violation("matrix yüksekliği isize sınırını aştı");
        };
        let Ok(ncol) = isize::try_from(self.width) else {
            crate::invariant_violation("matrix genişliği isize sınırını aştı");
        };
        let Some(entry_count) = self.height.checked_mul(self.width) else {
            crate::invariant_violation("matrix alanı hesaplanırken taşma oluştu");
        };
        let mut visited = vec![false; entry_count];

        // İlk karakterin 8. biti için doğru konumdan başlar.
        let mut i = 4;
        let mut j = 0;
        let mut codeword_idx = 0;

        macro_rules! visit {
            ($indices:expr) => {
                let ii = $indices;
                for v in ii {
                    let Some(seen) = visited.get_mut(v) else {
                        crate::invariant_violation(
                            "codeword yerleşim konumu matrix sınırlarının dışında",
                        );
                    };
                    *seen = true;
                }
                visit_fn(codeword_idx, ii);
                let Some(next_codeword_idx) = codeword_idx.checked_add(1) else {
                    crate::invariant_violation("codeword indeksi hesaplanırken taşma oluştu");
                };
                codeword_idx = next_codeword_idx;
            };
        }

        loop {
            // Her turda önce özel köşe durumlarından birini denetler.
            if i == nrow && j == 0 {
                visit!(self.corner1());
            }
            if i == nrow - 2 && j == 0 && ncol % 4 != 0 {
                visit!(self.corner2());
            }
            if i == nrow - 2 && j == 0 && ncol % 8 == 4 {
                visit!(self.corner3());
            }
            if i == nrow + 4 && j == 2 && ncol % 8 == 0 {
                visit!(self.corner4());
            }
            // Çapraz olarak yukarı tarar.
            loop {
                if i < nrow && j >= 0 && !self.was_visited(&visited, i, j) {
                    visit!(self.utah(i, j));
                }
                i -= 2;
                j += 2;
                if !(i >= 0 && j < ncol) {
                    break;
                }
            }
            i += 1;
            j += 3;

            // Çapraz olarak aşağı tarar.
            loop {
                if i >= 0 && j < ncol && !self.was_visited(&visited, i, j) {
                    visit!(self.utah(i, j));
                }
                i += 2;
                j -= 2;
                if !(i < nrow && j >= 0) {
                    break;
                }
            }
            i += 3;
            j += 1;

            // Map'in tamamı dolaşılana kadar sürdürür.
            if !(i < nrow || j < ncol) {
                break;
            }
        }
    }

    fn was_visited(&self, visited: &[bool], row: isize, column: isize) -> bool {
        let Ok(row) = usize::try_from(row) else {
            crate::invariant_violation("negatif matrix satırı ziyaret edilmeye çalışıldı");
        };
        let Ok(column) = usize::try_from(column) else {
            crate::invariant_violation("negatif matrix sütunu ziyaret edilmeye çalışıldı");
        };
        let index = bitmap_index(self.width, row, column);
        let Some(value) = visited.get(index).copied() else {
            crate::invariant_violation("ziyaret edilen matrix konumu sınırların dışında");
        };
        value
    }

    /// Wrapping uygulayarak indeksi hesaplar.
    fn idx(&self, mut i: isize, mut j: isize) -> usize {
        let Ok(h) = isize::try_from(self.height) else {
            crate::invariant_violation("matrix yüksekliği isize sınırını aştı");
        };
        let Ok(w) = isize::try_from(self.width) else {
            crate::invariant_violation("matrix genişliği isize sınırını aştı");
        };
        if i < 0 {
            let Some(wrapped_i) = i.checked_add(h) else {
                crate::invariant_violation("matrix satırı sarmalanırken taşma oluştu");
            };
            i = wrapped_i;
            let adjustment = 4 - ((h + 4) % 8);
            let Some(adjusted_j) = j.checked_add(adjustment) else {
                crate::invariant_violation("matrix sütunu ayarlanırken taşma oluştu");
            };
            j = adjusted_j;
        }
        if j < 0 {
            let Some(wrapped_j) = j.checked_add(w) else {
                crate::invariant_violation("matrix sütunu sarmalanırken taşma oluştu");
            };
            j = wrapped_j;
            let adjustment = 4 - ((w + 4) % 8);
            let Some(adjusted_i) = i.checked_add(adjustment) else {
                crate::invariant_violation("matrix satırı ayarlanırken taşma oluştu");
            };
            i = adjusted_i;
        }
        // Bu işlem DMRE size değerleri için gereklidir.
        if i >= h {
            let Some(wrapped_i) = i.checked_sub(h) else {
                crate::invariant_violation("matrix satırı sarmalanırken taşma oluştu");
            };
            i = wrapped_i;
        }
        if !(i >= 0 && i < h && j >= 0 && j < w) {
            crate::invariant_violation("sarmalanmış codeword konumu matrix sınırlarının dışında");
        }
        let Ok(row) = usize::try_from(i) else {
            crate::invariant_violation("matrix satırı usize değerine dönüştürülemedi");
        };
        let Ok(column) = usize::try_from(j) else {
            crate::invariant_violation("matrix sütunu usize değerine dönüştürülemedi");
        };
        bitmap_index(self.width, row, column)
    }

    /// Utah biçimli symbol (standart symbol) için indeksleri hesaplar.
    fn utah(&self, i: isize, j: isize) -> [usize; 8] {
        [
            self.idx(i - 2, j - 2),
            self.idx(i - 2, j - 1),
            self.idx(i - 1, j - 2),
            self.idx(i - 1, j - 1),
            self.idx(i - 1, j),
            self.idx(i, j - 2),
            self.idx(i, j - 1),
            self.idx(i, j),
        ]
    }

    fn dimensions(&self) -> (isize, isize) {
        let Ok(height) = isize::try_from(self.height) else {
            crate::invariant_violation("matrix yüksekliği isize sınırını aştı");
        };
        let Ok(width) = isize::try_from(self.width) else {
            crate::invariant_violation("matrix genişliği isize sınırını aştı");
        };
        (height, width)
    }

    fn corner1(&self) -> [usize; 8] {
        let (h, w) = self.dimensions();
        [
            self.idx(h - 1, 0),
            self.idx(h - 1, 1),
            self.idx(h - 1, 2),
            self.idx(0, w - 2),
            self.idx(0, w - 1),
            self.idx(1, w - 1),
            self.idx(2, w - 1),
            self.idx(3, w - 1),
        ]
    }

    fn corner2(&self) -> [usize; 8] {
        let (h, w) = self.dimensions();
        [
            self.idx(h - 3, 0),
            self.idx(h - 2, 0),
            self.idx(h - 1, 0),
            self.idx(0, w - 4),
            self.idx(0, w - 3),
            self.idx(0, w - 2),
            self.idx(0, w - 1),
            self.idx(1, w - 1),
        ]
    }

    fn corner3(&self) -> [usize; 8] {
        let (h, w) = self.dimensions();
        [
            self.idx(h - 3, 0),
            self.idx(h - 2, 0),
            self.idx(h - 1, 0),
            self.idx(0, w - 2),
            self.idx(0, w - 1),
            self.idx(1, w - 1),
            self.idx(2, w - 1),
            self.idx(3, w - 1),
        ]
    }

    fn corner4(&self) -> [usize; 8] {
        let (h, w) = self.dimensions();
        [
            self.idx(h - 1, 0),
            self.idx(h - 1, w - 1),
            self.idx(0, w - 3),
            self.idx(0, w - 2),
            self.idx(0, w - 1),
            self.idx(1, w - 3),
            self.idx(1, w - 2),
            self.idx(1, w - 1),
        ]
    }
}

impl MatrixMap<bool> {
    /// MatrixMap oluşturur ve codeword'lerle doldurur.
    ///
    /// # Hatalar
    ///
    /// `data` uzunluğu seçilen symbol size değerinin codeword sayısıyla
    /// eşleşmiyorsa hata döndürür.
    pub fn new_with_codewords(
        data: &[u8],
        symbol_size: SymbolSize,
    ) -> Result<Self, BitmapConversionError> {
        let expected = symbol_size.num_codewords();
        if data.len() != expected {
            return Err(BitmapConversionError::CodewordCount {
                expected,
                actual: data.len(),
            });
        }
        let mut matrix = Self::new(symbol_size);
        matrix.copy_from_codewords(data);
        Ok(matrix)
    }

    /// Data'yı codeword'lerden karşılık gelen konumlara kopyalar.
    ///
    /// Gerekiyorsa padding pattern da yazar.
    ///
    fn copy_from_codewords(&mut self, data: &[u8]) {
        self.traverse_mut(|idx, bits| {
            let Some(mut codeword) = data.get(idx).copied() else {
                crate::invariant_violation("doğrulanmış codeword konumu data sınırlarının dışında");
            };
            for bit in bits.into_iter().rev() {
                *bit = codeword & 1 == 1;
                codeword >>= 1;
            }
        });
        self.write_padding();
    }

    /// Codeword'leri çıkarır.
    ///
    /// Error correction codeword'leri de sonuca dahildir.
    pub fn codewords(&self) -> Vec<u8> {
        let mut data = vec![0; self.entries.len() / 8];
        self.traverse(|idx, bits| {
            let Some(codeword) = data.get_mut(idx) else {
                crate::invariant_violation("çıkarılan codeword konumu çıktı sınırlarının dışında");
            };
            for bit in bits {
                *codeword = (*codeword << 1) | (bit as u8);
            }
        });
        data
    }
}

/// Soyut bitmap.
///
/// İçeriği render etmek için yardımcılar içerir. Piksel benzeri hedeflerde
/// [pixels()](Self::pixels), vector formatlarda [path()](Self::path) kullanılabilir.
#[derive(Clone, Debug)]
pub struct Bitmap<M> {
    width: usize,
    bits: Vec<M>,
}

impl Bit for bool {
    const LOW: bool = false;
    const HIGH: bool = true;
}

impl<B: Bit> Bitmap<B> {
    /// Yeni bir Bitmap oluşturur.
    ///
    /// Böylece rendering yardımcıları örneğin QR code'lar için de kullanılabilir:
    ///
    /// ```rust
    /// # use datamatrix::placement::Bitmap;
    /// use qrcode2::{QrCode, Color};
    ///
    /// let code = QrCode::new(b"Hello, World!!")?;
    /// let width = code.width();
    /// let bits = code.into_colors().into_iter().map(|c| c == Color::Dark);
    /// let bitmap = Bitmap::new(bits, width)?;
    /// print!("{}", bitmap.unicode());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Hatalar
    ///
    /// Genişlik sıfırsa, data genişlikle uyuşmuyorsa veya boyutlar hedef
    /// mimarinin güvenle temsil edebileceği aralığı aşıyorsa hata döndürür.
    pub fn new<T>(bits: T, width: usize) -> Result<Self, BitmapConversionError>
    where
        T: IntoIterator<Item = B>,
    {
        let bits = Vec::from_iter(bits);
        if width == 0 {
            return Err(BitmapConversionError::ZeroWidth);
        }
        if !bits.len().is_multiple_of(width) {
            return Err(BitmapConversionError::DataSize);
        }
        let height = bits.len() / width;
        let graph_width = width
            .checked_add(1)
            .ok_or(BitmapConversionError::ArithmeticOverflow)?;
        let graph_height = height
            .checked_add(1)
            .ok_or(BitmapConversionError::ArithmeticOverflow)?;
        graph_width
            .checked_mul(graph_height)
            .ok_or(BitmapConversionError::ArithmeticOverflow)?;
        isize::try_from(graph_width).map_err(|_| BitmapConversionError::ArithmeticOverflow)?;
        isize::try_from(graph_height).map_err(|_| BitmapConversionError::ArithmeticOverflow)?;
        Ok(Self { width, bits })
    }

    /// Bitmap genişliğini döndürür; quiet zone dahil değildir.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Bitmap yüksekliğini döndürür; quiet zone dahil değildir.
    pub fn height(&self) -> usize {
        self.bits.len() / self.width
    }

    /// Unicode gösterimi ("ASCII art") hesaplar.
    ///
    /// Bu yalnızca demo özelliğidir. Satır yüksekliği yanlışsa veya monospaced font
    /// kullanılmıyorsa görünüm bozuk olabilir.
    pub fn unicode(&self) -> String {
        const BORDER: usize = 1;
        const INVERT: bool = false;
        const CHAR: [char; 4] = [' ', '▄', '▀', '█'];
        let height = self.height();
        let get = |i: usize, j: usize| -> usize {
            let res =
                if i < BORDER || i >= BORDER + height || j < BORDER || j >= BORDER + self.width {
                    B::LOW
                } else if i - BORDER < height && j - BORDER < self.width {
                    let index = (i - BORDER)
                        .checked_mul(self.width)
                        .and_then(|row| row.checked_add(j - BORDER));
                    index
                        .and_then(|index| self.bits.get(index).copied())
                        .unwrap_or(B::LOW)
                } else {
                    B::LOW
                };
            if res == B::HIGH { 1 } else { 0 }
        };
        let mut out =
            String::with_capacity((height + 2 * BORDER) * (self.width + 1 + 2 * BORDER) * 3 / 2);
        for i in (0..height + 2 * BORDER).step_by(2) {
            for j in 0..(self.width + 2 * BORDER) {
                let idx = (get(i, j) << 1) | get(i + 1, j);
                let char_index = if INVERT { (!idx) & 0b11 } else { idx };
                out.push(CHAR.get(char_index).copied().unwrap_or(' '));
            }
            out.push('\n');
        }
        out
    }

    /// "Siyah" piksellerin `(x, y)` koordinatları üzerinde iterator döndürür.
    ///
    /// Siyah piksel, Data Matrix'i oluşturan küçük siyah karelerden biridir.
    /// Hedefe göre böyle bir piksel birden fazla görsel pikseliyle veya Data Matrix'i
    /// görselleştirmek için kullanılan başka bir öğeyle render edilebilir.
    ///
    /// Koordinat sistemi sol üst köşedeki `(0, 0)` noktasından başlar; x ekseni
    /// yatay, y ekseni dikeydir. Pikseller y'den önce x artırılarak sıralanır.
    ///
    /// Quiet zone koordinatlara dahil değildir ancak rendering sırasında eklenmelidir.
    /// Data Matrix çevresindeki asgari boşluk, bir "siyah" pikselin genişliği ve
    /// yüksekliği kadar olmalıdır. Quiet zone arka plan rengini kullanmalıdır.
    ///
    /// Data Matrix açık arka plan üzerinde koyu renkle veya bunun tersiyle render
    /// edilebilir. Contrast, size ve diğer ayrıntılar specification içinde anılan
    /// referans standartlarda bulunabilir.
    ///
    /// # Örnek
    ///
    /// ```rust
    /// # use datamatrix::{DataMatrix, SymbolSize};
    /// let code = DataMatrix::encode(b"Foo", SymbolSize::Square10)?;
    /// for (x, y) in code.bitmap().pixels() {
    ///     // Data Matrix'i render etmek için (x, y) konumuna kare veya daire yerleştirin.
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn pixels(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        let w = self.width();
        self.bits
            .iter()
            .enumerate()
            .filter(|(_i, b)| **b == B::HIGH)
            .map(move |(i, _b)| (i % w, i / w))
    }

    #[doc(hidden)]
    pub fn bits(&self) -> &[B] {
        &self.bits
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    impl super::Bit for (u16, u8) {
        const LOW: Self = (0, 0);
        const HIGH: Self = (0, 1);
    }

    pub fn log(symbol_size: super::SymbolSize) -> Vec<(u16, u8)> {
        let mut matrix = super::MatrixMap::<(u16, u8)>::new(symbol_size);
        matrix.traverse_mut(|codeword, bits| {
            for (index, bit) in bits.into_iter().enumerate() {
                *bit = ((codeword + 1) as u16, (index + 1) as u8);
            }
        });
        matrix.write_padding();
        matrix.entries
    }
}

#[test]
fn test_12x12() {
    let log = tests::log(SymbolSize::Square12);
    #[rustfmt::skip]
    let should = [
        (2,1), (2,2), (3,6), (3,7), (3,8), (4,3), (4,4), (4,5), (1,1), (1,2),
        (2,3), (2,4), (2,5), (5,1), (5,2), (4,6), (4,7), (4,8), (1,3), (1,4),
        (2,6), (2,7), (2,8), (5,3), (5,4), (5,5), (10,1), (10,2), (1,6), (1,7),
        (1,5), (6,1), (6,2), (5,6), (5,7), (5,8), (10,3), (10,4), (10,5), (7,1),
        (1,8), (6,3), (6,4), (6,5), (9,1), (9,2), (10,6), (10,7), (10,8), (7,3),
        (7,2), (6,6), (6,7), (6,8), (9,3), (9,4), (9,5), (11,1), (11,2), (7,6),
        (7,4), (7,5), (8,1), (8,2), (9,6), (9,7), (9,8), (11,3), (11,4), (11,5),
        (7,7), (7,8), (8,3), (8,4), (8,5), (12,1), (12,2), (11,6), (11,7), (11,8),
        (3,1), (3,2), (8,6), (8,7), (8,8), (12,3), (12,4), (12,5), (0,1), (0,0),
        (3,3), (3,4), (3,5), (4,1), (4,2), (12,6), (12,7), (12,8), (0,0), (0,1)
    ];
    assert_eq!(&log, &should);
}

#[test]
fn test_10x10() {
    let log = tests::log(SymbolSize::Square10);
    #[rustfmt::skip]
    let should = [
        (2,1), (2,2), (3,6), (3,7), (3,8), (4,3), (4,4), (4,5),
        (2,3), (2,4), (2,5), (5,1), (5,2), (4,6), (4,7), (4,8),
        (2,6), (2,7), (2,8), (5,3), (5,4), (5,5), (1,1), (1,2),
        (1,5), (6,1), (6,2), (5,6), (5,7), (5,8), (1,3), (1,4),
        (1,8), (6,3), (6,4), (6,5), (8,1), (8,2), (1,6), (1,7),
        (7,2), (6,6), (6,7), (6,8), (8,3), (8,4), (8,5), (7,1),
        (7,4), (7,5), (3,1), (3,2), (8,6), (8,7), (8,8), (7,3),
        (7,7), (7,8), (3,3), (3,4), (3,5), (4,1), (4,2), (7,6),
    ];
    assert_eq!(&log, &should);
}

#[test]
fn test_8x32() {
    let log = tests::log(SymbolSize::Rect8x32);
    #[rustfmt::skip]
    let should = [
        (2,1), (2,2), (3,6), (3,7), (3,8), (4,3), (4,4), (4,5), (8,1), (8,2), (9,6), (9,7), (9,8), (10,3), (10,4), (10,5), (14,1), (14,2), (15,6), (15,7), (15,8), (16,3), (16,4), (16,5), (20,1), (20,2), (1,4), (1,5),
        (2,3), (2,4), (2,5), (5,1), (5,2), (4,6), (4,7), (4,8), (8,3), (8,4), (8,5), (11,1), (11,2), (10,6), (10,7), (10,8), (14,3), (14,4), (14,5), (17,1), (17,2), (16,6), (16,7), (16,8), (20,3), (20,4), (20,5), (1,6),
        (2,6), (2,7), (2,8), (5,3), (5,4), (5,5), (7,1), (7,2), (8,6), (8,7), (8,8), (11,3), (11,4), (11,5), (13,1), (13,2), (14,6), (14,7), (14,8), (17,3), (17,4), (17,5), (19,1), (19,2), (20,6), (20,7), (20,8), (1,7),
        (1,1), (6,1), (6,2), (5,6), (5,7), (5,8), (7,3), (7,4), (7,5), (12,1), (12,2), (11,6), (11,7), (11,8), (13,3), (13,4), (13,5), (18,1), (18,2), (17,6), (17,7), (17,8), (19,3), (19,4), (19,5), (21,1), (21,2), (1,8),
        (1,2), (6,3), (6,4), (6,5), (3,1), (3,2), (7,6), (7,7), (7,8), (12,3), (12,4), (12,5), (9,1), (9,2), (13,6), (13,7), (13,8), (18,3), (18,4), (18,5), (15,1), (15,2), (19,6), (19,7), (19,8), (21,3), (21,4), (21,5),
        (1,3), (6,6), (6,7), (6,8), (3,3), (3,4), (3,5), (4,1), (4,2), (12,6), (12,7), (12,8), (9,3), (9,4), (9,5), (10,1), (10,2), (18,6), (18,7), (18,8), (15,3), (15,4), (15,5), (16,1), (16,2), (21,6), (21,7), (21,8),
    ];
    assert_eq!(&log, &should);
}

#[test]
fn test_from_bits_all() -> Result<(), BitmapConversionError> {
    let mut random_map = crate::test::random_maps();
    for size in SymbolList::all() {
        let map = random_map(size);
        let bitmap = map.bitmap();
        let (map2, _size) = MatrixMap::try_from_bits(&bitmap.bits, bitmap.width)?;
        assert_eq!(map.entries, map2.entries);
    }
    Ok(())
}

#[test]
fn test_bitmap_new() -> Result<(), BitmapConversionError> {
    Bitmap::new(vec![true, false], 2)?;
    Bitmap::new([true, false], 2)?;
    let data = &[true, false];
    Bitmap::new(data.iter().cloned(), 2)?;
    Ok(())
}

#[test]
fn test_bitmap_new_rejects_invalid_boundaries() {
    assert_eq!(
        Bitmap::<bool>::new([], 0).err(),
        Some(BitmapConversionError::ZeroWidth)
    );
    assert_eq!(
        Bitmap::<bool>::new([true, false, true], 2).err(),
        Some(BitmapConversionError::DataSize)
    );
    assert_eq!(
        Bitmap::<bool>::new([], usize::MAX).err(),
        Some(BitmapConversionError::ArithmeticOverflow)
    );
}

#[test]
fn test_codeword_count_is_validated_at_boundary() {
    assert_eq!(
        MatrixMap::new_with_codewords(&[], SymbolSize::Square10).err(),
        Some(BitmapConversionError::CodewordCount {
            expected: SymbolSize::Square10.num_codewords(),
            actual: 0,
        })
    );
}
