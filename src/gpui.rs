//! Yerel GPUI snapshot'ı için Data Matrix rendering desteği.
//!
//! Bu modül yalnızca `gpui` feature etkinleştirildiğinde derlenir ve
//! `../gpui/crates/gpui` içindeki API'yi hedefler. Data Matrix encoding ile
//! bitmap hazırlığı render döngüsünün dışında yapılmalıdır.
//!
//! # Örnek
//!
//! ```rust
//! use datamatrix::{
//!     DataMatrix, SymbolList,
//!     gpui::{DataMatrixElement, PreparedDataMatrix},
//! };
//!
//! let code = DataMatrix::encode(b"gpui", SymbolList::default())?;
//! let prepared = PreparedDataMatrix::new(&code);
//! let element = DataMatrixElement::new("giris-data-matrix", prepared);
//! # let _ = element;
//! # Ok::<(), datamatrix::data::DataEncodingError>(())
//! ```

use alloc::{sync::Arc, vec::Vec};
use core::num::{NonZeroU8, NonZeroU16};

use ::gpui::{
    App, Bounds, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Point, RenderOnce, Rgba, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, canvas, div, fill, point, px, rgb, size,
};

use crate::{
    DataMatrix,
    placement::{Bitmap, ModuleRun},
};

const MAX_EXACT_F32_INTEGER: usize = 1 << 24;

/// Bir bitmap GPUI rendering için hazırlanırken oluşan hata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparationError {
    /// Bitmap hiçbir satır içermiyor.
    EmptyBitmap,
    /// Bitmap boyutlarından en az biri GPUI koordinatlarında kesin olarak
    /// gösterilebilecek sınırı aşıyor.
    DimensionsTooLarge {
        /// Verilen bitmap genişliği.
        width: usize,
        /// Verilen bitmap yüksekliği.
        height: usize,
        /// Desteklenen azami genişlik veya yükseklik.
        maximum: usize,
    },
}

impl core::fmt::Display for PreparationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyBitmap => f.write_str("GPUI için hazırlanacak bitmap boş olamaz"),
            Self::DimensionsTooLarge {
                width,
                height,
                maximum,
            } => write!(
                f,
                "bitmap GPUI koordinat sınırını aşıyor: {width}×{height}; azami {maximum}×{maximum} destekleniyor"
            ),
        }
    }
}

impl core::error::Error for PreparationError {}

/// GPUI render döngüsünde ucuz biçimde paylaşılabilen hazırlanmış Data Matrix.
///
/// Bitmap ve yatay HIGH module run'ları yalnızca bir kez hazırlanır. Bu yapı
/// `Clone` edildiğinde bitmap verisi kopyalanmaz.
#[derive(Clone, Debug)]
pub struct PreparedDataMatrix {
    bitmap: Arc<Bitmap<bool>>,
    dark_runs: Arc<[ModuleRun]>,
}

impl PreparedDataMatrix {
    /// Encode edilmiş bir Data Matrix'i GPUI rendering için hazırlar.
    pub fn new(data_matrix: &DataMatrix) -> Self {
        Self::from_valid_bitmap(data_matrix.bitmap())
    }

    /// Genel bir bitmap'i GPUI rendering için hazırlamayı dener.
    ///
    /// Bitmap boşsa veya koordinatları `f32` içinde kesin gösterilemeyecek kadar
    /// büyükse hata döndürür.
    pub fn try_from_bitmap(bitmap: Bitmap<bool>) -> Result<Self, PreparationError> {
        let (width, height) = bitmap.dimensions();
        if height == 0 {
            return Err(PreparationError::EmptyBitmap);
        }
        if width > MAX_EXACT_F32_INTEGER || height > MAX_EXACT_F32_INTEGER {
            return Err(PreparationError::DimensionsTooLarge {
                width,
                height,
                maximum: MAX_EXACT_F32_INTEGER,
            });
        }
        Ok(Self::from_valid_bitmap(bitmap))
    }

    fn from_valid_bitmap(bitmap: Bitmap<bool>) -> Self {
        let dark_runs: Vec<_> = bitmap.dark_runs().collect();
        Self {
            bitmap: Arc::new(bitmap),
            dark_runs: Arc::from(dark_runs),
        }
    }

