//! Sorting methods
#[forbid(deprecated_mode)];
#[forbid(deprecated_pattern)];

use vec::{len, push};
use core::cmp::{Eq, Ord};
use dvec::DVec;

export le;
export merge_sort;
export quick_sort;
export quick_sort3;
export timsort;
export Sort;

type Le<T> = pure fn(v1: &T, v2: &T) -> bool;

/**
 * Merge sort. Returns a new vector containing the sorted list.
 *
 * Has worst case O(n log n) performance, best case O(n), but
 * is not space efficient. This is a stable sort.
 */
fn merge_sort<T: Copy>(v: &[const T], le: Le<T>) -> ~[T] {
    type Slice = (uint, uint);

    return merge_sort_(v, (0u, len(v)), le);

    fn merge_sort_<T: Copy>(v: &[const T], slice: Slice, le: Le<T>)
        -> ~[T] {
        let begin = slice.first();
        let end = slice.second();

        let v_len = end - begin;
        if v_len == 0u { return ~[]; }
        if v_len == 1u { return ~[v[begin]]; }

        let mid = v_len / 2u + begin;
        let a = (begin, mid);
        let b = (mid, end);
        return merge(le, merge_sort_(v, a, le), merge_sort_(v, b, le));
    }

    fn merge<T: Copy>(le: Le<T>, a: &[T], b: &[T]) -> ~[T] {
        let mut rs = vec::with_capacity(len(a) + len(b));
        let a_len = len(a);
        let mut a_ix = 0u;
        let b_len = len(b);
        let mut b_ix = 0u;
        while a_ix < a_len && b_ix < b_len {
            if le(&a[a_ix], &b[b_ix]) {
                vec::push(rs, a[a_ix]);
                a_ix += 1u;
            } else { vec::push(rs, b[b_ix]); b_ix += 1u; }
        }
        rs = vec::append(rs, vec::slice(a, a_ix, a_len));
        rs = vec::append(rs, vec::slice(b, b_ix, b_len));
        return rs;
    }
}

fn part<T: Copy>(arr: &[mut T], left: uint,
                right: uint, pivot: uint, compare_func: Le<T>) -> uint {
    let pivot_value = arr[pivot];
    arr[pivot] <-> arr[right];
    let mut storage_index: uint = left;
    let mut i: uint = left;
    while i < right {
        if compare_func(&arr[i], &pivot_value) {
            arr[i] <-> arr[storage_index];
            storage_index += 1u;
        }
        i += 1u;
    }
    arr[storage_index] <-> arr[right];
    return storage_index;
}

fn qsort<T: Copy>(arr: &[mut T], left: uint,
             right: uint, compare_func: Le<T>) {
    if right > left {
        let pivot = (left + right) / 2u;
        let new_pivot = part::<T>(arr, left, right, pivot, compare_func);
        if new_pivot != 0u {
            // Need to do this check before recursing due to overflow
            qsort::<T>(arr, left, new_pivot - 1u, compare_func);
        }
        qsort::<T>(arr, new_pivot + 1u, right, compare_func);
    }
}

/**
 * Quicksort. Sorts a mut vector in place.
 *
 * Has worst case O(n^2) performance, average case O(n log n).
 * This is an unstable sort.
 */
fn quick_sort<T: Copy>(arr: &[mut T], compare_func: Le<T>) {
    if len::<T>(arr) == 0u { return; }
    qsort::<T>(arr, 0u, len::<T>(arr) - 1u, compare_func);
}

fn qsort3<T: Copy Ord Eq>(arr: &[mut T], left: int, right: int) {
    if right <= left { return; }
    let v: T = arr[right];
    let mut i: int = left - 1;
    let mut j: int = right;
    let mut p: int = i;
    let mut q: int = j;
    loop {
        i += 1;
        while arr[i] < v { i += 1; }
        j -= 1;
        while v < arr[j] {
            if j == left { break; }
            j -= 1;
        }
        if i >= j { break; }
        arr[i] <-> arr[j];
        if arr[i] == v {
            p += 1;
            arr[p] <-> arr[i];
        }
        if v == arr[j] {
            q -= 1;
            arr[j] <-> arr[q];
        }
    }
    arr[i] <-> arr[right];
    j = i - 1;
    i += 1;
    let mut k: int = left;
    while k < p {
        arr[k] <-> arr[j];
        k += 1;
        j -= 1;
        if k == len::<T>(arr) as int { break; }
    }
    k = right - 1;
    while k > q {
        arr[i] <-> arr[k];
        k -= 1;
        i += 1;
        if k == 0 { break; }
    }
    qsort3::<T>(arr, left, j);
    qsort3::<T>(arr, i, right);
}

