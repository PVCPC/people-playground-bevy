//! GearBehaviour and the Main scene's GearManagerBehaviour (both [Obsolete]). ManagedUpdate and ManagedLateUpdate
//! are empty and OnTriggerEnter2D's GetComponentInParent result is unused: what is left is the manager's
//! `Gears.Sort(comparer)`, whose RandomComparer draws Random.Range(-100, 100) per comparison.
use super::Game;

/// GearManagerBehaviour.Gears, a List<GearBehaviour>: `_items` (its Length, the capacity, is what the sort's depth
/// limit and bounds use) and `_size`. A slot holds a GearBehaviour's instance id.
#[derive(Clone, Debug, Default)]
pub struct GearList {
    pub items: Vec<Option<i32>>,
    pub size: usize,
}

impl GearList {
    /// List.Add: EnsureCapacity doubles from 4.
    fn add(&mut self, id: i32) {
        if self.size == self.items.len() {
            let n = if self.items.is_empty() { 4 } else { self.items.len() * 2 };
            self.items.resize(n, None);
        }
        self.items[self.size] = Some(id);
        self.size += 1;
    }

    /// List.Remove: IndexOf (UnityEngine.Object.Equals: the same instance id; null slots never match), RemoveAt.
    fn remove(&mut self, id: i32) {
        let Some(i) = self.items[..self.size].iter().position(|&x| x == Some(id)) else { return };
        self.items.copy_within(i + 1..self.size, i);
        self.size -= 1;
        self.items[self.size] = None;
    }

    /// List.Clear: the capacity stays.
    fn clear(&mut self) {
        self.items[..self.size].fill(None);
        self.size = 0;
    }
}

/// Mono's ArraySortHelper<T>.Sort(keys, 0, length, comparer) (mscorlib): IntrospectiveSort. `Err` is the
/// IndexOutOfRangeException an inconsistent comparer can cause, which Sort rethrows as an ArgumentException.
fn introspective_sort(keys: &mut [Option<i32>], length: usize, cmp: &mut impl FnMut(Option<i32>, Option<i32>) -> i32) -> Result<(), ()> {
    if length >= 2 {
        // IntrospectiveSortUtilities.FloorLog2(keys.Length)
        let depth = 2 * (usize::BITS - keys.len().leading_zeros()) as i32;
        intro_sort(keys, 0, length as isize - 1, depth, cmp)?;
    }
    Ok(())
}

/// keys[i], which throws past either end.
fn at(keys: &[Option<i32>], i: isize) -> Result<Option<i32>, ()> {
    usize::try_from(i).ok().and_then(|i| keys.get(i).copied()).ok_or(())
}

fn swap(keys: &mut [Option<i32>], i: isize, j: isize) -> Result<(), ()> {
    if i != j {
        let (a, b) = (at(keys, i)?, at(keys, j)?);
        keys[i as usize] = b;
        keys[j as usize] = a;
    }
    Ok(())
}

fn swap_if_greater(keys: &mut [Option<i32>], cmp: &mut impl FnMut(Option<i32>, Option<i32>) -> i32, a: isize, b: isize) -> Result<(), ()> {
    if a != b {
        let (x, y) = (at(keys, a)?, at(keys, b)?);
        if cmp(x, y) > 0 {
            swap(keys, a, b)?;
        }
    }
    Ok(())
}

fn intro_sort(keys: &mut [Option<i32>], lo: isize, mut hi: isize, mut depth: i32, cmp: &mut impl FnMut(Option<i32>, Option<i32>) -> i32) -> Result<(), ()> {
    while hi > lo {
        match hi - lo + 1 {
            2 => return swap_if_greater(keys, cmp, lo, hi),
            3 => {
                swap_if_greater(keys, cmp, lo, hi - 1)?;
                swap_if_greater(keys, cmp, lo, hi)?;
                return swap_if_greater(keys, cmp, hi - 1, hi);
            }
            n if n <= 16 => return insertion_sort(keys, lo, hi, cmp),
            _ => {}
        }
        if depth == 0 {
            return heapsort(keys, lo, hi, cmp);
        }
        depth -= 1;
        let p = pick_pivot_and_partition(keys, lo, hi, cmp)?;
        intro_sort(keys, p + 1, hi, depth, cmp)?;
        hi = p - 1;
    }
    Ok(())
}

fn pick_pivot_and_partition(keys: &mut [Option<i32>], lo: isize, hi: isize, cmp: &mut impl FnMut(Option<i32>, Option<i32>) -> i32) -> Result<isize, ()> {
    let mid = lo + (hi - lo) / 2;
    swap_if_greater(keys, cmp, lo, mid)?;
    swap_if_greater(keys, cmp, lo, hi)?;
    swap_if_greater(keys, cmp, mid, hi)?;
    let pivot = at(keys, mid)?;
    swap(keys, mid, hi - 1)?;
    let (mut i, mut j) = (lo, hi - 1);
    while i < j {
        loop {
            i += 1;
            if cmp(at(keys, i)?, pivot) >= 0 {
                break;
            }
        }
        loop {
            j -= 1;
            if cmp(pivot, at(keys, j)?) >= 0 {
                break;
            }
        }
        if i >= j {
            break;
        }
        swap(keys, i, j)?;
    }
    swap(keys, i, hi - 1)?;
    Ok(i)
}

