# Editor RefCell Refactor Plan

## Goal

Refactor `Editor` to use interior mutability so that generators can read from `World` and write blocks without borrow checker conflicts.

**Before:**
```rust
async fn generate(editor: &mut Editor) {
    let h = editor.world().get_height(p);  // borrows editor as &self
    editor.place_block(&block, p).await;   // ERROR: needs &mut self
}
```

**After:**
```rust
async fn generate(editor: &Editor) {
    let h = editor.world().get_height(p);  // &self
    editor.place_block(&block, p).await;   // &self - no conflict
}
```

## Current State

From [editor.rs:9-18](../src/editor/editor.rs):

```rust
pub struct Editor {
    build_area: Rect3D,
    provider: GDMCHTTPProvider,
    block_buffer: Vec<PositionedBlock>,      // needs mutation
    buffer_size: usize,
    block_cache: HashMap<Point3D, Block>,    // needs mutation
    world: World,
    materials: HashMap<MaterialId, Material>,
    block_form_cache: HashMap<BlockID, BlockForm>,  // needs mutation
}
```

## Analysis

| Field | Access Pattern | Needs RefCell? |
|-------|---------------|----------------|
| `build_area` | Read-only | No |
| `provider` | Read-only (internal mutation) | No |
| `block_buffer` | Push on place, clear on flush | **Yes** |
| `buffer_size` | Read-only after init | No |
| `block_cache` | Insert on place, read on get | **Yes** |
| `world` | Read-only | No |
| `materials` | Read-only after load | No |
| `block_form_cache` | Insert on cache miss | **Yes** |

## Changes

### Step 1: Update Editor struct

```rust
use std::cell::RefCell;

#[derive(Debug)]
pub struct Editor {
    build_area: Rect3D,
    provider: GDMCHTTPProvider,
    block_buffer: RefCell<Vec<PositionedBlock>>,     // wrapped
    buffer_size: usize,
    block_cache: RefCell<HashMap<Point3D, Block>>,   // wrapped
    world: World,
    materials: HashMap<MaterialId, Material>,
    block_form_cache: RefCell<HashMap<BlockID, BlockForm>>,  // wrapped
}
```

### Step 2: Update Editor::new()

```rust
pub fn new(build_area: Rect3D, world: World) -> Self {
    let mut editor = Self {
        build_area,
        provider: GDMCHTTPProvider::new(),
        block_buffer: RefCell::new(Vec::new()),       // wrap
        buffer_size: 32,
        block_cache: RefCell::new(HashMap::new()),    // wrap
        world,
        materials: HashMap::new(),
        block_form_cache: RefCell::new(HashMap::new()), // wrap
    };
    editor.load_data().expect("Failed to load materials");
    editor
}
```

### Step 3: Update place_block methods

Change `&mut self` to `&self`:

```rust
// Before
pub async fn place_block(&mut self, block: &Block, point: Point3D)

// After
pub async fn place_block(&self, block: &Block, point: Point3D)
```

Implementation:

```rust
pub async fn place_block(&self, block: &Block, point: Point3D) {
    self.place_block_options(block, point, false).await;
}

pub async fn place_block_forced(&self, block: &Block, point: Point3D) {
    self.place_block_options(block, point, true).await;
}

pub async fn place_block_options(&self, block: &Block, point: Point3D, force: bool) {
    if !self.world.build_area.contains(point + self.build_area.origin) {
        warn!("Point {:?} is outside build area", point + self.build_area.origin);
        return;
    }

    if !force && self.block_cache.borrow().contains_key(&point) {
        let density = self.get_block_form(&block.id).density();
        let current_block = self.block_cache.borrow()
            .get(&point)
            .expect("Block should be in cache")
            .id.clone();

        if density <= self.get_block_form(&current_block).density() {
            info!("Block at {:?} already has denser block, skipping", point);
            return;
        }
    }

    self.block_cache.borrow_mut().insert(point, block.clone());
    self.block_buffer.borrow_mut().push(
        PositionedBlock::from_block(block.clone(), (point + self.build_area.origin).into())
    );

    if self.block_buffer.borrow().len() >= self.buffer_size {
        self.flush_buffer().await;
    }
}
```

