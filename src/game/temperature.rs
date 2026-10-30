//! AmbientTemperatureGridBehaviour: the air's temperature grid (World), the heat PhysicalBehaviour
//! exchanges with it and the per-fixed-step diffusion. Cell keys are Vector2Int as (x, y).
use super::{d, mathf, Game};

/// AmbientTemperatureGridBehaviour.Cell
#[derive(Clone, Debug)]
pub struct Cell {
    pub ReadTemperature: f32,
    pub WriteTemperature: f32,
    pub CanCreateNeighbours: bool,
    pub CanTransferTemperature: bool,
}

impl Cell {
    /// Cell(float t)
    pub fn new(t: f32) -> Cell {
        Cell { ReadTemperature: t, WriteTemperature: t, CanCreateNeighbours: true, CanTransferTemperature: true }
    }
    pub fn Set(&mut self, t: f32) {
        self.WriteTemperature = t;
    }
    pub fn ForceSet(&mut self, t: f32) {
        self.WriteTemperature = t;
        self.ReadTemperature = t;
    }
    pub fn Get(&self) -> f32 {
        self.ReadTemperature
    }
    pub fn Sync(&mut self) {
        self.ReadTemperature = self.WriteTemperature;
    }
}

/// ConcurrentDictionary<Vector2Int, Cell>.Node; the value is the Cell's slot (nodes copied by GrowTable share it).
#[derive(Clone, Debug)]
struct Node {
    key: (i32, i32),
    cell: usize,
    next: Option<usize>,
    hashcode: i32,
}

/// ConcurrentDictionary.Tables
#[derive(Clone, Debug)]
struct Tables {
    buckets: Vec<Option<usize>>,
    locks: usize,
    countPerLock: Vec<i32>,
}

/// ConcurrentDictionary<Vector2Int, Cell> as the game's mscorlib builds it (`new ConcurrentDictionary()`:
/// concurrencyLevel = Environment.ProcessorCount, capacity 31 raised to the concurrency level, growLockArray):
/// the same buckets, lock count, node chaining (new nodes go first in their bucket) and GrowTable, so an
/// enumeration visits the entries in the game's order. Nodes live in an arena; `tables` holds the current
/// Tables last, older ones only while an enumeration that started on them may still be walking them.
#[derive(Clone, Debug)]
pub struct CellWorld {
    cells: Vec<Cell>,
    nodes: Vec<Node>,
    tables: Vec<Tables>,
    budget: i32,
    growLockArray: bool,
}

impl Default for CellWorld {
    fn default() -> Self {
        Self::new(PROCESSOR_COUNT)
    }
}

/// Environment.ProcessorCount of the machine the recordings were made on (logical processors; the
/// Ryzen 9 9950X3D has 32). It sets the lock count and with it the first table size and budget.
pub const PROCESSOR_COUNT: i32 = 32;

/// Vector2Int.GetHashCode (EqualityComparer<Vector2Int>.Default goes through IEquatable to it)
fn vector2int_hash(key: (i32, i32)) -> i32 {
    key.0 ^ key.1.wrapping_shl(2)
}

/// ConcurrentDictionary.GetBucketAndLockNo
fn bucket_and_lock_no(hashcode: i32, bucket_count: usize, lock_count: usize) -> (usize, usize) {
    let bucket_no = ((hashcode & 0x7FFFFFFF) as usize) % bucket_count;
    (bucket_no, bucket_no % lock_count)
}

/// The enumerator GetEnumerator() returns: it holds the bucket array it started on and reads the buckets
/// and the nodes' next links live.
pub struct CellEnumerator {
    /// Bucket index to read next (head of that bucket) or the node last yielded in the bucket.
    bucket: usize,
    node: Option<usize>,
}