fn heapsort(keys: &mut [Option<i32>], lo: isize, hi: isize, cmp: &mut impl FnMut(Option<i32>, Option<i32>) -> i32) -> Result<(), ()> {
    let n = hi - lo + 1;
    for i in (1..=n / 2).rev() {
        down_heap(keys, i, n, lo, cmp)?;
    }
    for i in (2..=n).rev() {
        swap(keys, lo, lo + i - 1)?;
        down_heap(keys, 1, i - 1, lo, cmp)?;
    }
    Ok(())
}

fn down_heap(keys: &mut [Option<i32>], mut i: isize, n: isize, lo: isize, cmp: &mut impl FnMut(Option<i32>, Option<i32>) -> i32) -> Result<(), ()> {
    let d = at(keys, lo + i - 1)?;
    while i <= n / 2 {
        let mut child = 2 * i;
        if child < n {
            let (x, y) = (at(keys, lo + child - 1)?, at(keys, lo + child)?);
            if cmp(x, y) < 0 {
                child += 1;
            }
        }
        let c = at(keys, lo + child - 1)?;
        if cmp(d, c) >= 0 {
            break;
        }
        keys[(lo + i - 1) as usize] = c;
        i = child;
    }
    keys[(lo + i - 1) as usize] = d;
    Ok(())
}

fn insertion_sort(keys: &mut [Option<i32>], lo: isize, hi: isize, cmp: &mut impl FnMut(Option<i32>, Option<i32>) -> i32) -> Result<(), ()> {
    for i in lo..hi {
        let mut j = i;
        let t = at(keys, i + 1)?;
        while j >= lo {
            let k = at(keys, j)?;
            if cmp(t, k) >= 0 {
                break;
            }
            keys[(j + 1) as usize] = k;
            j -= 1;
        }
        keys[(j + 1) as usize] = t;
    }
    Ok(())
}

impl Game {
    /// GearBehaviour.Awake
    pub fn gear_awake(&mut self, id: i32) {
        self.gears.add(id);
    }

    /// GearBehaviour.OnDestroy: onDestroy has no listeners (nothing adds one).
    pub fn gear_destroy(&mut self, id: i32) {
        self.gears.remove(id);
    }

    /// GearManagerBehaviour.Awake: Gears.Clear(); the distributor's rate only sets how often the empty
    /// ManagedUpdate runs.
    pub fn gear_manager_awake(&mut self) {
        self.gears.clear();
    }

    /// GearManagerBehaviour.Update: Gears.Sort(RandomComparer). An ArgumentException (an inconsistent comparison
    /// running off the array) ends the Update, leaving the list as the sort left it; the rest only calls the empty
    /// ManagedUpdate and ManagedLateUpdate.
    pub fn gear_manager_update(&mut self) {
        let (gears, random) = (&mut self.gears, &mut self.random);
        let _ = introspective_sort(&mut gears.items, gears.size, &mut |_, _| random.range_int(-100, 100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A consistent comparer sorts, through every path (sizes 2 and 3, insertion sort, partitions, heapsort at a
    /// zero depth limit), with null slots past the size left alone.
    #[test]
    fn introspective_sort_sorts() {
        for n in [2usize, 3, 7, 16, 17, 40, 200, 1000] {
            let mut keys: Vec<Option<i32>> = (0..n as i32).map(|i| Some((i * 7919) % 1009)).collect();
            keys.resize(n.next_power_of_two(), None);
            let mut want = keys[..n].to_vec();
            want.sort();
            let mut calls = 0;
            introspective_sort(&mut keys, n, &mut |a, b| {
                calls += 1;
                a.cmp(&b) as i32
            })
            .unwrap();
            assert_eq!(keys[..n], want[..], "{n}");
            assert!(keys[n..].iter().all(Option::is_none));
            assert!(calls >= n - 1);
        }
        let mut keys: Vec<Option<i32>> = (0..100).rev().map(Some).collect();
        heapsort(&mut keys, 0, 99, &mut |a, b| a.cmp(&b) as i32).unwrap();
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
    }

    /// List.Remove shifts down and clears the freed slot; Clear keeps the capacity.
    #[test]
    fn gear_list() {
        let mut l = GearList::default();
        for id in 1..=5 {
            l.add(id);
        }
        assert_eq!(l.items.len(), 8);
        l.remove(2);
        assert_eq!(l.items[..5], [Some(1), Some(3), Some(4), Some(5), None]);
        l.clear();
        assert_eq!((l.size, l.items.len()), (0, 8));
    }
}