### Step 4: Update get_block_form

```rust
// Before
fn get_block_form(&mut self, id: &BlockID) -> BlockForm

// After
fn get_block_form(&self, id: &BlockID) -> BlockForm {
    // Check if cached
    if let Some(form) = self.block_form_cache.borrow().get(id) {
        return *form;
    }

    // Compute and cache
    let form = BlockForm::infer_from_block(id);
    self.block_form_cache.borrow_mut().insert(id.clone(), form);
    form
}
```

### Step 5: Update get_block

```rust
// Before
pub fn get_block(&mut self, point: Point3D) -> Block

// After
pub fn get_block(&self, point: Point3D) -> Block {
    if let Some(block) = self.block_cache.borrow().get(&(point - self.build_area.origin)) {
        return block.clone();
    }

    self.world.get_block(point)
        .expect(&format!("Block at {:?} not found", point))
}
```

### Step 6: Update flush_buffer

```rust
// Before
pub async fn flush_buffer(&mut self)

// After
pub async fn flush_buffer(&self) {
    let buffer: Vec<_> = self.block_buffer.borrow_mut().drain(..).collect();

    if buffer.is_empty() {
        return;
    }

    let result = self.provider.put_blocks(&buffer).await
        .expect("Failed to send blocks");

    for (index, response) in result.iter().enumerate() {
        // ... error checking with buffer[index] ...
    }
}
```

### Step 7: Update place_block_chance

```rust
// Before
pub async fn place_block_chance(&mut self, ...)

// After
pub async fn place_block_chance(&self, block: &Block, point: Point3D, rng: &mut RNG, chance: i32) {
    if rng.rand_i32_range(1, 100) <= chance {
        self.place_block(block, point).await;
    }
}
```

### Step 8: Update Drop impl

```rust
impl Drop for Editor {
    fn drop(&mut self) {
        if !self.block_buffer.borrow().is_empty() {
            error!("Editor dropped with non-empty block buffer!");
        }
    }
}
```

## Files to Modify

1. **src/editor/editor.rs** - Main changes listed above

2. **All generator files** - Change `&mut Editor` to `&Editor`:
   - src/generator/buildings/*.rs
   - src/generator/terrain/*.rs
   - src/generator/paths/*.rs
   - src/generator/districts/*.rs
   - src/generator/nbts/*.rs
   - src/main.rs

## Migration Strategy

### Phase 1: Editor changes (non-breaking)
1. Add RefCell wrappers to Editor struct
2. Update all Editor methods to use `&self`
3. Keep `&mut self` signatures temporarily with `#[allow(unused_mut)]`

### Phase 2: Update call sites
1. Search for `&mut editor` and `editor: &mut Editor`
2. Change to `&editor` and `editor: &Editor`
3. Remove unnecessary `mut` bindings

### Phase 3: Cleanup
1. Remove `#[allow(unused_mut)]`
2. Run clippy to find any remaining issues
3. Test thoroughly

## Search Patterns

Find all places that need updating:

```bash
# Find &mut Editor parameters
rg "&mut Editor" src/

# Find &mut self in editor.rs
rg "&mut self" src/editor/editor.rs

# Find mutable editor bindings
rg "mut editor" src/
```

## Testing

1. Run existing tests after each phase
2. Verify block placement still works correctly
3. Check that density system still prevents overwrites
4. Ensure buffer flushes at correct times
5. Test that Drop warning still fires for non-empty buffer

## Potential Issues

### Nested borrows
If any code path tries to borrow the same RefCell twice:
```rust
let a = self.block_cache.borrow();
let b = self.block_cache.borrow_mut();  // PANIC!
```

**Mitigation:** Keep borrows short-lived, drop before re-borrowing.

### Debug trait
`RefCell` implements `Debug` if `T: Debug`, so the derive should still work.

### Performance
RefCell has minimal overhead (atomic reference count check). Not a concern for this use case.

## Rollback Plan

If issues arise, revert to `&mut self` pattern. The changes are localized to Editor and its call sites.