/**
 * Fancy quicksort. Sorts a mut vector in place.
 *
 * Based on algorithm presented by ~[Sedgewick and Bentley]
 * (http://www.cs.princeton.edu/~rs/talks/QuicksortIsOptimal.pdf).
 * According to these slides this is the algorithm of choice for
 * 'randomly ordered keys, abstract compare' & 'small number of key values'.
 *
 * This is an unstable sort.
 */
fn quick_sort3<T: Copy Ord Eq>(arr: &[mut T]) {
    if arr.len() <= 1 { return; }
    qsort3(arr, 0, (arr.len() - 1) as int);
}

trait Sort {
    fn qsort(self);
}

impl<T: Copy Ord Eq> &[mut T] : Sort {
    fn qsort(self) { quick_sort3(self); }
}

const MIN_MERGE: uint = 64;
const MIN_GALLOP: uint = 7;
const INITIAL_TMP_STORAGE: uint = 128;

fn tim_sort<T: Ord>(array: &[mut T]) {
    let size = array.len();
    if size < 2 {
        return;
    }

    if size < MIN_MERGE {
        let init_run_len = count_run_ascending(array);
        binarysort(array, init_run_len);
        return;
    }

    let ms = &MergeState();
    let min_run = min_run_length(size);

    let mut idx = 0;
    let mut remaining = size;
    loop {
        let arr = vec::mut_view(array, idx, size);
        let mut run_len: uint = count_run_ascending(arr);

        if run_len < min_run {
            let force = if remaining <= min_run {remaining} else {min_run};
            let slice = vec::mut_view(arr, 0, force);
            binarysort(slice, run_len);
            run_len = force;
        }

        ms.push_run(idx, run_len);
        ms.merge_collapse(array);

        idx += run_len;
        remaining -= run_len;
        if remaining == 0 { break; }
    }

    ms.merge_force_collapse(array);
}

fn binarysort<T: Ord>(array: &[mut T], start: uint) {
    let size = array.len();
    let mut start = start;
    assert start <= size;

    if start == 0 { start += 1; }

    let mut pivot = ~[];
    vec::reserve(&mut pivot, 1);
    unsafe { vec::raw::set_len(pivot, 1); };

    while start < size {
        unsafe {
            let tmp_view = vec::mut_view(array, start, start+1);
            vec::raw::memmove(pivot, tmp_view, 1);
        }
        let mut left = 0;
        let mut right = start;
        assert left <= right;

        while left < right {
            let mid = (left + right) >> 1;
            if pivot[0] < array[mid] {
                right = mid;
            } else {
                left = mid+1;
            }
        }
        assert left == right;
        let mut n = start-left;

        unsafe {
            move_vec(array, left+1, array, left, n);
        }
        array[left] <-> pivot[0];
        start += 1;
    }
    unsafe { vec::raw::set_len(pivot, 0); } // Forget the boxed element
}

/// Reverse the order of elements in a slice, in place
fn reverse_slice<T>(v: &[mut T], start: uint, end:uint) {
    let mut i = start;
    while i < end / 2 {
        v[i] <-> v[end - i - 1];
        i += 1;
    }
}

pure fn min_run_length(n: uint) -> uint {
    let mut n = n;
    let mut r = 0;   // becomes 1 if any 1 bits are shifted off

    while n >= MIN_MERGE {
        r |= n & 1;
        n >>= 1;
    }
    return n + r;
}

fn count_run_ascending<T: Ord>(array: &[mut T]) -> uint {
    let size = array.len();
    assert size > 0;
    if size == 1 { return 1; }

    let mut run = 2;
    if array[1] < array[0] {
        while run < size && array[run] < array[run-1] {
            run += 1;
        }
        reverse_slice(array, 0, run);
    } else {
        while run < size && array[run] >= array[run-1] {
            run += 1;
        }
    }

    return run;
}