    /// Hazırlanan bitmap'i döndürür.
    pub fn bitmap(&self) -> &Bitmap<bool> {
        &self.bitmap
    }

    /// Bitmap'in `(width, height)` değerlerini döndürür; quiet zone dahil değildir.
    pub fn dimensions(&self) -> (usize, usize) {
        self.bitmap.dimensions()
    }

    /// Önceden hesaplanmış yatay HIGH module run'larını döndürür.
    pub fn dark_runs(&self) -> &[ModuleRun] {
        &self.dark_runs
    }
}

impl From<&DataMatrix> for PreparedDataMatrix {
    fn from(value: &DataMatrix) -> Self {
        Self::new(value)
    }
}

impl From<DataMatrix> for PreparedDataMatrix {
    fn from(value: DataMatrix) -> Self {
        Self::new(&value)
    }
}

impl TryFrom<Bitmap<bool>> for PreparedDataMatrix {
    type Error = PreparationError;

    fn try_from(value: Bitmap<bool>) -> Result<Self, Self::Error> {
        Self::try_from_bitmap(value)
    }
}

/// Quiet zone module sayısı sıfır olduğunda döndürülen hata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuietZoneError {
    supplied: u8,
}

impl QuietZoneError {
    /// Geçersiz olarak verilen module sayısını döndürür.
    pub const fn supplied(self) -> u8 {
        self.supplied
    }
}

impl core::fmt::Display for QuietZoneError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "quiet zone en az bir module olmalıdır; {} verildi",
            self.supplied
        )
    }
}

impl core::error::Error for QuietZoneError {}

/// Data Matrix çevresindeki quiet zone genişliği.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct QuietZone(NonZeroU8);

impl QuietZone {
    /// Specification tarafından istenen asgari quiet zone.
    pub const MINIMUM: Self = Self(NonZeroU8::MIN);

    /// Verilen module sayısıyla quiet zone oluşturur.
    pub const fn new(modules: u8) -> Result<Self, QuietZoneError> {
        let Some(modules) = NonZeroU8::new(modules) else {
            return Err(QuietZoneError { supplied: 0 });
        };
        Ok(Self(modules))
    }

    /// Quiet zone genişliğini module sayısı olarak döndürür.
    pub const fn modules(self) -> u8 {
        self.0.get()
    }
}

impl Default for QuietZone {
    fn default() -> Self {
        Self::MINIMUM
    }
}

/// GPUI element ağacında Data Matrix render eden stateless bileşen.
///
/// Bileşen available bounds içine en büyük tam fiziksel-piksel module boyutuyla
/// yerleşir, en-boy oranını korur ve ortalanır. Bounds, istenen asgari module
/// boyutuna yetmiyorsa taranamayacak bulanık bir çıktı üretmek yerine hiçbir şey
/// çizmez.
pub struct DataMatrixElement {
    id: ElementId,
    prepared: PreparedDataMatrix,
    foreground: Rgba,
    background: Rgba,
    quiet_zone: QuietZone,
    minimum_module_pixels: NonZeroU16,
    aria_label: SharedString,
}

impl DataMatrixElement {
    /// Hazırlanmış Data Matrix'ten yeni bir GPUI bileşeni oluşturur.
    pub fn new(id: impl Into<ElementId>, prepared: PreparedDataMatrix) -> Self {
        Self {
            id: id.into(),
            prepared,
            foreground: rgb(0x000000),
            background: rgb(0xffffff),
            quiet_zone: QuietZone::default(),
            minimum_module_pixels: NonZeroU16::MIN,
            aria_label: "Data Matrix kodu".into(),
        }
    }

    /// Encode edilmiş Data Matrix'i hazırlayıp yeni bir GPUI bileşeni oluşturur.
    ///
    /// Bu fonksiyon bitmap hazırladığı için her frame çağrılmamalıdır. Dinamik
    /// view'larda [PreparedDataMatrix] model durumunda saklanmalıdır.
    pub fn from_data_matrix(id: impl Into<ElementId>, data_matrix: &DataMatrix) -> Self {
        Self::new(id, PreparedDataMatrix::new(data_matrix))
    }