impl CellWorld {
    /// ConcurrentDictionary(concurrencyLevel, 31, growLockArray: true, null)
    pub fn new(concurrency_level: i32) -> CellWorld {
        let capacity = 31.max(concurrency_level) as usize;
        let locks = concurrency_level as usize;
        CellWorld {
            cells: Vec::new(),
            nodes: Vec::new(),
            tables: vec![Tables { buckets: vec![None; capacity], locks, countPerLock: vec![0; locks] }],
            budget: (capacity / locks) as i32,
            growLockArray: true,
        }
    }
    fn current(&self) -> &Tables {
        self.tables.last().unwrap()
    }
    /// Count
    pub fn len(&self) -> usize {
        self.current().countPerLock.iter().map(|&c| c as usize).sum()
    }
    /// ContainsKey
    pub fn contains_key(&self, key: (i32, i32)) -> bool {
        self.find(key).is_some()
    }
    /// TryGetValue: the cell's slot.
    pub fn find(&self, key: (i32, i32)) -> Option<usize> {
        let hashcode = vector2int_hash(key);
        let t = self.current();
        let mut node = t.buckets[((hashcode & 0x7FFFFFFF) as usize) % t.buckets.len()];
        while let Some(n) = node {
            if hashcode == self.nodes[n].hashcode && self.nodes[n].key == key {
                return Some(self.nodes[n].cell);
            }
            node = self.nodes[n].next;
        }
        None
    }
    /// TryAdd (TryAddInternal): the slot of the cell stored under key; an existing key keeps its cell.
    pub fn try_add(&mut self, key: (i32, i32), value: Cell) -> usize {
        let hashcode = vector2int_hash(key);
        let (bucket_no, lock_no) = {
            let t = self.current();
            bucket_and_lock_no(hashcode, t.buckets.len(), t.locks)
        };
        let mut node = self.current().buckets[bucket_no];
        while let Some(n) = node {
            if hashcode == self.nodes[n].hashcode && self.nodes[n].key == key {
                return self.nodes[n].cell;
            }
            node = self.nodes[n].next;
        }
        self.cells.push(value);
        let cell = self.cells.len() - 1;
        let head = self.current().buckets[bucket_no];
        self.nodes.push(Node { key, cell, next: head, hashcode });
        let n = self.nodes.len() - 1;
        let budget = self.budget;
        let t = self.tables.last_mut().unwrap();
        t.buckets[bucket_no] = Some(n);
        t.countPerLock[lock_no] += 1;
        if t.countPerLock[lock_no] > budget {
            self.grow_table();
        }
        cell
    }
    /// ConcurrentDictionary.GrowTable
    fn grow_table(&mut self) {
        let tables = self.current().clone();
        let num: i64 = tables.countPerLock.iter().map(|&c| c as i64).sum();
        if num < (tables.buckets.len() / 4) as i64 {
            self.budget = self.budget.wrapping_mul(2);
            if self.budget < 0 {
                self.budget = i32::MAX;
            }
            return;
        }
        let mut j = tables.buckets.len() as i32 * 2 + 1;
        while j % 3 == 0 || j % 5 == 0 || j % 7 == 0 {
            j += 2;
        }
        let mut locks = tables.locks;
        if self.growLockArray && locks < 1024 {
            locks *= 2;
        }
        let mut buckets: Vec<Option<usize>> = vec![None; j as usize];
        let mut count_per_lock = vec![0; locks];
        for l in 0..tables.buckets.len() {
            let mut node = tables.buckets[l];
            while let Some(n) = node {
                let next = self.nodes[n].next;
                let (key, cell, hashcode) = (self.nodes[n].key, self.nodes[n].cell, self.nodes[n].hashcode);
                let (bucket_no, lock_no) = bucket_and_lock_no(hashcode, buckets.len(), locks);
                self.nodes.push(Node { key, cell, next: buckets[bucket_no], hashcode });
                buckets[bucket_no] = Some(self.nodes.len() - 1);
                count_per_lock[lock_no] += 1;
                node = next;
            }
        }
        self.budget = 1.max(buckets.len() as i32 / locks as i32);
        self.tables.push(Tables { buckets, locks, countPerLock: count_per_lock });
    }
    /// Clear: a fresh 31-bucket table on the same locks.
    pub fn clear(&mut self) {
        let locks = self.current().locks;
        self.cells.clear();
        self.nodes.clear();
        self.tables = vec![Tables { buckets: vec![None; 31], locks, countPerLock: vec![0; locks] }];
        self.budget = 1.max(31 / locks as i32);
    }
    /// GetEnumerator: starts on the current bucket array.
    pub fn begin_enumeration(&mut self) -> CellEnumerator {
        // Older tables are only needed by enumerations that began before a growth; none outlive Diffuse.
        let last = self.tables.len() - 1;
        self.tables.drain(..last);
        CellEnumerator { bucket: 0, node: None }
    }
    /// The enumerator's MoveNext: the next entry's key and cell slot. It walks the bucket array it started
    /// on, so entries added to buckets it has not reached are seen and those added to a later table are not.
    pub fn move_next(&self, e: &mut CellEnumerator) -> Option<((i32, i32), usize)> {
        let buckets = &self.tables[0].buckets;
        loop {
            let found = match e.node {
                Some(n) => match self.nodes[n].next {
                    Some(m) => Some(m),
                    None => {
                        e.bucket += 1;
                        e.node = None;
                        continue;
                    }
                },
                None => {
                    if e.bucket >= buckets.len() {
                        return None;
                    }
                    match buckets[e.bucket] {
                        Some(m) => Some(m),
                        None => {
                            e.bucket += 1;
                            continue;
                        }
                    }
                }
            };
            e.node = found;
            let n = found.unwrap();
            return Some((self.nodes[n].key, self.nodes[n].cell));
        }
    }
    pub fn cell(&self, i: usize) -> &Cell {
        &self.cells[i]
    }
    pub fn cell_mut(&mut self, i: usize) -> &mut Cell {
        &mut self.cells[i]
    }
}

