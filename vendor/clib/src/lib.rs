//! Shared math and stable sorting for the vendored native WebP library.

use std::ffi::{c_int, c_void};

// The math functions and qsort the vendored libraries call on every target (include/pngdeer_clib.h):
// one implementation, so they give the same bits everywhere.

#[no_mangle]
extern "C" fn pngdeer_log(x: f64) -> f64 {
    libm::log(x)
}

#[no_mangle]
extern "C" fn pngdeer_log2(x: f64) -> f64 {
    libm::log2(x)
}

#[no_mangle]
extern "C" fn pngdeer_log10(x: f64) -> f64 {
    libm::log10(x)
}

#[no_mangle]
extern "C" fn pngdeer_exp(x: f64) -> f64 {
    libm::exp(x)
}

#[no_mangle]
extern "C" fn pngdeer_pow(x: f64, y: f64) -> f64 {
    libm::pow(x, y)
}

#[no_mangle]
extern "C" fn pngdeer_logf(x: f32) -> f32 {
    libm::logf(x)
}

#[no_mangle]
extern "C" fn pngdeer_expf(x: f32) -> f32 {
    libm::expf(x)
}

/// qsort as a stable sort: equal elements keep their order, whatever the platform.
///
/// # Safety
/// As qsort: `base` holds `count` elements of `size` bytes, and `compare` orders them.
#[no_mangle]
unsafe extern "C" fn pngdeer_qsort(base: *mut c_void, count: usize, size: usize, compare: extern "C" fn(*const c_void, *const c_void) -> c_int) {
    if count < 2 || size == 0 {
        return;
    }
    let bytes = std::slice::from_raw_parts_mut(base.cast::<u8>(), count * size);
    let element = |i: usize| bytes.as_ptr().add(i * size).cast::<c_void>();
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by(|&a, &b| compare(element(a), element(b)).cmp(&0));
    let sorted: Vec<u8> = order.iter().flat_map(|&i| bytes[i * size..(i + 1) * size].iter().copied()).collect();
    bytes.copy_from_slice(&sorted);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qsort_is_stable() {
        extern "C" fn by_first(a: *const c_void, b: *const c_void) -> c_int {
            // SAFETY: called by pngdeer_qsort on elements of [u8; 2]
            unsafe { (*a.cast::<[u8; 2]>())[0] as c_int - (*b.cast::<[u8; 2]>())[0] as c_int }
        }
        let mut items = [[2u8, 0], [1, 1], [2, 2], [1, 3], [0, 4]];
        unsafe { pngdeer_qsort(items.as_mut_ptr().cast(), items.len(), 2, by_first) };
        assert_eq!(items, [[0, 4], [1, 1], [1, 3], [2, 0], [2, 2]]);
    }
}
