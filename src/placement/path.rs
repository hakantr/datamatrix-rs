use alloc::{vec, vec::Vec};
use core::cell::Cell;

#[cfg(test)]
use pretty_assertions::assert_eq;

use super::{Bitmap, BitmapConversionError};
type N = isize;

/// Segment of a vector graphics path.
///
/// This is used in the [path() function](Bitmap::path) of the [Bitmap struct](Bitmap).
/// See the documentation there for more details.
#[derive(Debug, PartialEq)]
pub enum PathSegment {
    /// Represents a relative move without drawing.
    ///
    /// The first entry is the relative x distance `dx` (so the horizontal distance),
    /// and the second entry is the relative vertical distance `dy`. This segment begins a new
    /// subpath.
    ///
    /// This is like a `m` entry in a SVG path, but there the order of `dx` and `dy` are
    /// switched.
    ///
    /// A list of path segments returned by [path()](Bitmap::path) does _not_
    /// start with this. The first path is assumed to start implicitly.
    Move(isize, isize),
    /// A horizontal draw, relative distance.
    ///
    /// This is like a `h` entry in a SVG path.
    Horizontal(isize),
    /// A vertical draw, relative distance.
    ///
    /// This is like a `v` entry in a SVG path.
    Vertical(isize),
    /// Close the current (sub)path. Can occur multiple times.
    ///
    /// This is like a `z` entry in a SVG path.
    Close,
}

#[derive(Debug)]
enum MicroStep {
    Jump((N, N)),
    Step((N, N)),
}