pure fn gallop_left<T: Ord>(key: &const T, array: &[const T],
                            hint: uint) -> uint {
    let size = array.len();
    assert size != 0 && hint < size;

    let mut last_ofs = 0;
    let mut ofs = 1;

    if *key > array[hint] {
        // Gallop right until array[hint+last_ofs] < key <= array[hint+ofs]
        let max_ofs = size - hint;
        while ofs < max_ofs && *key > array[hint+ofs] {
            last_ofs = ofs;
            ofs = (ofs << 1) + 1;
            if ofs < last_ofs { ofs = max_ofs; } // uint overflow guard
        }
        if ofs > max_ofs { ofs = max_ofs; }

        last_ofs += hint;
        ofs += hint;
    } else {
        let max_ofs = hint + 1;
        while ofs < max_ofs && *key <= array[hint-ofs] {
            last_ofs = ofs;
            ofs = (ofs << 1) + 1;
            if ofs < last_ofs { ofs = max_ofs; } // uint overflow guard
        }

        if ofs > max_ofs { ofs = max_ofs; }

        let tmp = last_ofs;
        last_ofs = hint - ofs;
        ofs = hint - tmp;
    }
    assert (last_ofs < ofs || last_ofs+1 < ofs+1) && ofs <= size;

    last_ofs += 1;
    while last_ofs < ofs {
        let m = last_ofs + ((ofs - last_ofs) >> 1);
        if *key > array[m] {
            last_ofs = m+1;
        } else {
            ofs = m;
        }
    }
    assert last_ofs == ofs;
    return ofs;
}

pure fn gallop_right<T: Ord>(key: &const T, array: &[const T],
                            hint: uint) -> uint {
    let size = array.len();
    assert size != 0 && hint < size;

    let mut last_ofs = 0;
    let mut ofs = 1;

    if *key >= array[hint] {
        // Gallop right until array[hint+last_ofs] <= key < array[hint+ofs]
        let max_ofs = size - hint;
        while ofs < max_ofs && *key >= array[hint+ofs] {
            last_ofs = ofs;
            ofs = (ofs << 1) + 1;
            if ofs < last_ofs { ofs = max_ofs; }
        }
        if ofs > max_ofs { ofs = max_ofs; }

        last_ofs += hint;
        ofs += hint;
    } else {
        // Gallop left until array[hint-ofs] <= key < array[hint-last_ofs]
        let max_ofs = hint + 1;
        while ofs < max_ofs && *key < array[hint-ofs] {
            last_ofs = ofs;
            ofs = (ofs << 1) + 1;
            if ofs < last_ofs { ofs = max_ofs; }
        }
        if ofs > max_ofs { ofs = max_ofs; }

        let tmp = last_ofs;
        last_ofs = hint - ofs;
        ofs = hint - tmp;
    }

    assert (last_ofs < ofs || last_ofs+1 < ofs+1) && ofs <= size;

    last_ofs += 1;
    while last_ofs < ofs {
        let m = last_ofs + ((ofs - last_ofs) >> 1);

        if *key >= array[m] {
            last_ofs = m + 1;
        } else {
            ofs = m;
        }
    }
    assert last_ofs == ofs;
    return ofs;
}

struct RunState {
    base: uint,
    len: uint,
}

struct MergeState<T> {
    mut min_gallop: uint,
    mut tmp: ~[T],
    runs: DVec<RunState>,

    drop {
        unsafe {
            vec::raw::set_len(self.tmp, 0);
        }
    }
}

fn MergeState<T>() -> MergeState<T> {
    let mut tmp = ~[];
    vec::reserve(&mut tmp, INITIAL_TMP_STORAGE);
    MergeState {
        min_gallop: MIN_GALLOP,
        tmp: tmp,
        runs: DVec(),
    }
}

impl<T: Ord> &MergeState<T> {
    fn push_run(run_base: uint, run_len: uint) {
        let tmp = RunState{base: run_base, len: run_len};
        self.runs.push(tmp);
    }