/// AmbientTemperatureGridBehaviour (the Main scene's instance: its serialized rates and GridSize).
#[derive(Clone, Debug)]
pub struct AmbientTemperatureGridBehaviour {
    pub World: CellWorld,
    pub ObjectToAirTransferRate: f32,
    pub AirToObjectTransferRate: f32,
    pub AmbientTemperatureTransferRate: f32,
    pub GridSize: i32,
    pub BlockedCells: std::collections::HashSet<(i32, i32)>,
}

impl Default for AmbientTemperatureGridBehaviour {
    fn default() -> Self {
        Self {
            World: CellWorld::default(),
            ObjectToAirTransferRate: 0.0035,
            AirToObjectTransferRate: 0.0003,
            AmbientTemperatureTransferRate: 0.00025,
            GridSize: 10,
            BlockedCells: Default::default(),
        }
    }
}

/// Mathf.RoundToInt (Math.Round: ties to even).
fn round_to_int(f: f32) -> i32 {
    (f as f64).round_ties_even() as i32
}
fn ceil_to_int(f: f32) -> i32 {
    (f as f64).ceil() as i32
}
fn floor_to_int(f: f32) -> i32 {
    (f as f64).floor() as i32
}

/// GetAverageNeighbouringTemperature's isDifferenceAboveThreshold
fn is_difference_above_threshold(a: f32, b: f32) -> bool {
    (mathf::clamp(a, -350.0, 350.0) - mathf::clamp(b, -350.0, 350.0)).abs() > 300.0
}

impl Game {
    /// [Obsolete] AmbientTemperatureGridBehaviour.ComputeBlockedCellPositions
    pub fn ComputeBlockedCellPositions(&mut self) {
        self.ambient_grid.BlockedCells.clear();
        let b = self.BoundingBox;
        let min = [b.center[0] - b.extents[0], b.center[1] - b.extents[1]];
        let max = [b.center[0] + b.extents[0], b.center[1] + b.extents[1]];
        let size = self.ambient_grid.GridSize;
        let g = size as f32;
        let num = floor_to_int(min[0] / g) * size;
        let num2 = ceil_to_int(max[0] / g) * size;
        let num3 = floor_to_int(min[1] / g) * size;
        let num4 = ceil_to_int(max[1] / g) * size;
        for _i in num..num2 {
            for _j in num3..num4 {
                // Physics2D.OverlapPoint(GridToWorldPoint(i, j)), then collider2D.gameObject.layer == 11.
                unimplemented!("Physics2D.OverlapPoint");
            }
        }
    }

    /// [Obsolete] AmbientTemperatureGridBehaviour.IsInsideWorldWall
    pub fn IsInsideWorldWall(&self, grid_point: (i32, i32)) -> bool {
        self.ambient_grid.BlockedCells.contains(&grid_point)
    }

    /// AmbientTemperatureGridBehaviour.FixedUpdate. ponytail: the game queues Diffuse on the thread pool and
    /// it races the main thread; here it runs to completion at the queue point, so diffuseIsFree is always true.
    pub fn ambient_grid_fixed_update(&mut self) {
        if self.prefs.AmbientTemperatureTransfer {
            self.ambient_grid_diffuse();
        }
    }

