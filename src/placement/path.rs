use alloc::{vec, vec::Vec};
use core::cell::Cell;

#[cfg(test)]
use pretty_assertions::assert_eq;

use super::Bitmap;
#[cfg(test)]
use super::BitmapConversionError;
type N = isize;

/// Vector grafik path segment'i.
///
/// [Bitmap yapısının](Bitmap) [path() fonksiyonunda](Bitmap::path) kullanılır.
/// Ayrıntılar için ilgili dokümantasyona bakın.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathSegment {
    /// Çizim yapmadan relative hareketi temsil eder.
    ///
    /// İlk öğe relative x mesafesi `dx` (yatay mesafe), ikinci öğe relative dikey
    /// mesafe `dy` değeridir. Bu segment yeni bir subpath başlatır.
    ///
    /// SVG path içindeki `m` öğesine benzer ancak orada `dx` ve `dy` sırası terstir.
    ///
    /// [path()](Bitmap::path) tarafından döndürülen path segment listesi bununla
    /// başlamaz; ilk path'in örtük olarak başladığı varsayılır.
    Move(isize, isize),
    /// Relative mesafeli yatay çizim.
    ///
    /// SVG path içindeki `h` öğesine benzer.
    Horizontal(isize),
    /// Relative mesafeli dikey çizim.
    ///
    /// SVG path içindeki `v` öğesine benzer.
    Vertical(isize),
    /// Geçerli subpath'i kapatır. Birden fazla kez oluşabilir.
    ///
    /// SVG path içindeki `z` öğesine benzer.
    Close,
}

#[derive(Debug)]
enum MicroStep {
    Jump((N, N)),
    Step((N, N)),
}

impl Bitmap<bool> {
    /// Bu bitmap için vector path çizim komutlarını döndürür.
    ///
    /// Relative çizim, hareket ve kapatma komutlarından oluşan bir dizi hesaplar.
    /// Sonuç path doğru doldurulduğunda bitmap'i gösterir; aşağıya bakın.
    ///
    /// Koordinat sistemi [pixels()](Self::pixels) fonksiyonuyla aynıdır. Yalnızca
    /// relative koordinatlar döndürüldüğünden başlangıç konumu gerekmez.
    ///
    /// # Doldurma kuralı
    ///
    /// Vector grafiklerde bilinen even-odd doldurma kuralı kullanılmalıdır. SVG ve
    /// PDF dahil birçok vector grafik formatı bunu destekler.
    ///
    /// # Örnek
    ///
    /// `examples/` dizini bu yardımcıyı kullanan SVG, EPS ve PDF kod örnekleri içerir.
    ///
    /// # Implementasyon
    ///
    /// Dış hat bir graph olarak modellenir ve ardından Eulerian circuit'lere ayrılır.
    pub fn path(&self) -> Vec<PathSegment> {
        let mut graph = bits_to_edge_graph(&self.bits, self.width(), self.height());
        let mut pos = if let Some(pos) = graph.edge_left() {
            pos
        } else {
            return vec![];
        };
        let mut elements = Vec::new();

        let mut alternatives = Vec::new();
        let mut insert: usize = 0;
        // Graph içindeki, genellikle birden fazla parçadan oluşan Eulerian walk'ları dolaşır.
        loop {
            // Hierholzer algoritmasıyla bir Eulerian tour tamamlar.
            'euler: loop {
                let mut local_loop = Vec::new();
                let insert_pos = insert;

                if !graph.remove_edge(&pos) {
                    crate::invariant_violation("Eulerian path başlangıç kenarı bulunamadı");
                }
                let start = pos.start_node();
                local_loop.push(MicroStep::Step(pos.end_node()));
                let Some(next_insert) = insert.checked_add(1) else {
                    crate::invariant_violation("Eulerian path uzunluğu hesaplanırken taşma oluştu");
                };
                insert = next_insert;

                // Başlangıç node'u yeniden bulunana kadar ilerler.
                loop {
                    let (new_pos, had_alternatives) = graph.follow(&pos);
                    if had_alternatives {
                        alternatives.push((insert, pos));
                    }
                    let Some(new_pos) = new_pos else {
                        crate::invariant_violation(
                            "Eulerian path geçerli bir sonraki kenar bulamadı",
                        );
                    };
                    pos = new_pos;
                    if !graph.remove_edge(&pos) {
                        crate::invariant_violation("Eulerian path üzerindeki kenar kaldırılamadı");
                    }
                    let end = pos.end_node();
                    local_loop.push(MicroStep::Step(end));
                    if end == start {
                        break;
                    }
                    let Some(next_insert) = insert.checked_add(1) else {
                        crate::invariant_violation(
                            "Eulerian path uzunluğu hesaplanırken taşma oluştu",
                        );
                    };
                    insert = next_insert;
                }
                if insert_pos > elements.len() {
                    crate::invariant_violation(
                        "Eulerian path ekleme konumu çıktı sınırlarının dışında",
                    );
                }
                elements.splice(insert_pos..insert_pos, local_loop.drain(..));

                // Bu Eulerian walk için kalan edge var mı?
                for (idx, pos_alt) in alternatives.drain(..) {
                    if let Some(new_pos) = graph.can_step(&pos_alt) {
                        pos = new_pos;
                        insert = idx;
                        continue 'euler;
                    }
                }
                break;
            }

            // Graph içinde edge kaldıysa yeni bir Eulerian tour başlatır.
            if let Some(new_pos) = graph.edge_left() {
                elements.push(MicroStep::Jump(new_pos.start_node()));
                pos = new_pos;
                insert = elements.len();
                continue;
            }
            break;
        }
        compress_path(elements.into_iter())
    }
}

