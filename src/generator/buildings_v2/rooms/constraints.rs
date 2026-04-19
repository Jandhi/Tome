use crate::geometry::Rect2D;

/// State of a cell in a room's interior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellState {
    /// Walkable, available for furniture placement.
    Empty,
    /// Furniture or structure here. Impassable, cannot place on.
    Blocked,
    /// Impassable, cannot place on, must stay reachable (doors, chest fronts).
    /// Connectivity check ensures each cell is adjacent to a walkable cell.
    BlockedReachable,
    /// Walkable, cannot place on (carpets, decorations).
    UnblockedReachable,
}

/// 2D grid of cell states for a room's interior.
/// Ground layer tracks walkability/constraints. Ceiling layer is just occupied or not.
#[derive(Debug, Clone)]
pub struct ConstraintMap {
    /// World coordinate of the grid's (0,0) corner.
    pub origin: (i32, i32),
    /// ground[x][z] — floor-level cell states (walkability, accessibility).
    pub ground: Vec<Vec<CellState>>,
    /// ceiling[x][z] — true if a ceiling item is placed here.
    pub ceiling: Vec<Vec<bool>>,
}

impl ConstraintMap {
    /// Create a grid covering the given interior rect, all cells empty.
    pub fn new(interior: &Rect2D) -> Self {
        let w = interior.size.x as usize;
        let h = interior.size.y as usize;
        Self {
            origin: (interior.min().x, interior.min().y),
            ground: vec![vec![CellState::Empty; h]; w],
            ceiling: vec![vec![false; h]; w],
        }
    }

    /// Convert world coords to local indices. Returns None if out of bounds.
    fn local(&self, cell: (i32, i32)) -> Option<(usize, usize)> {
        let lx = cell.0 - self.origin.0;
        let lz = cell.1 - self.origin.1;
        if lx < 0 || lz < 0 { return None; }
        let (ux, uz) = (lx as usize, lz as usize);
        if ux < self.ground.len() && uz < self.ground[0].len() {
            Some((ux, uz))
        } else {
            None
        }
    }

    pub fn get(&self, cell: (i32, i32)) -> Option<CellState> {
        self.local(cell).map(|(x, z)| self.ground[x][z])
    }

    pub fn set(&mut self, cell: (i32, i32), state: CellState) {
        if let Some((x, z)) = self.local(cell) {
            self.ground[x][z] = state;
        }
    }

    pub fn ceiling_occupied(&self, cell: (i32, i32)) -> bool {
        self.local(cell).map_or(true, |(x, z)| self.ceiling[x][z])
    }

    pub fn set_ceiling(&mut self, cell: (i32, i32)) {
        if let Some((x, z)) = self.local(cell) {
            self.ceiling[x][z] = true;
        }
    }

    /// Walkable on the ground layer (Empty or Occupied).
    pub fn is_walkable(&self, cell: (i32, i32)) -> bool {
        matches!(self.get(cell), Some(CellState::Empty | CellState::UnblockedReachable))
    }

    /// Open for ground furniture placement.
    pub fn is_open(&self, cell: (i32, i32)) -> bool {
        matches!(self.get(cell), Some(CellState::Empty))
    }

    /// Fill ratio on the ground layer.
    pub fn fill_ratio(&self) -> f32 {
        let total = self.ground.iter().map(|col| col.len()).sum::<usize>();
        if total == 0 { return 0.0; }
        let filled = self.ground.iter().flatten().filter(|&&s| s != CellState::Empty).count();
        filled as f32 / total as f32
    }

    pub fn width(&self) -> usize { self.ground.len() }

    pub fn height(&self) -> usize {
        self.ground.first().map_or(0, |col| col.len())
    }

    /// Iterate over ground layer cells as (world_x, world_z, state).
    pub fn iter_ground(&self) -> impl Iterator<Item = ((i32, i32), CellState)> + '_ {
        self.ground.iter().enumerate().flat_map(move |(x, col)| {
            col.iter().enumerate().map(move |(z, &state)| {
                ((self.origin.0 + x as i32, self.origin.1 + z as i32), state)
            })
        })
    }
}

/// A record of a placed furniture item.
#[derive(Debug, Clone)]
pub struct PlacedFurniture {
    /// Furniture item key (e.g. "bed", "furnace").
    pub name: String,
    /// World (x, z) cells occupied by this item.
    pub cells: Vec<(i32, i32)>,
}
