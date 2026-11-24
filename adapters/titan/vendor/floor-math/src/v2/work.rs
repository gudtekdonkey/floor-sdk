// SPDX-License-Identifier: Apache-2.0
#[derive(Clone, Copy, Debug)]
pub enum W {
    Isqrt,
    DivW,
    DivWide,
    DivWideBit,
    MulDiv,
}

pub const N: usize = W::MulDiv as usize + 1;

pub const NAMES: [&str; N] = ["isqrt", "div_w", "div_wide", "div_wide_bit", "mul_div"];

#[derive(Clone, Copy, Debug)]
pub enum Stage {
    Other,
}

pub const STAGES: usize = Stage::Other as usize + 1;

pub const STAGE_NAMES: [&str; STAGES] = ["other"];

const MARKS: [&str; STAGES] = ["split:other"];

pub type Counts = [[u64; N]; STAGES];

#[cfg(all(feature = "work-count", not(target_os = "solana")))]
std::thread_local! {
    static C: core::cell::RefCell<(usize, Counts)> = const { core::cell::RefCell::new((0, [[0; N]; STAGES])) };
}

#[inline(always)]
pub fn bump(w: W, n: u64) {
    #[cfg(all(feature = "work-count", not(target_os = "solana")))]
    C.with(|c| {
        let mut c = c.borrow_mut();
        let st = c.0;
        c.1[st][w as usize] += n;
    });
    #[cfg(not(all(feature = "work-count", not(target_os = "solana"))))]
    let _ = (w, n);
}

#[inline(always)]
pub fn stage(s: Stage) {
    crate::trace::mark(MARKS[s as usize]);
    #[cfg(all(feature = "work-count", not(target_os = "solana")))]
    C.with(|c| c.borrow_mut().0 = s as usize);
    #[cfg(not(all(feature = "work-count", not(target_os = "solana"))))]
    let _ = s;
}

pub fn take() -> Counts {
    #[cfg(all(feature = "work-count", not(target_os = "solana")))]
    return C.with(|c| core::mem::replace(&mut *c.borrow_mut(), (0, [[0; N]; STAGES])).1);
    #[cfg(not(all(feature = "work-count", not(target_os = "solana"))))]
    [[0; N]; STAGES]
}

pub const ON: bool = cfg!(all(feature = "work-count", not(target_os = "solana")));