fn compress_path(micro_steps: impl Iterator<Item = MicroStep>) -> Vec<PathSegment> {
    let mut steps = Vec::new();
    let mut pos = (0, 0);

    // İşlenmekte olan step
    let mut step_wip = None;
    for micro_step in micro_steps {
        match micro_step {
            MicroStep::Step((i, j)) => {
                match step_wip {
                    // Step'in step_wip ile birleştirilip birleştirilemeyeceğini denetler.
                    Some(PathSegment::Horizontal(m)) if i == pos.0 => {
                        let distance = checked_sub(j, pos.1);
                        step_wip = Some(PathSegment::Horizontal(checked_add(m, distance)));
                    }
                    Some(PathSegment::Vertical(m)) if j == pos.1 => {
                        let distance = checked_sub(i, pos.0);
                        step_wip = Some(PathSegment::Vertical(checked_add(m, distance)));
                    }
                    // Yeni step_wip başlatır.
                    mut other => {
                        if let Some(other) = other.take() {
                            steps.push(other);
                        }
                        if i == pos.0 {
                            step_wip = Some(PathSegment::Horizontal(checked_sub(j, pos.1)));
                        } else {
                            step_wip = Some(PathSegment::Vertical(checked_sub(i, pos.0)));
                        }
                    }
                }
                pos = (i, j);
            }
            MicroStep::Jump((i, j)) => {
                // step_wip içeriğini bırakır ve yalnızca close ekler.
                step_wip = None;
                steps.push(PathSegment::Close);
                steps.push(PathSegment::Move(
                    checked_sub(j, pos.1),
                    checked_sub(i, pos.0),
                ));
                pos = (i, j);
            }
        }
    }
    steps.push(PathSegment::Close);
    steps
}

fn checked_add(left: N, right: N) -> N {
    let Some(value) = left.checked_add(right) else {
        crate::invariant_violation("path koordinatı toplanırken taşma oluştu");
    };
    value
}

fn checked_sub(left: N, right: N) -> N {
    let Some(value) = left.checked_sub(right) else {
        crate::invariant_violation("path koordinatı çıkarılırken taşma oluştu");
    };
    value
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Direction {
    Up,
    Down,
    Right,
    Left,
}

impl Direction {
    fn flip(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
            Self::Right => Self::Left,
            Self::Left => Self::Right,
        }
    }
}

/// Graph içindeki bir edge üzerinde yönlendirilmiş konum.
#[derive(Debug, Clone, PartialEq)]
struct Position {
    i: N,
    j: N,
    dir: Direction,
}