    /// AmbientTemperatureGridBehaviour.Diffuse
    fn ambient_grid_diffuse(&mut self) {
        // foreach over World: the enumerator sees what GetAverageNeighbouringTemperature adds meanwhile.
        let mut e = self.ambient_grid.World.begin_enumeration();
        while let Some((key, cell)) = self.ambient_grid.World.move_next(&mut e) {
            let t = self.get_average_neighbouring_temperature(cell, key);
            self.ambient_grid.World.cell_mut(cell).Set(t);
        }
        let mut e = self.ambient_grid.World.begin_enumeration();
        while let Some((_, cell)) = self.ambient_grid.World.move_next(&mut e) {
            self.ambient_grid.World.cell_mut(cell).Sync();
        }
    }

    /// AmbientTemperatureGridBehaviour.GetAverageNeighbouringTemperature; center is the cell's slot in World.
    fn get_average_neighbouring_temperature(&mut self, center: usize, v: (i32, i32)) -> f32 {
        let num = self.ambient_grid.World.cell(center).Get();
        let temperature_at = self.GetTemperatureAt(v.0 - 1, v.1);
        let temperature_at2 = self.GetTemperatureAt(v.0 + 1, v.1);
        let temperature_at3 = self.GetTemperatureAt(v.0, v.1 + 1);
        let temperature_at4 = self.GetTemperatureAt(v.0, v.1 - 1);
        // MapConfig.Instance.Bounds.BoundingBox is the camera's BoundingBox (BoundingBoxBehaviour.Start copies
        // it); Bounds.Contains is inclusive, the point's z is 0 and the maps' box has z 0 +- 0.
        let b = self.BoundingBox;
        let (x, y) = (v.0 as f32, v.1 as f32);
        let contains = !(x < b.center[0] - b.extents[0]
            || x > b.center[0] + b.extents[0]
            || y < b.center[1] - b.extents[1]
            || y > b.center[1] + b.extents[1]);
        if self.ambient_grid.World.cell(center).CanCreateNeighbours && contains && is_difference_above_threshold(num, self.AmbientTemperature) {
            let neighbours = [
                ((v.0 - 1, v.1), temperature_at),
                ((v.0 + 1, v.1), temperature_at2),
                ((v.0, v.1 + 1), temperature_at3),
                ((v.0, v.1 - 1), temperature_at4),
            ];
            for (pos, t) in neighbours {
                if is_difference_above_threshold(t, num) && !self.ambient_grid.World.contains_key(pos) {
                    // createCell
                    self.ambient_grid.World.cell_mut(center).CanCreateNeighbours = false;
                    let value = Cell::new(self.AmbientTemperature);
                    self.ambient_grid.World.try_add(pos, value);
                }
            }
        }
        ((d(temperature_at) + temperature_at2 + temperature_at3 + temperature_at4 + num) / 5.0f32).f()
    }

    /// AmbientTemperatureGridBehaviour.GetTemperatureAt
    pub fn GetTemperatureAt(&self, x: i32, y: i32) -> f32 {
        if let Some(i) = self.ambient_grid.World.find((x, y)) {
            let value = self.ambient_grid.World.cell(i);
            if value.CanTransferTemperature {
                return value.Get();
            }
        }
        self.AmbientTemperature
    }

    /// AmbientTemperatureGridBehaviour.GetTemperatureAtPoint
    pub fn GetTemperatureAtPoint(&self, world_point: [f32; 2]) -> f32 {
        let v = self.WorldToGridPoint(world_point[0], world_point[1]);
        self.GetTemperatureAt(v.0, v.1)
    }

    /// AmbientTemperatureGridBehaviour.ChangeTemperatureAt(Vector2, float)
    pub fn ChangeTemperatureAtPoint(&mut self, world_point: [f32; 2], celsius_change: f32) {
        let key = self.WorldToGridPoint(world_point[0], world_point[1]);
        if let Some(i) = self.ambient_grid.World.find(key) {
            let value = self.ambient_grid.World.cell_mut(i);
            if value.CanTransferTemperature {
                let num = value.Get();
                value.Set(num + celsius_change);
            }
        }
    }

