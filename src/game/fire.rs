//! LocalFirePropagationMap: fire spreading over a burnable object's serialized burn points, and the Mono
//! Array.Sort its Start shuffles with.
use super::misc::FixedIntervalDistributor;
use super::{d, mathf, Game};

/// LocalFirePropagationMap.BurnPoint
#[derive(Clone, Debug, Default)]
pub struct BurnPoint {
    pub LocalPoint: [f32; 2],
    pub FireIntensity: f32,
    pub Softness: f32,
    pub NeighbourIndices: Vec<i32>,
}

/// LocalFirePropagationMap
#[derive(Clone, Debug)]
pub struct FireMap {
    /// phys (GetComponent<PhysicalBehaviour>), on the same GameObject.
    pub physical: usize,
    /// Collider (its instance id); None when the field is null.
    pub collider: Option<i32>,
    pub Points: Vec<BurnPoint>,
    pub GridStep: f32,
    pub FireIntensityDampening: f32,
    pub FireSpreadSpeed: f32,
    pub RandomisedIndices: Vec<i32>,
    /// PhysicalBehaviour.fireEmissionInterval (MaxCycles 2); only the fire map's emission uses it.
    pub fireEmissionInterval: FixedIntervalDistributor,
}

impl FireMap {
    pub fn new(physical: usize) -> Self {
        Self {
            physical,
            collider: None,
            Points: Vec::new(),
            GridStep: 0.2,
            FireIntensityDampening: 3.0,
            FireSpreadSpeed: 1.0,
            RandomisedIndices: Vec::new(),
            fireEmissionInterval: FixedIntervalDistributor { MaxCycles: 2, ..FixedIntervalDistributor::with_rate(60.0) },
        }
    }
}

/// Utils.SmoothApproach(float, float, float, float)
fn smooth_approach(past: f32, target: f32, speed: f32, dt: f32) -> f32 {
    let num = dt * speed;
    let exp = (-(num as f64)).exp() as f32;
    let v = (d(past) - target) * exp + d(target);
    // NaNFallback: NaN reads 0.
    let v = v.f();
    if v.is_nan() { 0.0 } else { v }
}

impl Game {
    /// LocalFirePropagationMap.Start
    pub fn fire_map_start(&mut self, f: usize) {
        let n = self.fire_maps[f].Points.len();
        let mut indices = Vec::with_capacity(n);
        for i in 0..n {
            self.fire_maps[f].Points[i].Softness = self.random.range(0.001, 1.0);
            indices.push(i as i32);
        }
        // Array.Sort(RandomisedIndices, (a, b) => Random.value > 0f ? 1 : -1)
        let random = &mut self.random;
        introspective_sort(&mut indices, &mut |_, _| if random.value() > 0.0 { 1 } else { -1 });
        self.fire_maps[f].RandomisedIndices = indices;
    }

    /// LocalFirePropagationMap.FixedUpdate
    pub fn fire_map_fixed_update(&mut self, f: usize) {
        let dt = self.time.fixedDeltaTime;
        let on_fire = self.physicals[self.fire_maps[f].physical].OnFire;
        let m = &mut self.fire_maps[f];
        for i in 0..m.Points.len() {
            for j in 0..m.Points[i].NeighbourIndices.len() {
                let num = m.Points[i].NeighbourIndices[j] as usize;
                let burn = m.Points[num].FireIntensity;
                if burn > m.Points[i].FireIntensity {
                    let speed = m.FireSpreadSpeed * m.Points[i].Softness;
                    m.Points[i].FireIntensity = smooth_approach(m.Points[i].FireIntensity, burn, speed, dt);
                } else {
                    let speed = m.FireSpreadSpeed * m.Points[num].Softness;
                    m.Points[num].FireIntensity = smooth_approach(burn, m.Points[i].FireIntensity, speed, dt);
                }
            }
            if !on_fire {
                let speed = m.FireIntensityDampening * m.Points[i].Softness;
                m.Points[i].FireIntensity = smooth_approach(m.Points[i].FireIntensity, 0.0, speed, dt);
            }
        }
    }

    /// LocalFirePropagationMap.GetRandomFireyIndex
    pub fn fire_map_random_firey_index(&mut self, f: usize, minimum: f32) -> i32 {
        let len = self.fire_maps[f].Points.len() as i32;
        let mut result = -1;
        let num = 1.0 / len as f32;
        let mut num2 = self.random.range_int(0, len);
        for _ in 0..len {
            num2 = (num2 + 1) % len;
            if self.fire_maps[f].Points[num2 as usize].FireIntensity > minimum {
                result = num2;
            }
            if num > self.random.value() {
                return result;
            }
        }
        result
    }