impl Position {
    /// Konumun işaret ettiği node'un koordinatını döndürür.
    fn end_node(&self) -> (N, N) {
        let i = self.i;
        let j = self.j;
        match self.dir {
            Direction::Up | Direction::Left => (i, j),
            Direction::Down => (i + 1, j),
            Direction::Right => (i, j + 1),
        }
    }

    /// Konumun geldiği node'un koordinatını döndürür.
    fn start_node(&self) -> (N, N) {
        self.flip().end_node()
    }

    fn flip(&self) -> Position {
        Position {
            dir: self.dir.flip(),
            ..self.clone()
        }
    }

    fn straight(&self) -> Self {
        let (i, j) = match self.dir {
            Direction::Up => (self.i - 1, self.j),
            Direction::Down => (self.i + 1, self.j),
            Direction::Right => (self.i, self.j + 1),
            Direction::Left => (self.i, self.j - 1),
        };
        Self {
            i,
            j,
            dir: self.dir,
        }
    }

    fn left(&self) -> Self {
        let (i, j, dir) = match self.dir {
            Direction::Up => (self.i, self.j - 1, Direction::Left),
            Direction::Down => (self.i + 1, self.j, Direction::Right),
            Direction::Right => (self.i - 1, self.j + 1, Direction::Up),
            Direction::Left => (self.i, self.j, Direction::Down),
        };
        Self { i, j, dir }
    }