impl Bitmap<bool> {
    /// Get vector path drawing instructions for this bitmap.
    ///
    /// This function computes a sequence of relative draw, relative move, and close instructions.
    /// The resulting path shows the bitmap if filled properly (see below).
    ///
    /// The coordinate system is identical to the one of the function [pixels()](Self::pixels).
    /// The starting position is not needed in this function because only
    /// relative coordinates are returned.
    ///
    /// # Filling rule
    ///
    /// The even-odd filling rule (as known in vector graphics) must be used. It is supported
    /// by many vector graphic formats, including SVG and PDF.
    ///
    /// # Example
    ///
    /// The `examples/` directory contains a SVG, EPS and PDF code example using this
    /// helper.
    ///
    /// # Implementation
    ///
    /// The outline is modeled as a graph which is then decomposed into
    /// Eulerian circuits.
    pub fn path(&self) -> Result<Vec<PathSegment>, BitmapConversionError> {
        let mut graph = bits_to_edge_graph(&self.bits, self.width(), self.height())?;
        let mut pos = if let Some(pos) = graph.edge_left() {
            pos
        } else {
            return Ok(vec![]);
        };
        let mut elements = Vec::new();

        let mut alternatives = Vec::new();
        let mut insert: usize = 0;
        // loop over the eulerian walks in the graph (composed of multiple in general)
        loop {
            // complete an Eulerian tour, Hierholzer's algorithm
            'euler: loop {
                let mut local_loop = Vec::new();
                let insert_pos = insert;

                if !graph.remove_edge(&pos) {
                    return Err(BitmapConversionError::InternalError(
                        "Eulerian path başlangıç kenarı bulunamadı",
                    ));
                }
                let start = pos.start_node();
                local_loop.push(MicroStep::Step(pos.end_node()));
                insert = insert
                    .checked_add(1)
                    .ok_or(BitmapConversionError::ArithmeticOverflow)?;

                // walk until we find start node again
                loop {
                    let (new_pos, had_alternatives) = graph.follow(&pos);
                    if had_alternatives {
                        alternatives.push((insert, pos));
                    }
                    pos = new_pos.ok_or(BitmapConversionError::InternalError(
                        "Eulerian path geçerli bir sonraki kenar bulamadı",
                    ))?;
                    if !graph.remove_edge(&pos) {
                        return Err(BitmapConversionError::InternalError(
                            "Eulerian path üzerindeki kenar kaldırılamadı",
                        ));
                    }
                    let end = pos.end_node();
                    local_loop.push(MicroStep::Step(end));
                    if end == start {
                        break;
                    }
                    insert = insert
                        .checked_add(1)
                        .ok_or(BitmapConversionError::ArithmeticOverflow)?;
                }
                if insert_pos > elements.len() {
                    return Err(BitmapConversionError::InternalError(
                        "Eulerian path ekleme konumu çıktı sınırlarının dışında",
                    ));
                }
                elements.splice(insert_pos..insert_pos, local_loop.drain(..));

                // are there remaining edges for this euler walk?
                for (idx, pos_alt) in alternatives.drain(..) {
                    if let Some(new_pos) = graph.can_step(&pos_alt) {
                        pos = new_pos;
                        insert = idx;
                        continue 'euler;
                    }
                }
                break;
            }

            // are there edges remaining in the graph, then start a new Eulerian tour
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

fn compress_path(
    micro_steps: impl Iterator<Item = MicroStep>,
) -> Result<Vec<PathSegment>, BitmapConversionError> {
    let mut steps = Vec::new();
    let mut pos = (0, 0);

    // step, "work in progress"
    let mut step_wip = None;
    for micro_step in micro_steps {
        match micro_step {
            MicroStep::Step((i, j)) => {
                match step_wip {
                    // check if we can combine step with step_wip
                    Some(PathSegment::Horizontal(m)) if i == pos.0 => {
                        let distance = checked_sub(j, pos.1)?;
                        step_wip = Some(PathSegment::Horizontal(checked_add(m, distance)?));
                    }
                    Some(PathSegment::Vertical(m)) if j == pos.1 => {
                        let distance = checked_sub(i, pos.0)?;
                        step_wip = Some(PathSegment::Vertical(checked_add(m, distance)?));
                    }
                    // start new step_wip
                    mut other => {
                        if let Some(other) = other.take() {
                            steps.push(other);
                        }
                        if i == pos.0 {
                            step_wip = Some(PathSegment::Horizontal(checked_sub(j, pos.1)?));
                        } else {
                            step_wip = Some(PathSegment::Vertical(checked_sub(i, pos.0)?));
                        }
                    }
                }
                pos = (i, j);
            }
            MicroStep::Jump((i, j)) => {
                // drop content of step_wip, just add close
                step_wip = None;
                steps.push(PathSegment::Close);
                steps.push(PathSegment::Move(
                    checked_sub(j, pos.1)?,
                    checked_sub(i, pos.0)?,
                ));
                pos = (i, j);
            }
        }
    }
    steps.push(PathSegment::Close);
    Ok(steps)
}

fn checked_add(left: N, right: N) -> Result<N, BitmapConversionError> {
    left.checked_add(right)
        .ok_or(BitmapConversionError::ArithmeticOverflow)
}

fn checked_sub(left: N, right: N) -> Result<N, BitmapConversionError> {
    left.checked_sub(right)
        .ok_or(BitmapConversionError::ArithmeticOverflow)
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

/// Oriented position in the graph on an edge.
#[derive(Debug, Clone, PartialEq)]
struct Position {
    i: N,
    j: N,
    dir: Direction,
}

impl Position {
    /// Get node coordinate of the node the position points to.
    fn end_node(&self) -> (N, N) {
        let i = self.i;
        let j = self.j;
        match self.dir {
            Direction::Up | Direction::Left => (i, j),
            Direction::Down => (i + 1, j),
            Direction::Right => (i, j + 1),
        }
    }

    /// Get node coordinate of the node the position comes from.
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
    /// Check if the left edge of cell `(i, j)` is part of the graph.
    fn left(&self, i: N, j: N) -> bool {
        self.edge(i, j).is_some_and(|edge| edge.left)
    }

    /// Check if the top edge of cell `(i, j)` is part of the graph.
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

    /// Check if there is any edge in the graph that could be reached from the position.
    ///
    /// See also [Self::follow()].
    fn can_step(&self, pos: &Position) -> Option<Position> {
        None.or_else(|| Some(pos.straight()).filter(|p| self.has_edge(p)))
            .or_else(|| Some(pos.left()).filter(|p| self.has_edge(p)))
            .or_else(|| Some(pos.right()).filter(|p| self.has_edge(p)))
    }

    /// Return a new position that can be reached from a given position.
    ///
    /// Returns the new position and and a boolean that indicates whether
    /// there was more than one possibility.
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

    /// Find a remaining position for the graph.
    ///
    /// If no edges are left `None` is returned.
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

fn bits_to_edge_graph(
    bits: &[bool],
    width: usize,
    height: usize,
) -> Result<Graph, BitmapConversionError> {
    let expected_bits = width
        .checked_mul(height)
        .ok_or(BitmapConversionError::ArithmeticOverflow)?;
    if bits.len() != expected_bits {
        return Err(BitmapConversionError::DataSize);
    }
    let graph_width = width
        .checked_add(1)
        .ok_or(BitmapConversionError::ArithmeticOverflow)?;
    let graph_height = height
        .checked_add(1)
        .ok_or(BitmapConversionError::ArithmeticOverflow)?;
    N::try_from(graph_width).map_err(|_| BitmapConversionError::ArithmeticOverflow)?;
    N::try_from(graph_height).map_err(|_| BitmapConversionError::ArithmeticOverflow)?;
    let edge_count = graph_width
        .checked_mul(graph_height)
        .ok_or(BitmapConversionError::ArithmeticOverflow)?;
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
            let idx = i
                .checked_mul(width)
                .and_then(|row| row.checked_add(j))
                .ok_or(BitmapConversionError::ArithmeticOverflow)?;
            if bits.get(idx).copied() != Some(true) {
                continue;
            }
            let cell = i
                .checked_mul(graph_width)
                .and_then(|row| row.checked_add(j))
                .ok_or(BitmapConversionError::ArithmeticOverflow)?;
            edge_hint.get_or_insert(cell);
            if j == 0 || bits.get(idx - 1).copied() == Some(false) {
                // left
                set_edge(&mut graph.edges, cell, EdgeSide::Left)?;
            }
            if i == 0 || bits.get(idx - width).copied() == Some(false) {
                // top
                set_edge(&mut graph.edges, cell, EdgeSide::Top)?;
            }
            if j == width - 1 || bits.get(idx + 1).copied() == Some(false) {
                // right
                let right = cell
                    .checked_add(1)
                    .ok_or(BitmapConversionError::ArithmeticOverflow)?;
                set_edge(&mut graph.edges, right, EdgeSide::Left)?;
            }
            if i == height - 1 || bits.get(idx + width).copied() == Some(false) {
                // bottom
                let bottom = cell
                    .checked_add(graph_width)
                    .ok_or(BitmapConversionError::ArithmeticOverflow)?;
                set_edge(&mut graph.edges, bottom, EdgeSide::Top)?;
            }
        }
    }
    graph.edge_hint.set(edge_hint.unwrap_or(graph.edges.len()));
    Ok(graph)
}

enum EdgeSide {
    Left,
    Top,
}

fn set_edge(edges: &mut [Edge], index: usize, side: EdgeSide) -> Result<(), BitmapConversionError> {
    let edge = edges
        .get_mut(index)
        .ok_or(BitmapConversionError::InternalError(
            "path kenarı graph sınırlarının dışında",
        ))?;
    match side {
        EdgeSide::Left => edge.left = true,
        EdgeSide::Top => edge.top = true,
    }
    Ok(())
}

#[test]
fn mini_2x2_one_euler() -> Result<(), BitmapConversionError> {
    let bm = Bitmap {
        bits: vec![true, false, true, true],
        width: 2,
    };
    assert_eq!(
        bits_to_edge_graph(&bm.bits, 2, 2)?,
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
        bm.path()?,
        vec![
            PathSegment::Horizontal(1),
            PathSegment::Vertical(1),
            PathSegment::Horizontal(1),
            PathSegment::Vertical(1),
            PathSegment::Horizontal(-2),
            PathSegment::Close,
        ],
    );
    Ok(())
}

#[test]
fn mini_2x3_one_euler() -> Result<(), BitmapConversionError> {
    let bm = Bitmap {
        bits: vec![true, false, true, true, true, false],
        width: 3,
    };
    assert_eq!(
        bm.path()?,
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
    Ok(())
}

#[test]
fn mini_3x2_two_euler() -> Result<(), BitmapConversionError> {
    let bm = Bitmap {
        bits: vec![true, true, false, false, false, true],
        width: 2,
    };
    assert_eq!(
        bm.path()?,
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
    Ok(())
}

#[test]
fn empty() -> Result<(), BitmapConversionError> {
    let bm = Bitmap::new(vec![false; 6], 2)?;
    assert_eq!(bm.path()?, vec![]);
    Ok(())
}

#[test]
fn edge_hint() -> Result<(), BitmapConversionError> {
    let bm = Bitmap {
        bits: vec![false, false, true, true, true, true],
        width: 3,
    };
    let graph = bits_to_edge_graph(&bm.bits, bm.width(), bm.height())?;
    assert_eq!(graph.edge_hint.get(), 2);
    Ok(())
}