    /// AmbientTemperatureGridBehaviour.SetTemperatureAt(Vector2, float)
    pub fn SetTemperatureAtPoint(&mut self, world_point: [f32; 2], celsius: f32) {
        let key = self.WorldToGridPoint(world_point[0], world_point[1]);
        if let Some(i) = self.ambient_grid.World.find(key) {
            let value = self.ambient_grid.World.cell_mut(i);
            if value.CanTransferTemperature {
                value.Set(celsius);
            }
        }
    }

    /// AmbientTemperatureGridBehaviour.ChangeTemperatureAt(int, int, float)
    pub fn ChangeTemperatureAt(&mut self, x: i32, y: i32, celsius_change: f32) {
        if let Some(i) = self.ambient_grid.World.find((x, y)) {
            let value = self.ambient_grid.World.cell_mut(i);
            if value.CanTransferTemperature {
                let num = value.Get();
                value.ForceSet(num + celsius_change);
            }
        }
    }

    /// AmbientTemperatureGridBehaviour.SetTemperatureAt(int, int, float)
    pub fn SetTemperatureAt(&mut self, x: i32, y: i32, celsius: f32) {
        if let Some(i) = self.ambient_grid.World.find((x, y)) {
            let value = self.ambient_grid.World.cell_mut(i);
            if value.CanTransferTemperature {
                value.Set(celsius);
            }
        }
    }

    /// AmbientTemperatureGridBehaviour.EnsureExistence
    pub fn EnsureExistence(&mut self, x: i32, y: i32) {
        if !self.ambient_grid.World.contains_key((x, y)) {
            let value = Cell::new(self.AmbientTemperature);
            self.ambient_grid.World.try_add((x, y), value);
        }
    }

    /// AmbientTemperatureGridBehaviour.TransferHeat
    pub fn TransferHeat(&mut self, phys: usize) {
        // phys.spriteRenderer is never null for a PhysicalBehaviour (Awake dereferences it).
        if !self.physicals[phys].SimulateTemperature {
            return;
        }
        let (center, size) = self.sprite_renderer_bounds(phys);
        let num = mathf::min(size[0], 30.0);
        let num2 = mathf::min(size[1], 30.0);
        let num3 = ceil_to_int(num);
        let num4 = ceil_to_int(num2);
        let num5 = num3 * num4;
        let multiplier = 1.0f32 / num5 as f32;
        let num6 = num / 2.0f32;
        let num7 = num2 / 2.0f32;
        for i in 0..num3 {
            for j in 0..num4 {
                let x = (d(center[0]) + i as f32 - num6).f();
                let y = (d(center[1]) + j as f32 - num7).f();
                let key = self.WorldToGridPoint(x, y);
                let cell = match self.ambient_grid.World.find(key) {
                    Some(c) => c,
                    None => {
                        let value = Cell::new(self.AmbientTemperature);
                        self.ambient_grid.World.try_add(key, value)
                    }
                };
                self.ambient_grid_transfer(phys, cell, multiplier);
            }
        }
    }

    /// SpriteRenderer.bounds of a physical object: (center, size), size being extents * 2.
    fn sprite_renderer_bounds(&self, phys: usize) -> ([f32; 3], [f32; 3]) {
        let (c, e) = self.sprite_world_bounds(phys);
        (c, [e[0] * 2.0, e[1] * 2.0, e[2] * 2.0])
    }

    /// AmbientTemperatureGridBehaviour.WorldToGridPoint
    pub fn WorldToGridPoint(&self, x: f32, y: f32) -> (i32, i32) {
        let g = self.ambient_grid.GridSize as f32;
        (round_to_int(x / g), round_to_int(y / g))
    }

    /// AmbientTemperatureGridBehaviour.GridToWorldPoint
    pub fn GridToWorldPoint(&self, x: i32, y: i32) -> [f32; 2] {
        let g = self.ambient_grid.GridSize;
        [(x * g) as f32, (y * g) as f32]
    }

