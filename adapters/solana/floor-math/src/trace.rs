// SPDX-License-Identifier: Apache-2.0
#[cfg(all(feature = "cu-trace", target_os = "solana"))]
extern "C" {
    fn sol_log_(message: *const u8, len: u64);
    fn sol_log_compute_units_();
}

#[inline(always)]
pub fn mark(label: &str) {
    #[cfg(all(feature = "cu-trace", target_os = "solana"))]
    unsafe {
        sol_log_(label.as_ptr(), label.len() as u64);
        sol_log_compute_units_();
    }
    #[cfg(all(feature = "op-count", not(target_os = "solana")))]
    crate::fixed::ARGS.with(|a| a.borrow_mut().push((2, label.as_ptr() as u128 | (label.len() as u128) << 64)));
    #[cfg(not(any(all(feature = "cu-trace", target_os = "solana"), all(feature = "op-count", not(target_os = "solana")))))]
    let _ = label;
}

#[cfg(all(feature = "op-count", not(target_os = "solana")))]
pub fn label_of(v: u128) -> &'static str {
    unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(v as u64 as usize as *const u8, (v >> 64) as usize)) }
}