    fn merge_at(n: uint, array: &[mut T]) {
        let mut size = self.runs.len();
        assert size >= 2;
        assert n == size-2 || n == size-3;

        do self.runs.borrow_mut |arr| {

            let mut b1 = arr[n].base;
            let mut l1 = arr[n].len;
            let b2 = arr[n+1].base;
            let l2 = arr[n+1].len;

            assert l1 > 0 && l2 > 0;
            assert b1 + l1 == b2;

            arr[n].len = l1 + l2;
            if n == size-3 {
                arr[n+1].base = arr[n+2].base;
                arr[n+1].len = arr[n+2].len;
            }

            let slice = vec::mut_view(array, b1, b1+l1);
            let k = gallop_right(&const array[b2], slice, 0);
            b1 += k;
            l1 -= k;
            if l1 != 0 {
                let slice = vec::mut_view(array, b2, b2+l2);
                let l2 = gallop_left(
                    &const array[b1+l1-1],slice,l2-1);
                if l2 > 0 {
                    if l1 <= l2 {
                        self.merge_lo(array, b1, l1, b2, l2);
                    } else {
                        self.merge_hi(array, b1, l1, b2, l2);
                    }
                }
            }
        }
        self.runs.pop();
    }

    fn merge_lo(array: &[mut T], base1: uint, len1: uint,
                base2: uint, len2: uint) {
        assert len1 != 0 && len2 != 0 && base1+len1 == base2;

        vec::reserve(&mut self.tmp, len1);

        unsafe {
            vec::raw::set_len(self.tmp, len1);
            move_vec(self.tmp, 0, array, base1, len1);
        }

        let mut c1 = 0;
        let mut c2 = base2;
        let mut dest = base1;
        let mut len1 = len1;
        let mut len2 = len2;

        array[dest] <-> array[c2];
        dest += 1; c2 += 1; len2 -= 1;

        if len2 == 0 {
            unsafe {
                move_vec(array, dest, self.tmp, 0, len1);
                vec::raw::set_len(self.tmp, 0); // Forget the elements
            }
            return;
        }
        if len1 == 1 {
            unsafe {
                move_vec(array, dest, array, c2, len2);
                array[dest+len2] <-> self.tmp[c1];
                vec::raw::set_len(self.tmp, 0); // Forget the element
            }
            return;
        }

        let mut min_gallop = self.min_gallop;
        loop {
            let mut count1 = 0;
            let mut count2 = 0;
            let mut break_outer = false;

            loop {
                assert len1 > 1 && len2 != 0;
                if array[c2] < self.tmp[c1] {
                    array[dest] <-> array[c2];
                    dest += 1; c2 += 1; len2 -= 1;
                    count2 += 1; count1 = 0;
                    if len2 == 0 {
                        break_outer = true;
                    }
                } else {
                    array[dest] <-> self.tmp[c1];
                    dest += 1; c1 += 1; len1 -= 1;
                    count1 += 1; count2 = 0;
                    if len1 == 1 {
                        break_outer = true;
                    }
                }
                if break_outer || ((count1 | count2) >= min_gallop) {
                    break;
                }
            }
            if break_outer { break; }

            // Start to gallop
            loop {
                assert len1 > 1 && len2 != 0;

                let tmp_view = vec::mut_view(self.tmp, c1, c1+len1);
                count1 = gallop_right(&const array[c2], tmp_view, 0);
                if count1 != 0 {
                    unsafe {
                        move_vec(array, dest, self.tmp, c1, count1);
                    }
                    dest += count1; c1 += count1; len1 -= count1;
                    if len1 <= 1 { break_outer = true; break; }
                }
                array[dest] <-> array[c2];
                dest += 1; c2 += 1; len2 -= 1;
                if len2 == 0 { break_outer = true; break; }

                let tmp_view = vec::mut_view(array, c2, c2+len2);
                count2 = gallop_left(&const self.tmp[c1], tmp_view, 0);
                if count2 != 0 {
                    unsafe {
                        move_vec(array, dest, array, c2, count2);
                    }
                    dest += count2; c2 += count2; len2 -= count2;
                    if len2 == 0 { break_outer = true; break; }
                }
                array[dest] <-> self.tmp[c1];
                dest += 1; c1 += 1; len1 -= 1;
                if len1 == 1 { break_outer = true; break; }
                min_gallop -= 1;
                if !(count1 >= MIN_GALLOP || count2 >= MIN_GALLOP) {
                    break;
                }
            }
            if break_outer { break; }
            if min_gallop < 0 { min_gallop = 0; }
            min_gallop += 2; // Penalize for leaving gallop
        }
        self.min_gallop = if min_gallop < 1 { 1 } else { min_gallop };

        if len1 == 1 {
            assert len2 > 0;
            unsafe {
                move_vec(array, dest, array, c2, len2);
            }
            array[dest+len2] <-> self.tmp[c1];
        } else if len1 == 0 {
            fail fmt!("Method merge_lo violates its contract! %?", len1);
        } else {
            assert len2 == 0;
            assert len1 > 1;
            unsafe {
                move_vec(array, dest, self.tmp, c1, len1);
            }
        }
        unsafe { vec::raw::set_len(self.tmp, 0); }
    }