    fn right(&self) -> Self {
        let (i, j, dir) = match self.dir {
            Direction::Up => (self.i, self.j, Direction::Right),
            Direction::Down => (self.i + 1, self.j - 1, Direction::Left),
            Direction::Right => (self.i, self.j + 1, Direction::Down),
            Direction::Left => (self.i - 1, self.j, Direction::Up),
        };
        Self { i, j, dir }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
struct Edge {
    left: bool,
    top: bool,
}

#[derive(Debug, PartialEq)]
struct Graph {
    edges: Vec<Edge>,
    width: usize,
    height: usize,
    edge_hint: Cell<usize>,
}

impl Graph {
    /// `(i, j)` hücresinin sol edge'inin graph içinde olup olmadığını denetler.
    fn left(&self, i: N, j: N) -> bool {
        self.edge(i, j).is_some_and(|edge| edge.left)
    }

    /// `(i, j)` hücresinin üst edge'inin graph içinde olup olmadığını denetler.
    fn top(&self, i: N, j: N) -> bool {
        self.edge(i, j).is_some_and(|edge| edge.top)
    }

    fn has_cell(&self, i: N, j: N) -> bool {
        if i < 0 || j < 0 {
            return false;
        }
        let Ok(i) = usize::try_from(i) else {
            return false;
        };
        let Ok(j) = usize::try_from(j) else {
            return false;
        };
        i <= self.height && j <= self.width
    }

    fn edge_index(&self, i: N, j: N) -> Option<usize> {
        if !self.has_cell(i, j) {
            return None;
        }
        let row = usize::try_from(i).ok()?;
        let column = usize::try_from(j).ok()?;
        let stride = self.width.checked_add(1)?;
        row.checked_mul(stride)?.checked_add(column)
    }

    fn edge(&self, i: N, j: N) -> Option<&Edge> {
        self.edge_index(i, j)
            .and_then(|index| self.edges.get(index))
    }

    /// Konumdan erişilebilecek bir graph edge'i olup olmadığını denetler.
    ///
    /// Ayrıca [Self::follow()] yöntemine bakın.
    fn can_step(&self, pos: &Position) -> Option<Position> {
        None.or_else(|| Some(pos.straight()).filter(|p| self.has_edge(p)))
            .or_else(|| Some(pos.left()).filter(|p| self.has_edge(p)))
            .or_else(|| Some(pos.right()).filter(|p| self.has_edge(p)))
    }

    /// Verilen konumdan erişilebilen yeni bir konum döndürür.
    ///
    /// Yeni konumu ve birden fazla seçenek olup olmadığını belirten boolean değeri döndürür.
    fn follow(&self, pos: &Position) -> (Option<Position>, bool) {
        let mut found = None;
        let mut alternatives = false;

        macro_rules! try_step {
            ($pos:expr) => {
                if let Some(pos) = Some($pos).filter(|p| self.has_edge(p)) {
                    if found.is_none() {
                        found = Some(pos);
                    } else {
                        alternatives = true;
                    }
                }
            };
        }

        try_step!(pos.straight());
        try_step!(pos.left());
        try_step!(pos.right());

        (found, alternatives)
    }

    fn has_edge(&self, pos: &Position) -> bool {
        match pos.dir {
            Direction::Left | Direction::Right => self.top(pos.i, pos.j),
            Direction::Up | Direction::Down => self.left(pos.i, pos.j),
        }
    }

    fn remove_top(&mut self, i: N, j: N) -> bool {
        let Some(index) = self.edge_index(i, j) else {
            return false;
        };
        self.edges
            .get_mut(index)
            .is_some_and(|edge| core::mem::replace(&mut edge.top, false))
    }

    fn remove_left(&mut self, i: N, j: N) -> bool {
        let Some(index) = self.edge_index(i, j) else {
            return false;
        };
        self.edges
            .get_mut(index)
            .is_some_and(|edge| core::mem::replace(&mut edge.left, false))
    }

    fn remove_edge(&mut self, pos: &Position) -> bool {
        match pos.dir {
            Direction::Left | Direction::Right => self.remove_top(pos.i, pos.j),
            Direction::Up | Direction::Down => self.remove_left(pos.i, pos.j),
        }
    }

    /// Graph için kalan bir konum bulur.
    ///
    /// Edge kalmadıysa `None` döndürür.
    fn edge_left(&self) -> Option<Position> {
        let hint = self.edge_hint.get();
        let remaining = self.edges.get(hint..)?;
        let stride = self.width.checked_add(1)?;
        for (idx, edge) in remaining.iter().enumerate() {
            if edge.left || edge.top {
                let idx = idx.checked_add(hint)?;
                let i = N::try_from(idx / stride).ok()?;
                let j = N::try_from(idx % stride).ok()?;
                self.edge_hint.set(idx);
                return Some(Position {
                    i,
                    j,
                    dir: if edge.top {
                        Direction::Right
                    } else {
                        Direction::Up
                    },
                });
            }
        }
        self.edge_hint.set(self.edges.len());
        None
    }
}

fn bits_to_edge_graph(bits: &[bool], width: usize, height: usize) -> Graph {
    let Some(expected_bits) = width.checked_mul(height) else {
        crate::invariant_violation("bitmap alanı hesaplanırken taşma oluştu");
    };
    if bits.len() != expected_bits {
        crate::invariant_violation("bitmap data uzunluğu boyutlarıyla uyuşmuyor");
    }
    let Some(graph_width) = width.checked_add(1) else {
        crate::invariant_violation("path graph genişliği hesaplanırken taşma oluştu");
    };
    let Some(graph_height) = height.checked_add(1) else {
        crate::invariant_violation("path graph yüksekliği hesaplanırken taşma oluştu");
    };
    if N::try_from(graph_width).is_err() || N::try_from(graph_height).is_err() {
        crate::invariant_violation("path graph boyutu isize sınırını aştı");
    }
    let Some(edge_count) = graph_width.checked_mul(graph_height) else {
        crate::invariant_violation("path graph alanı hesaplanırken taşma oluştu");
    };
    let mut graph = Graph {
        edges: vec![
            Edge {
                left: false,
                top: false
            };
            edge_count
        ],
        edge_hint: Cell::new(0),
        width,
        height,
    };

    let mut edge_hint = None;

    for i in 0..height {
        for j in 0..width {
            let Some(idx) = i.checked_mul(width).and_then(|row| row.checked_add(j)) else {
                crate::invariant_violation("bitmap piksel konumu hesaplanırken taşma oluştu");
            };
            if bits.get(idx).copied() != Some(true) {
                continue;
            }
            let Some(cell) = i
                .checked_mul(graph_width)
                .and_then(|row| row.checked_add(j))
            else {
                crate::invariant_violation("path graph hücresi hesaplanırken taşma oluştu");
            };
            edge_hint.get_or_insert(cell);
            if j == 0 || bits.get(idx - 1).copied() == Some(false) {
                // Sol
                set_edge(&mut graph.edges, cell, EdgeSide::Left);
            }
            if i == 0 || bits.get(idx - width).copied() == Some(false) {
                // Üst
                set_edge(&mut graph.edges, cell, EdgeSide::Top);
            }
            if j == width - 1 || bits.get(idx + 1).copied() == Some(false) {
                // Sağ
                let Some(right) = cell.checked_add(1) else {
                    crate::invariant_violation("sağ path kenarı hesaplanırken taşma oluştu");
                };
                set_edge(&mut graph.edges, right, EdgeSide::Left);
            }
            if i == height - 1 || bits.get(idx + width).copied() == Some(false) {
                // Alt
                let Some(bottom) = cell.checked_add(graph_width) else {
                    crate::invariant_violation("alt path kenarı hesaplanırken taşma oluştu");
                };
                set_edge(&mut graph.edges, bottom, EdgeSide::Top);
            }
        }
    }
    graph.edge_hint.set(edge_hint.unwrap_or(graph.edges.len()));
    graph
}

enum EdgeSide {
    Left,
    Top,
}

fn set_edge(edges: &mut [Edge], index: usize, side: EdgeSide) {
    let Some(edge) = edges.get_mut(index) else {
        crate::invariant_violation("path kenarı graph sınırlarının dışında");
    };
    match side {
        EdgeSide::Left => edge.left = true,
        EdgeSide::Top => edge.top = true,
    }
}

#[test]
fn mini_2x2_one_euler() {
    let bm = Bitmap {
        bits: vec![true, false, true, true],
        width: 2,
    };
    assert_eq!(
        bits_to_edge_graph(&bm.bits, 2, 2),
        Graph {
            edges: vec![
                Edge {
                    left: true,
                    top: true
                },
                Edge {
                    left: true,
                    top: false
                },
                Edge {
                    left: false,
                    top: false
                },
                Edge {
                    left: true,
                    top: false
                },
                Edge {
                    left: false,
                    top: true
                },
                Edge {
                    left: true,
                    top: false
                },
                Edge {
                    left: false,
                    top: true
                },
                Edge {
                    left: false,
                    top: true
                },
                Edge {
                    left: false,
                    top: false
                },
            ],
            edge_hint: Cell::new(0),
            width: 2,
            height: 2,
        }
    );
    assert_eq!(
        bm.path(),
        vec![
            PathSegment::Horizontal(1),
            PathSegment::Vertical(1),
            PathSegment::Horizontal(1),
            PathSegment::Vertical(1),
            PathSegment::Horizontal(-2),
            PathSegment::Close,
        ],
    );
}

#[test]
fn mini_2x3_one_euler() {
    let bm = Bitmap {
        bits: vec![true, false, true, true, true, false],
        width: 3,
    };
    assert_eq!(
        bm.path(),
        vec![
            PathSegment::Horizontal(1),
            PathSegment::Vertical(1),
            PathSegment::Horizontal(2),
            PathSegment::Vertical(-1),
            PathSegment::Horizontal(-1),
            PathSegment::Vertical(2),
            PathSegment::Horizontal(-2),
            PathSegment::Close,
        ],
    );
}

#[test]
fn mini_3x2_two_euler() {
    let bm = Bitmap {
        bits: vec![true, true, false, false, false, true],
        width: 2,
    };
    assert_eq!(
        bm.path(),
        vec![
            PathSegment::Horizontal(2),
            PathSegment::Vertical(1),
            PathSegment::Horizontal(-2),
            PathSegment::Close,
            PathSegment::Move(1, 2),
            PathSegment::Horizontal(1),
            PathSegment::Vertical(1),
            PathSegment::Horizontal(-1),
            PathSegment::Close,
        ],
    );
}

#[test]
fn empty() -> Result<(), BitmapConversionError> {
    let bm = Bitmap::new(vec![false; 6], 2)?;
    assert_eq!(bm.path(), vec![]);
    Ok(())
}

#[test]
fn edge_hint() -> Result<(), BitmapConversionError> {
    let bm = Bitmap {
        bits: vec![false, false, true, true, true, true],
        width: 3,
    };
    let graph = bits_to_edge_graph(&bm.bits, bm.width(), bm.height());
    assert_eq!(graph.edge_hint.get(), 2);
    Ok(())
}