    /// HIGH module rengini değiştirir.
    pub fn with_foreground(mut self, foreground: Rgba) -> Self {
        self.foreground = foreground;
        self
    }

    /// Quiet zone dahil arka plan rengini değiştirir.
    pub fn with_background(mut self, background: Rgba) -> Self {
        self.background = background;
        self
    }

    /// Quiet zone genişliğini değiştirir.
    pub fn with_quiet_zone(mut self, quiet_zone: QuietZone) -> Self {
        self.quiet_zone = quiet_zone;
        self
    }

    /// Tek bir module için gereken asgari fiziksel piksel boyutunu değiştirir.
    pub fn with_minimum_module_pixels(mut self, minimum: NonZeroU16) -> Self {
        self.minimum_module_pixels = minimum;
        self
    }

    /// Erişilebilirlik etiketini değiştirir.
    ///
    /// Hassas olabilecek encoded data varsayılan etikete otomatik eklenmez.
    pub fn with_aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.aria_label = label.into();
        self
    }
}

impl IntoElement for DataMatrixElement {
    type Element = ::gpui::ViewElement<Self>;

    #[track_caller]
    fn into_element(self) -> Self::Element {
        ::gpui::ViewElement::new(self)
    }
}

impl RenderOnce for DataMatrixElement {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let dimensions = self.prepared.dimensions();
        let quiet_zone = self.quiet_zone;
        let minimum_module_pixels = self.minimum_module_pixels;
        let prepared = self.prepared;
        let foreground = self.foreground;
        let background = self.background;

        let matrix = canvas(
            move |bounds, window, _cx| {
                PaintLayout::calculate(
                    bounds,
                    dimensions,
                    quiet_zone,
                    minimum_module_pixels,
                    window.scale_factor(),
                )
                .map(|mut layout| {
                    layout.origin = window.pixel_snap_point(layout.origin);
                    layout
                })
            },
            move |_bounds, layout, window, _cx| {
                let Some(layout) = layout else {
                    return;
                };
                window.paint_quad(fill(layout.background_bounds(), background));
                for run in prepared.dark_runs().iter().copied() {
                    window.paint_quad(fill(layout.run_bounds(run), foreground));
                }
            },
        )
        .size_full();

        div()
            .id(self.id)
            .role(Role::Image)
            .aria_label(self.aria_label)
            .size_full()
            .child(matrix)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PaintLayout {
    origin: Point<Pixels>,
    module_size: Pixels,
    width_with_quiet_zone: usize,
    height_with_quiet_zone: usize,
    quiet_zone: usize,
}

impl PaintLayout {
    fn calculate(
        bounds: Bounds<Pixels>,
        (width, height): (usize, usize),
        quiet_zone: QuietZone,
        minimum_module_pixels: NonZeroU16,
        scale_factor: f32,
    ) -> Option<Self> {
        let bounds_width = f32::from(bounds.size.width);
        let bounds_height = f32::from(bounds.size.height);
        let bounds_x = f32::from(bounds.origin.x);
        let bounds_y = f32::from(bounds.origin.y);
        if !scale_factor.is_finite()
            || scale_factor <= 0.0
            || !bounds_width.is_finite()
            || !bounds_height.is_finite()
            || !bounds_x.is_finite()
            || !bounds_y.is_finite()
            || bounds_width <= 0.0
            || bounds_height <= 0.0
        {
            return None;
        }

        let quiet_zone = usize::from(quiet_zone.modules());
        let width_with_quiet_zone = width.checked_add(quiet_zone.checked_mul(2)?)?;
        let height_with_quiet_zone = height.checked_add(quiet_zone.checked_mul(2)?)?;
        let module_device_pixels = ((bounds_width * scale_factor) / width_with_quiet_zone as f32)
            .floor()
            .min(((bounds_height * scale_factor) / height_with_quiet_zone as f32).floor());
        if !module_device_pixels.is_finite()
            || module_device_pixels < f32::from(minimum_module_pixels.get())
        {
            return None;
        }

        let module_size_value = module_device_pixels / scale_factor;
        let rendered_width = module_size_value * width_with_quiet_zone as f32;
        let rendered_height = module_size_value * height_with_quiet_zone as f32;
        let origin = point(
            px(bounds_x + (bounds_width - rendered_width) / 2.0),
            px(bounds_y + (bounds_height - rendered_height) / 2.0),
        );

        Some(Self {
            origin,
            module_size: px(module_size_value),
            width_with_quiet_zone,
            height_with_quiet_zone,
            quiet_zone,
        })
    }