    fn merge_hi(array: &[mut T], base1: uint, len1: uint,
                base2: uint, len2: uint) {
        assert len1 != 1 && len2 != 0 && base1 + len1 == base2;

        vec::reserve(&mut self.tmp, len2);

        unsafe {
            vec::raw::set_len(self.tmp, len2);
            move_vec(self.tmp, 0, array, base2, len2);
        }

        let mut c1 = base1 + len1 - 1;
        let mut c2 = len2 - 1;
        let mut dest = base2 + len2 - 1;
        let mut len1 = len1;
        let mut len2 = len2;

        array[dest] <-> array[c1];
        dest -= 1; c1 -= 1; len1 -= 1;

        if len1 == 0 {
            unsafe {
                move_vec(array, dest-(len2-1), self.tmp, 0, len2);
                vec::raw::set_len(self.tmp, 0); // Forget the elements
            }
            return;
        }
        if len2 == 1 {
            dest -= len1;
            c1 -= len1;
            unsafe {
                move_vec(array, dest+1, array, c1+1, len1);
                array[dest] <-> self.tmp[c2];
                vec::raw::set_len(self.tmp, 0); // Forget the element
            }
            return;
        }

        let mut min_gallop = self.min_gallop;
        loop {
            let mut count1 = 0;
            let mut count2 = 0;
            let mut break_outer = false;

            loop {
                assert len1 != 0 && len2 > 1;
                if self.tmp[c2] < array[c1] {
                    array[dest] <-> array[c1];
                    dest -= 1; c1 -= 1; len1 -= 1;
                    count1 += 1; count2 = 0;
                    if len1 == 0 {
                        break_outer = true;
                    }
                } else {
                    array[dest] <-> self.tmp[c2];
                    dest -= 1; c2 -= 1; len2 -= 1;
                    count2 += 1; count1 = 0;
                    if len2 == 1 {
                        break_outer = true;
                    }
                }
                if break_outer || ((count1 | count2) >= min_gallop) {
                    break;
                }
            }
            if break_outer { break; }

            // Start to gallop
            loop {
                assert len2 > 1 && len1 != 0;

                let tmp_view = vec::mut_view(array, base1, base1+len1);
                count1 = len1 - gallop_right(
                    &const self.tmp[c2], tmp_view, len1-1);

                if count1 != 0 {
                    dest -= count1; c1 -= count1; len1 -= count1;
                    unsafe {
                        move_vec(array, dest+1, array, c1+1, count1);
                    }
                    if len1 == 0 { break_outer = true; break; }
                }

                array[dest] <-> self.tmp[c2];
                dest -= 1; c2 -= 1; len2 -= 1;
                if len2 == 1 { break_outer = true; break; }

                let tmp_view = vec::mut_view(self.tmp, 0, len2);
                let count2 = len2 - gallop_left(
                    &const array[c1], tmp_view, len2-1);
                if count2 != 0 {
                    dest -= count2; c2 -= count2; len2 -= count2;
                    unsafe {
                        move_vec(array, dest+1, self.tmp, c2+1, count2);
                    }
                    if len2 <= 1 { break_outer = true; break; }
                }
                array[dest] <-> array[c1];
                dest -= 1; c1 -= 1; len1 -= 1;
                if len1 == 0 { break_outer = true; break; }
                min_gallop -= 1;
                if !(count1 >= MIN_GALLOP || count2 >= MIN_GALLOP) {
                    break;
                }
            }

            if break_outer { break; }
            if min_gallop < 0 { min_gallop = 0; }
            min_gallop += 2; // Penalize for leaving gallop
        }
        self.min_gallop = if min_gallop < 1 { 1 } else { min_gallop };

        if len2 == 1 {
            assert len1 > 0;
            dest -= len1;
            c1 -= len1;
            unsafe {
                move_vec(array, dest+1, array, c1+1, len1);
            }
            array[dest] <-> self.tmp[c2];
        } else if len2 == 0 {
            fail fmt!("Method merge_hi violates its contract! %?", len2);
        } else {
            assert len1 == 0;
            assert len2 != 0;
            unsafe {
                move_vec(array, dest-(len2-1), self.tmp, 0, len2);
            }
        }
        unsafe { vec::raw::set_len(self.tmp, 0); }
    }

