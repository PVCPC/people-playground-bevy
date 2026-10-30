//! MSVC's std::sort (introsort) as UnityPlayer instantiates it for Overlap2DQueryBase::RunQuery's
//! `_Sort_unchecked<Collider2D**, ColliderHitsByDepthComparitor>` (180be44a0): insertion sort up to 32 elements,
//! median-of-3 or Tukey ninther quicksort partitions, and a heap sort when the divisions run out. Equal elements
//! can be reordered above 32 elements, so the order is the algorithm's, not a stable sort's.

/// std::sort(v.begin(), v.end(), less): `_Sort_unchecked(first, last, last - first, pred)`.
pub fn msvc_sort<T: Copy>(v: &mut [T], less: impl Fn(&T, &T) -> bool) {
    sort_unchecked(v, 0, v.len(), v.len() as isize, &less);
}

fn sort_unchecked<T: Copy>(v: &mut [T], mut first: usize, mut last: usize, mut ideal: isize, less: &impl Fn(&T, &T) -> bool) {
    loop {
        if last - first <= 32 {
            insertion_sort(v, first, last, less);
            return;
        }
        if ideal <= 0 {
            make_heap(v, first, last, less);
            // _Sort_heap_unchecked
            let mut end = last;
            while end - first >= 2 {
                end -= 1;
                let val = v[end];
                v[end] = v[first];
                pop_heap_hole_by_index(v, first, 0, (end - first) as isize, val, less);
            }
            return;
        }
        let (mid_first, mid_second) = partition_by_median_guess(v, first, last, less);
        ideal = (ideal >> 1) + (ideal >> 2);
        if mid_first - first < last - mid_second {
            sort_unchecked(v, first, mid_first, ideal, less);
            first = mid_second;
        } else {
            sort_unchecked(v, mid_second, last, ideal, less);
            last = mid_first;
        }
    }
}

/// _Insertion_sort_unchecked: a new element goes before the first if it is less, else back past every greater one.
fn insertion_sort<T: Copy>(v: &mut [T], first: usize, last: usize, less: &impl Fn(&T, &T) -> bool) {
    if last - first < 2 {
        return;
    }
    for next in first + 1..last {
        let val = v[next];
        if less(&val, &v[first]) {
            v.copy_within(first..next, first + 1);
            v[first] = val;
        } else {
            let mut hole = next;
            while less(&val, &v[hole - 1]) {
                v[hole] = v[hole - 1];
                hole -= 1;
            }
            v[hole] = val;
        }
    }
}

/// _Med3_unchecked
fn med3<T: Copy>(v: &mut [T], a: usize, b: usize, c: usize, less: &impl Fn(&T, &T) -> bool) {
    if less(&v[b], &v[a]) {
        v.swap(a, b);
    }
    if less(&v[c], &v[b]) {
        v.swap(c, b);
        if less(&v[b], &v[a]) {
            v.swap(a, b);
        }
    }
}