    fn background_bounds(self) -> Bounds<Pixels> {
        ::gpui::bounds(
            self.origin,
            size(
                self.module_size * self.width_with_quiet_zone,
                self.module_size * self.height_with_quiet_zone,
            ),
        )
    }

    fn run_bounds(self, run: ModuleRun) -> Bounds<Pixels> {
        ::gpui::bounds(
            self.origin
                + point(
                    self.module_size * (self.quiet_zone + run.x()),
                    self.module_size * (self.quiet_zone + run.y()),
                ),
            size(self.module_size * run.width(), self.module_size),
        )
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn quiet_zone_rejects_zero() {
        assert_eq!(QuietZone::new(0), Err(QuietZoneError { supplied: 0 }));
        assert_eq!(QuietZone::new(2).map(QuietZone::modules), Ok(2));
    }

    #[test]
    fn preparation_rejects_empty_bitmap() -> Result<(), crate::placement::BitmapConversionError> {
        let bitmap = Bitmap::new([], 1)?;
        assert_eq!(
            PreparedDataMatrix::try_from_bitmap(bitmap).err(),
            Some(PreparationError::EmptyBitmap)
        );
        Ok(())
    }

    #[test]
    fn preparation_caches_dark_runs() -> Result<(), crate::placement::BitmapConversionError> {
        let bitmap = Bitmap::new([false, true, true, false, true, true, true, false], 4)?;
        let Ok(prepared) = PreparedDataMatrix::try_from_bitmap(bitmap) else {
            crate::invariant_violation("geçerli test bitmap'i GPUI için hazırlanamadı");
        };
        let runs: Vec<_> = prepared
            .dark_runs()
            .iter()
            .map(|run| (run.x(), run.y(), run.width()))
            .collect();
        assert_eq!(runs, vec![(1, 0, 2), (0, 1, 3)]);
        Ok(())
    }

    #[test]
    fn layout_uses_integer_device_pixels_and_centers_symbol() {
        let layout = PaintLayout::calculate(
            ::gpui::bounds(point(px(0.0), px(0.0)), size(px(130.0), px(130.0))),
            (10, 10),
            QuietZone::MINIMUM,
            NonZeroU16::MIN,
            1.0,
        );
        let Some(layout) = layout else {
            crate::invariant_violation("geçerli GPUI test bounds değeri layout üretmedi");
        };
        assert_eq!(f32::from(layout.module_size), 10.0);
        assert_eq!(layout.origin, point(px(5.0), px(5.0)));
        assert_eq!(layout.background_bounds().size, size(px(120.0), px(120.0)));
    }

    #[test]
    fn layout_respects_fractional_display_scale() {
        let layout = PaintLayout::calculate(
            ::gpui::bounds(point(px(0.0), px(0.0)), size(px(65.0), px(65.0))),
            (10, 10),
            QuietZone::MINIMUM,
            NonZeroU16::MIN,
            2.0,
        );
        let Some(layout) = layout else {
            crate::invariant_violation("geçerli HiDPI test bounds değeri layout üretmedi");
        };
        assert_eq!(f32::from(layout.module_size), 5.0);
        assert_eq!(layout.origin, point(px(2.5), px(2.5)));
    }

    #[test]
    fn layout_rejects_insufficient_space() {
        assert_eq!(
            PaintLayout::calculate(
                ::gpui::bounds(point(px(0.0), px(0.0)), size(px(11.0), px(11.0))),
                (10, 10),
                QuietZone::MINIMUM,
                NonZeroU16::MIN,
                1.0,
            ),
            None
        );
    }

    #[test]
    fn prepared_data_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DataMatrix>();
        assert_send_sync::<Bitmap<bool>>();
        assert_send_sync::<PreparedDataMatrix>();
    }
}