    /// PhysicalBehaviour.Ignite(bool)'s Points[Random.Range(0, Points.Length)].FireIntensity = 1f.
    pub fn fire_map_ignite_random(&mut self, f: usize) {
        let len = self.fire_maps[f].Points.len() as i32;
        let i = self.random.range_int(0, len);
        let Some(point) = self.fire_maps[f].Points.get_mut(i as usize) else {
            unimplemented!("LocalFirePropagationMap without Points: Points[Random.Range(0, 0)] (IndexOutOfRangeException)")
        };
        point.FireIntensity = 1.0;
    }

    /// LocalFirePropagationMap.IgniteAt
    pub fn fire_map_ignite_at(&mut self, f: usize, world_point: [f32; 2]) {
        let Some(collider) = self.fire_maps[f].collider else { unimplemented!("LocalFirePropagationMap.IgniteAt with a null Collider") };
        // Collider2D.ClosestPoint starts with the AutoSyncTransforms.
        self.sync_pending();
        let c = self.world.collider_closest_point(collider, crate::b2::math::Vec2::new(world_point[0], world_point[1]));
        let local = self.inverse_transform_point(self.fire_maps[f].physical, [c.x, c.y, 0.0]);
        let m = &mut self.fire_maps[f];
        let num = m.GridStep * m.GridStep;
        let mut num2 = f32::MAX;
        let mut num3 = 0;
        for i in 0..m.Points.len() {
            let sqr = mathf::sqr_magnitude([m.Points[i].LocalPoint[0] - local[0], m.Points[i].LocalPoint[1] - local[1]]);
            if sqr < num2 {
                num2 = sqr;
                num3 = i;
            }
            if sqr < num {
                m.Points[i].FireIntensity = 1.0;
            }
        }
        let Some(point) = m.Points.get_mut(num3) else { unimplemented!("LocalFirePropagationMap.IgniteAt without Points (IndexOutOfRangeException)") };
        point.FireIntensity = 1.0;
    }

    /// PhysicalBehaviour.SetParticleEmission's fire-map branch (BurnIntensity > 0.01): the Random draws of the
    /// emission; the emitted particles are presentation.
    pub fn fire_map_emission(&mut self, p: usize, f: usize) {
        let e = self.sprite_world_bounds(p).1;
        let sqr = (d(e[0]) * e[0] + d(e[1]) * e[1] + d(e[2]) * e[2]).f();
        let num = mathf::sqrt(sqr * 4.0);
        let ph = &self.physicals[p];
        let num2 = ((d(1.0) - ph.BurnProgress) * ph.burnIntensity * 2.0f32 * num).f();
        let len = self.fire_maps[f].Points.len() as i32;
        // RateHz = 15f * (float)Mathf.Max(Points.Length, 25) * 0.06f
        let rate = (d(15.0) * len.max(25) as f32 * 0.06f32).f();
        self.fire_maps[f].fireEmissionInterval.IntervalSeconds = 1.0 / rate;
        let dt = self.time.deltaTime;
        // The loop condition re-runs CalculateCycleCount (adding dt again) every iteration.
        let mut i = 0;
        while i < self.fire_maps[f].fireEmissionInterval.calculate_cycle_count(dt) {
            i += 1;
            let index = self.fire_map_random_firey_index(f, 0.05);
            if index != -1 {
                let fi = self.fire_maps[f].Points[index as usize].FireIntensity;
                let num3 = (d(fi) * fi * fi * fi * fi * fi * num2).f();
                // FireParticleSystem.Emit at TransformPoint(LocalPoint): presentation.
                let _ = self.random.value() < num3;
            }
        }
    }
}

/// IntrospectiveSortUtilities.FloorLog2
fn floor_log2(mut n: i32) -> i32 {
    let mut num = 0;
    while n >= 1 {
        num += 1;
        n /= 2;
    }
    num
}

/// Mono ArraySortHelper<T>.Sort(keys, 0, keys.Length, Comparison): IntrospectiveSort. An index the comparer drives
/// out of the array is the IndexOutOfRangeException Mono turns into ThrowOrIgnoreBadComparer.
pub fn introspective_sort(keys: &mut [i32], cmp: &mut impl FnMut(i32, i32) -> i32) {
    if keys.len() >= 2 {
        let hi = keys.len() as isize - 1;
        intro_sort(keys, 0, hi, 2 * floor_log2(keys.len() as i32), cmp);
    }
}

fn at(keys: &[i32], i: isize) -> i32 {
    *usize::try_from(i).ok().and_then(|i| keys.get(i)).unwrap_or_else(|| unimplemented!("Array.Sort: IndexOutOfRangeException from a bad comparer (ThrowOrIgnoreBadComparer)"))
}

fn swap_if_greater(keys: &mut [i32], cmp: &mut impl FnMut(i32, i32) -> i32, a: isize, b: isize) {
    if a != b && cmp(at(keys, a), at(keys, b)) > 0 {
        keys.swap(a as usize, b as usize);
    }
}