    fn merge_collapse(array: &[mut T]) {
        while self.runs.len() > 1 {
            let mut n = self.runs.len()-2;
            let chk = do self.runs.borrow |arr| {
                if n > 0 && arr[n-1].len <= arr[n].len + arr[n+1].len {
                    if arr[n-1].len < arr[n+1].len { n -= 1; }
                    true
                } else if arr[n].len <= arr[n+1].len {
                    true
                } else {
                    false
                }
            };
            if !chk { break; }
            self.merge_at(n, array);
        }
    }

    fn merge_force_collapse(array: &[mut T]) {
        while self.runs.len() > 1 {
            let mut n = self.runs.len()-2;
            if n > 0 {
                do self.runs.borrow |arr| {
                    if arr[n-1].len < arr[n+1].len {
                        n -= 1;
                    }
                }
            }
            self.merge_at(n, array);
        }
    }
}

// Moves elements to from dest to from
// Unsafe as it makes the from parameter invalid between s2 and s2+len
#[inline(always)]
unsafe fn move_vec<T>(dest: &[mut T], s1: uint,
                    from: &[const T], s2: uint, len: uint) {
    assert s1+len <= dest.len() && s2+len <= from.len();

    do vec::as_mut_buf(dest) |p, _len| {
        let destPtr = ptr::mut_offset(p, s1);

        do vec::as_const_buf(from) |p, _len| {
            let fromPtr = ptr::const_offset(p, s2);

            ptr::memmove(destPtr, fromPtr, len);
        }
    }
}

#[cfg(test)]
mod test_qsort3 {
    #[legacy_exports];
    fn check_sort(v1: &[mut int], v2: &[mut int]) {
        let len = vec::len::<int>(v1);
        quick_sort3::<int>(v1);
        let mut i = 0u;
        while i < len {
            log(debug, v2[i]);
            assert (v2[i] == v1[i]);
            i += 1u;
        }
    }

    #[test]
    fn test() {
        {
            let v1 = ~[mut 3, 7, 4, 5, 2, 9, 5, 8];
            let v2 = ~[mut 2, 3, 4, 5, 5, 7, 8, 9];
            check_sort(v1, v2);
        }
        {
            let v1 = ~[mut 1, 1, 1];
            let v2 = ~[mut 1, 1, 1];
            check_sort(v1, v2);
        }
        {
            let v1: ~[mut int] = ~[mut];
            let v2: ~[mut int] = ~[mut];
            check_sort(v1, v2);
        }
        { let v1 = ~[mut 9]; let v2 = ~[mut 9]; check_sort(v1, v2); }
        {
            let v1 = ~[mut 9, 3, 3, 3, 9];
            let v2 = ~[mut 3, 3, 3, 9, 9];
            check_sort(v1, v2);
        }
    }
}

#[cfg(test)]
mod test_qsort {
    #[legacy_exports];
    fn check_sort(v1: &[mut int], v2: &[mut int]) {
        let len = vec::len::<int>(v1);
        pure fn leual(a: &int, b: &int) -> bool { *a <= *b }
        quick_sort::<int>(v1, leual);
        let mut i = 0u;
        while i < len {
            log(debug, v2[i]);
            assert (v2[i] == v1[i]);
            i += 1u;
        }
    }

    #[test]
    fn test() {
        {
            let v1 = ~[mut 3, 7, 4, 5, 2, 9, 5, 8];
            let v2 = ~[mut 2, 3, 4, 5, 5, 7, 8, 9];
            check_sort(v1, v2);
        }
        {
            let v1 = ~[mut 1, 1, 1];
            let v2 = ~[mut 1, 1, 1];
            check_sort(v1, v2);
        }
        {
            let v1: ~[mut int] = ~[mut];
            let v2: ~[mut int] = ~[mut];
            check_sort(v1, v2);
        }
        { let v1 = ~[mut 9]; let v2 = ~[mut 9]; check_sort(v1, v2); }
        {
            let v1 = ~[mut 9, 3, 3, 3, 9];
            let v2 = ~[mut 3, 3, 3, 9, 9];
            check_sort(v1, v2);
        }
    }