    /// AmbientTemperatureGridBehaviour.Transfer; cell is its slot in World.
    fn ambient_grid_transfer(&mut self, phys: usize, cell: usize, multiplier: f32) {
        if self.physicals[phys].SimulateTemperature {
            let temperature = self.physicals[phys].Temperature;
            let num = self.ambient_grid.World.cell(cell).Get();
            let heat_capacity = self.heat_capacity(phys);
            let air_to_object = self.ambient_grid.AirToObjectTransferRate;
            let object_to_air = self.ambient_grid.ObjectToAirTransferRate;
            self.physicals[phys].Temperature = mathf::lerp(temperature, num, (d(air_to_object) / heat_capacity * multiplier).f());
            self.ambient_grid.World.cell_mut(cell).ForceSet(mathf::lerp(num, temperature, (d(object_to_air) * heat_capacity * multiplier).f()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(w: &mut CellWorld) -> Vec<(i32, i32)> {
        let mut e = w.begin_enumeration();
        let mut keys = Vec::new();
        while let Some((k, _)) = w.move_next(&mut e) {
            keys.push(k);
        }
        keys
    }

    /// 32 processors: 32 buckets, 32 locks, budget 1. Hash is x ^ (y << 2); a new node goes first in its bucket.
    #[test]
    fn world_enumeration_order() {
        let mut w = CellWorld::new(32);
        for k in [(0, 0), (4, 0), (32, 0), (0, 1), (1, 0)] {
            w.try_add(k, Cell::new(0.0));
        }
        // Buckets: (0,0)=0, (32,0)=0 (before it), (1,0)=1, (4,0)=4, (0,1)=4 (before (4,0)). The second
        // entry of bucket 0 raised the budget (2 < 32 / 4), no growth yet.
        assert_eq!(order(&mut w), vec![(32, 0), (0, 0), (1, 0), (0, 1), (4, 0)]);
        assert_eq!(w.len(), 5);

        // Growth: the 9th entry of lock 0 (all in bucket 0) finds 9 >= 32 / 4 entries; 32 * 2 + 1 = 65 is
        // divisible by 5, so 67 buckets; entries re-chain in old chain order into the new buckets.
        let mut w = CellWorld::new(32);
        for m in 0..9 {
            w.try_add((m * 32, 0), Cell::new(0.0));
        }
        let keys: Vec<i32> = order(&mut w).iter().map(|k| k.0).collect();
        assert_eq!(keys, vec![0, 224, 160, 96, 32, 256, 192, 128, 64]);
        assert_eq!(w.tables.last().unwrap().buckets.len(), 67);
        assert_eq!(w.tables.last().unwrap().locks, 64);
        assert_eq!(w.find((96, 0)).is_some(), true);
        assert_eq!(w.find((97, 0)).is_some(), false);
    }

    /// The enumerator reads buckets and next links live: later buckets show additions, passed ones do not.
    #[test]
    fn world_enumerator_is_live() {
        let mut w = CellWorld::new(32);
        w.try_add((0, 0), Cell::new(0.0));
        w.try_add((2, 0), Cell::new(0.0));
        let mut e = w.begin_enumeration();
        assert_eq!(w.move_next(&mut e).unwrap().0, (0, 0));
        w.try_add((5, 0), Cell::new(0.0)); // bucket 5: not reached yet
        w.try_add((32, 0), Cell::new(0.0)); // bucket 0: in front of the node the enumerator stands on
        let mut seen = Vec::new();
        while let Some((k, _)) = w.move_next(&mut e) {
            seen.push(k);
        }
        assert_eq!(seen, vec![(2, 0), (5, 0)]);
    }

    /// A hot cell spreads to its neighbours (created once the difference passes 300) and averages over 5.
    #[test]
    fn diffuse_creates_and_averages() {
        let mut g = Game::default();
        g.AmbientTemperature = 20.0;
        g.prefs.AmbientTemperatureTransfer = true;
        g.BoundingBox = super::super::Bounds { center: [0.0, 0.0], extents: [100.0, 100.0] };
        g.ambient_grid.World.try_add((0, 0), Cell::new(520.0));
        g.ambient_grid_fixed_update();
        assert_eq!(g.ambient_grid.World.len(), 5);
        assert_eq!(g.GetTemperatureAt(0, 0), (520.0f32 + 20.0 * 4.0) / 5.0);
        assert!(!g.ambient_grid.World.cell(0).CanCreateNeighbours);
        assert_eq!(g.GetTemperatureAt(7, 7), 20.0);
    }
}