fn intro_sort(keys: &mut [i32], lo: isize, mut hi: isize, mut depth_limit: i32, cmp: &mut impl FnMut(i32, i32) -> i32) {
    while hi > lo {
        let num = hi - lo + 1;
        if num <= 16 {
            match num {
                1 => {}
                2 => swap_if_greater(keys, cmp, lo, hi),
                3 => {
                    swap_if_greater(keys, cmp, lo, hi - 1);
                    swap_if_greater(keys, cmp, lo, hi);
                    swap_if_greater(keys, cmp, hi - 1, hi);
                }
                _ => insertion_sort(keys, lo, hi, cmp),
            }
            break;
        }
        if depth_limit == 0 {
            heapsort(keys, lo, hi, cmp);
            break;
        }
        depth_limit -= 1;
        let num2 = pick_pivot_and_partition(keys, lo, hi, cmp);
        intro_sort(keys, num2 + 1, hi, depth_limit, cmp);
        hi = num2 - 1;
    }
}

fn pick_pivot_and_partition(keys: &mut [i32], lo: isize, hi: isize, cmp: &mut impl FnMut(i32, i32) -> i32) -> isize {
    let num = lo + (hi - lo) / 2;
    swap_if_greater(keys, cmp, lo, num);
    swap_if_greater(keys, cmp, lo, hi);
    swap_if_greater(keys, cmp, num, hi);
    let val = keys[num as usize];
    keys.swap(num as usize, (hi - 1) as usize);
    let (mut num2, mut num3) = (lo, hi - 1);
    while num2 < num3 {
        loop {
            num2 += 1;
            if cmp(at(keys, num2), val) >= 0 {
                break;
            }
        }
        loop {
            num3 -= 1;
            if cmp(val, at(keys, num3)) >= 0 {
                break;
            }
        }
        if num2 >= num3 {
            break;
        }
        keys.swap(num2 as usize, num3 as usize);
    }
    keys.swap(num2 as usize, (hi - 1) as usize);
    num2
}

fn heapsort(keys: &mut [i32], lo: isize, hi: isize, cmp: &mut impl FnMut(i32, i32) -> i32) {
    let num = hi - lo + 1;
    for i in (1..=num / 2).rev() {
        down_heap(keys, i, num, lo, cmp);
    }
    for i in (2..=num).rev() {
        keys.swap(lo as usize, (lo + i - 1) as usize);
        down_heap(keys, 1, i - 1, lo, cmp);
    }
}

fn down_heap(keys: &mut [i32], mut i: isize, n: isize, lo: isize, cmp: &mut impl FnMut(i32, i32) -> i32) {
    let val = keys[(lo + i - 1) as usize];
    while i <= n / 2 {
        let mut num = 2 * i;
        if num < n && cmp(keys[(lo + num - 1) as usize], keys[(lo + num) as usize]) < 0 {
            num += 1;
        }
        if cmp(val, keys[(lo + num - 1) as usize]) >= 0 {
            break;
        }
        keys[(lo + i - 1) as usize] = keys[(lo + num - 1) as usize];
        i = num;
    }
    keys[(lo + i - 1) as usize] = val;
}

fn insertion_sort(keys: &mut [i32], lo: isize, hi: isize, cmp: &mut impl FnMut(i32, i32) -> i32) {
    for i in lo..hi {
        let mut num = i;
        let val = keys[(i + 1) as usize];
        while num >= lo && cmp(val, keys[num as usize]) < 0 {
            keys[(num + 1) as usize] = keys[num as usize];
            num -= 1;
        }
        keys[(num + 1) as usize] = val;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn introsort_sorts() {
        let mut seed = 12345u32;
        for n in 0..200 {
            let mut keys: Vec<i32> = (0..n)
                .map(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    (seed >> 16) as i32 % 50
                })
                .collect();
            let mut want = keys.clone();
            want.sort();
            introspective_sort(&mut keys, &mut |a, b| a.cmp(&b) as i32);
            assert_eq!(keys, want);
            // The heapsort fallback.
            let mut keys2 = want.iter().rev().copied().collect::<Vec<_>>();
            if n >= 2 {
                heapsort(&mut keys2, 0, n as isize - 1, &mut |a, b| a.cmp(&b) as i32);
            }
            assert_eq!(keys2, want);
        }
    }

    #[test]
    fn always_greater_comparer() {
        // Start's comparer: Random.value > 0 is (almost) always true, so every comparison says "greater".
        for n in 0..100 {
            let mut keys: Vec<i32> = (0..n).collect();
            let mut calls = 0;
            introspective_sort(&mut keys, &mut |_, _| {
                calls += 1;
                1
            });
            let mut sorted = keys.clone();
            sorted.sort();
            assert_eq!(sorted, (0..n).collect::<Vec<_>>());
        }
    }
}