/// _Partition_by_median_guess_unchecked: the (Pfirst, Plast) range equal to the pivot.
fn partition_by_median_guess<T: Copy>(v: &mut [T], first: usize, last: usize, less: &impl Fn(&T, &T) -> bool) -> (usize, usize) {
    let count = last - first;
    let mid = first + (count >> 1);
    if count - 1 > 40 {
        // Tukey's ninther
        let step = count >> 3;
        let two_step = step << 1;
        med3(v, first, first + step, first + two_step, less);
        med3(v, mid - step, mid, mid + step, less);
        med3(v, last - 1 - two_step, last - 1 - step, last - 1, less);
        med3(v, first + step, mid, last - 1 - step, less);
    } else {
        med3(v, first, mid, last - 1, less);
    }
    let (mut pfirst, mut plast) = (mid, mid + 1);
    while first < pfirst && !less(&v[pfirst - 1], &v[pfirst]) && !less(&v[pfirst], &v[pfirst - 1]) {
        pfirst -= 1;
    }
    while plast < last && !less(&v[plast], &v[pfirst]) && !less(&v[pfirst], &v[plast]) {
        plast += 1;
    }
    let (mut gfirst, mut glast) = (plast, pfirst);
    loop {
        while gfirst < last {
            if less(&v[pfirst], &v[gfirst]) {
            } else if less(&v[gfirst], &v[pfirst]) {
                break;
            } else {
                if plast != gfirst {
                    v.swap(plast, gfirst);
                }
                plast += 1;
            }
            gfirst += 1;
        }
        while first < glast {
            if less(&v[glast - 1], &v[pfirst]) {
            } else if less(&v[pfirst], &v[glast - 1]) {
                break;
            } else {
                pfirst -= 1;
                if pfirst != glast - 1 {
                    v.swap(pfirst, glast - 1);
                }
            }
            glast -= 1;
        }
        if glast == first && gfirst == last {
            return (pfirst, plast);
        }
        if glast == first {
            // no room at the bottom: rotate the pivot range upwards
            if plast != gfirst {
                v.swap(pfirst, plast);
            }
            plast += 1;
            v.swap(pfirst, gfirst);
            pfirst += 1;
            gfirst += 1;
        } else if gfirst == last {
            // no room at the top: rotate the pivot range downwards
            glast -= 1;
            pfirst -= 1;
            if glast != pfirst {
                v.swap(glast, pfirst);
            }
            plast -= 1;
            v.swap(pfirst, plast);
        } else {
            glast -= 1;
            v.swap(gfirst, glast);
            gfirst += 1;
        }
    }
}

/// _Make_heap_unchecked
fn make_heap<T: Copy>(v: &mut [T], first: usize, last: usize, less: &impl Fn(&T, &T) -> bool) {
    let bottom = (last - first) as isize;
    if bottom < 2 {
        return;
    }
    let mut hole = bottom >> 1;
    loop {
        hole -= 1;
        let val = v[first + hole as usize];
        pop_heap_hole_by_index(v, first, hole, bottom, val, less);
        if hole == 0 {
            return;
        }
    }
}

/// _Pop_heap_hole_by_index followed by _Push_heap_by_index
fn pop_heap_hole_by_index<T: Copy>(v: &mut [T], first: usize, mut hole: isize, bottom: isize, val: T, less: &impl Fn(&T, &T) -> bool) {
    let at = |i: isize| first + i as usize;
    let top = hole;
    let mut idx = hole;
    let max_sequence_non_leaf = (bottom - 1) >> 1;
    while idx < max_sequence_non_leaf {
        idx = 2 * idx + 2;
        if less(&v[at(idx)], &v[at(idx - 1)]) {
            idx -= 1;
        }
        v[at(hole)] = v[at(idx)];
        hole = idx;
    }
    if idx == max_sequence_non_leaf && bottom % 2 == 0 {
        v[at(hole)] = v[at(bottom - 1)];
        hole = bottom - 1;
    }
    // _Push_heap_by_index
    let mut idx = (hole - 1) >> 1;
    while top < hole && less(&v[at(idx)], &val) {
        v[at(hole)] = v[at(idx)];
        hole = idx;
        idx = (hole - 1) >> 1;
    }
    v[at(hole)] = val;
}

#[cfg(test)]
mod tests {
    use super::msvc_sort;

    /// Sorts by key for every size around the thresholds (32, 41, the heap sort), keeping the multiset.
    #[test]
    fn sorts_by_key() {
        let mut seed = 12345u32;
        for n in [0usize, 1, 2, 31, 32, 33, 34, 41, 42, 100, 1000] {
            let mut v: Vec<(u32, usize)> = (0..n)
                .map(|i| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    ((seed >> 24) % 7, i)
                })
                .collect();
            msvc_sort(&mut v, |a, b| a.0 < b.0);
            assert!(v.windows(2).all(|w| w[0].0 <= w[1].0));
            let mut ids: Vec<usize> = v.iter().map(|x| x.1).collect();
            ids.sort();
            assert_eq!(ids, (0..n).collect::<Vec<_>>());
        }
        // The heap path: an ideal of 0 sorts too.
        let mut v: Vec<u32> = (0..200).map(|i| (i * 37) % 101).collect();
        super::sort_unchecked(&mut v, 0, 200, 0, &|a: &u32, b: &u32| a < b);
        assert!(v.windows(2).all(|w| w[0] <= w[1]));
    }
}
