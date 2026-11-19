//! The stable merge sort of `ArraySortRange` (`I96`; `docs/spec/bytecode.md` 2.22,
//! `docs/design/vm.md` 3.17): the steps of the Pascal `TArray.SortItems<T>` (runs of
//! [`RUN_LENGTH`] sorted by insertion, then neighbouring runs merged with the width doubling each
//! pass, ties taking the left run first), so that it compares about as many times as the Pascal
//! sort and its result is the same (a stable sort by a total preorder has one result).

use std::cmp::Ordering;

/// The length of the runs sorted by insertion before merging (`RunLength` of `SortItems`).
const RUN_LENGTH: usize = 8;

/// Sorts `v` stably by `cmp`, with `work` (as long as `v`) as the room the passes merge into.
/// Returns the number of comparisons.
pub fn merge_sort(
    v: &mut [i64],
    work: &mut [i64],
    mut cmp: impl FnMut(i64, i64) -> Ordering,
) -> u64 {
    debug_assert_eq!(v.len(), work.len());
    let n = v.len();
    let mut count = 0u64;
    let mut greater = |a: i64, b: i64| {
        count += 1;
        cmp(a, b) == Ordering::Greater
    };
    for run in v.chunks_mut(RUN_LENGTH) {
        for i in 1..run.len() {
            let b = run[i];
            let mut j = i;
            while j > 0 && greater(run[j - 1], b) {
                run[j] = run[j - 1];
                j -= 1;
            }
            run[j] = b;
        }
    }
    // The passes go from `v` to `work` and back; `in_work` tells where the sorted runs are
    let mut in_work = false;
    let mut width = RUN_LENGTH;
    while width < n {
        let (src, dst): (&[i64], &mut [i64]) = if in_work { (work, v) } else { (v, work) };
        let mut lo = 0;
        while lo < n {
            let mid = (lo + width).min(n);
            let hi = (mid + width).min(n);
            // Already in order (one comparison, so sorted input is linear): copied as they are
            if mid < hi && greater(src[mid - 1], src[mid]) {
                let (mut l, mut r, mut i) = (lo, mid, lo);
                loop {
                    if greater(src[l], src[r]) {
                        dst[i] = src[r];
                        r += 1;
                        i += 1;
                        if r == hi {
                            break;
                        }
                    } else {
                        dst[i] = src[l];
                        l += 1;
                        i += 1;
                        if l == mid {
                            break;
                        }
                    }
                }
                // The rest of the run not used up
                dst[i..i + (mid - l)].copy_from_slice(&src[l..mid]);
                let i = i + (mid - l);
                dst[i..hi].copy_from_slice(&src[r..hi]);
            } else {
                dst[lo..hi].copy_from_slice(&src[lo..hi]);
            }
            lo = hi;
        }
        in_work = !in_work;
        width *= 2;
    }
    if in_work {
        v.copy_from_slice(work);
    }
    count
}

/// The order of two `Double`s of `KeyCompare` (`docs/spec/collections.md` 10.2): a NaN after
/// every number, two NaNs equal, and `-0.0` equal to `0.0` (unlike `f64::total_cmp`).
pub fn cmp_f64(a: f64, b: f64) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => a.partial_cmp(&b).expect("neither is a NaN"),
    }
}

/// [`cmp_f64`] for `Single`s (widening keeps the order, NaNs included).
pub fn cmp_f32(a: f32, b: f32) -> Ordering {
    cmp_f64(f64::from(a), f64::from(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(v: &[i64]) -> (Vec<i64>, u64) {
        let mut v = v.to_vec();
        let mut work = vec![0; v.len()];
        let n = merge_sort(&mut v, &mut work, |a, b| a.cmp(&b));
        (v, n)
    }

    // Every length around the run length and the merge widths sorts as `sort` does.
    #[test]
    fn merge_sort_sorts_every_length() {
        for n in 0..70 {
            let input: Vec<i64> = (0..n).map(|i| (i * 37 + 11) % 23 - 7).collect();
            let mut expect = input.clone();
            expect.sort();
            assert_eq!(sorted(&input).0, expect, "{n}");
        }
    }

    // Equal elements keep their order: sorting by the high bits leaves the low bits in order.
    #[test]
    fn merge_sort_is_stable() {
        let input: Vec<i64> = (0..100).map(|i| ((i * 7) % 5) * 1000 + i).collect();
        let mut v = input.clone();
        let mut work = vec![0; v.len()];
        merge_sort(&mut v, &mut work, |a, b| (a / 1000).cmp(&(b / 1000)));
        for w in v.windows(2) {
            assert!(w[0] / 1000 < w[1] / 1000 || w[0] < w[1], "{w:?}");
        }
    }

    // Sorted input takes n - 1 comparisons (one per run boundary check and the insertions).
    #[test]
    fn merge_sort_of_sorted_input_is_linear() {
        let input: Vec<i64> = (0..64).collect();
        assert_eq!(sorted(&input).1, 63);
    }

    // The order of KeyCompare: NaNs last and equal, -0.0 equal to 0.0.
    #[test]
    fn float_order_puts_nan_last() {
        assert_eq!(cmp_f64(f64::NAN, 1e308), Ordering::Greater);
        assert_eq!(cmp_f64(-f64::NAN, f64::NAN), Ordering::Equal);
        assert_eq!(cmp_f64(-0.0, 0.0), Ordering::Equal);
        assert_eq!(cmp_f64(f64::NEG_INFINITY, -1.0), Ordering::Less);
        assert_eq!(cmp_f32(f32::NAN, f32::INFINITY), Ordering::Greater);
        assert_eq!(cmp_f32(-0.0, 0.0), Ordering::Equal);
    }
}