    // Regression test for #750
    #[test]
    fn test_simple() {
        let names = ~[mut 2, 1, 3];

        let expected = ~[1, 2, 3];

        do sort::quick_sort(names) |x, y| { int::le(*x, *y) };

        let immut_names = vec::from_mut(names);

        let pairs = vec::zip(expected, immut_names);
        for vec::each(pairs) |p| {
            let (a, b) = *p;
            debug!("%d %d", a, b);
            assert (a == b);
        }
    }
}

#[cfg(test)]
mod tests {
    #[legacy_exports];

    fn check_sort(v1: &[int], v2: &[int]) {
        let len = vec::len::<int>(v1);
        pure fn le(a: &int, b: &int) -> bool { *a <= *b }
        let f = le;
        let v3 = merge_sort::<int>(v1, f);
        let mut i = 0u;
        while i < len {
            log(debug, v3[i]);
            assert (v3[i] == v2[i]);
            i += 1u;
        }
    }

    #[test]
    fn test() {
        {
            let v1 = ~[3, 7, 4, 5, 2, 9, 5, 8];
            let v2 = ~[2, 3, 4, 5, 5, 7, 8, 9];
            check_sort(v1, v2);
        }
        { let v1 = ~[1, 1, 1]; let v2 = ~[1, 1, 1]; check_sort(v1, v2); }
        { let v1:~[int] = ~[]; let v2:~[int] = ~[]; check_sort(v1, v2); }
        { let v1 = ~[9]; let v2 = ~[9]; check_sort(v1, v2); }
        {
            let v1 = ~[9, 3, 3, 3, 9];
            let v2 = ~[3, 3, 3, 9, 9];
            check_sort(v1, v2);
        }
    }

    #[test]
    fn test_merge_sort_mutable() {
        pure fn le(a: &int, b: &int) -> bool { *a <= *b }
        let v1 = ~[mut 3, 2, 1];
        let v2 = merge_sort(v1, le);
        assert v2 == ~[1, 2, 3];
    }

    #[test]
    fn test_merge_sort_stability()
    {
        // tjc: funny that we have to use parens
        pure fn ile(x: &(&static/str), y: &(&static/str)) -> bool
        {
            unsafe            // to_lower is not pure...
            {
                let x = x.to_lower();
                let y = y.to_lower();
                x <= y
            }
        }

        let names1 = ~["joe bob", "Joe Bob", "Jack Brown", "JOE Bob",
                       "Sally Mae", "JOE BOB", "Alex Andy"];
        let names2 = ~["Alex Andy", "Jack Brown", "joe bob", "Joe Bob",
                       "JOE Bob", "JOE BOB", "Sally Mae"];
        let names3 = merge_sort(names1, ile);
        assert names3 == names2;
    }
}

#[cfg(test)]
mod test_timsort {
    #[legacy_exports];
    fn check_sort(v1: &[mut int], v2: &[mut int]) {
        let len = vec::len::<int>(v1);
        timsort::<int>(v1);
        let mut i = 0u;
        while i < len {
            log(debug, v2[i]);
            assert (v2[i] == v1[i]);
            i += 1u;
        }
    }

    #[test]
    fn test() {
        {
            let v1 = ~[mut 3, 7, 4, 5, 2, 9, 5, 8];
            let v2 = ~[mut 2, 3, 4, 5, 5, 7, 8, 9];
            check_sort(v1, v2);
        }
        {
            let v1 = ~[mut 1, 1, 1];
            let v2 = ~[mut 1, 1, 1];
            check_sort(v1, v2);
        }
        {
            let v1: ~[mut int] = ~[mut];
            let v2: ~[mut int] = ~[mut];
            check_sort(v1, v2);
        }
        { let v1 = ~[mut 9]; let v2 = ~[mut 9]; check_sort(v1, v2); }
        {
            let v1 = ~[mut 9, 3, 3, 3, 9];
            let v2 = ~[mut 3, 3, 3, 9, 9];
            check_sort(v1, v2);
        }
    }
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
