//! What the event tap's callback leaves for the run-loop observer to write down: a key, in one
//! atomic word.
//!
//! Its own file for the reason `key_age.rs` has one: arithmetic over two integers, with nothing
//! macOS about it, so `backend/mod.rs` borrows it by `#[path]` under `cfg(test)` and it runs on the
//! machine it is written on. The callback must not lock, allocate or write to the log — a file
//! write in there is the delay that gets the tap switched off (`tap.rs`) — so what a line needs is
//! stored in an atomic slot there, and the observer takes it out at the next turn of the run loop
//! and writes the line.

use crate::backend::PassWhy;

/// The reasons a captured key is let through, each with a slot of its own, in the order of
/// [`slot`].
pub(crate) const REASONS: [PassWhy; 4] = [PassWhy::OutOfScope, PassWhy::MenuMode, PassWhy::MenuFlag, PassWhy::OwnWindow];

/// The slot of `why`: its place in [`REASONS`]. A new reason goes in both — the match below asks
/// for it, and the assertion after it holds the list to the slots; `tap.rs`'s slots are as many as
/// the list holds.
pub(crate) const fn slot(why: PassWhy) -> usize {
    match why {
        PassWhy::OutOfScope => 0,
        PassWhy::MenuMode => 1,
        PassWhy::MenuFlag => 2,
        PassWhy::OwnWindow => 3,
    }
}

// Each reason listed at its own slot, checked where it is built.
const _: () = {
    let mut i = 0;
    while i < REASONS.len() {
        assert!(slot(REASONS[i]) == i, "a reason not at its slot");
        i += 1;
    }
};

/// Set in every word that holds a key, so that 0 is a slot with none — vk 0 with mask 0 included.
const HELD: u64 = 1 << 40;

/// A key and its mask, as one word for a slot: never 0.
pub(crate) const fn pack(vk: u32, mask: u8) -> u64 {
    HELD | ((mask as u64) << 32) | vk as u64
}

/// The key and the mask a slot's word holds; `None` for an empty slot.
pub(crate) const fn unpack(word: u64) -> Option<(u32, u8)> {
    if word & HELD == 0 {
        return None;
    }
    Some((word as u32, (word >> 32) as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key and mask comes back as it went in, none of them as an empty slot, and an empty
    /// slot holds none.
    #[test]
    fn a_key_comes_out_of_its_slot_as_it_went_in() {
        for (vk, mask) in [(0, 0), (0x09, 0), (0x0E, 3), (0xFF, 0xFF), (u32::MAX, 1)] {
            let word = pack(vk, mask);
            assert_ne!(word, 0, "{vk:#x} {mask}");
            assert_eq!(unpack(word), Some((vk, mask)));
        }
        assert_eq!(unpack(0), None);
    }

    /// Each reason has a slot of its own, and the list holds each one at its slot.
    #[test]
    fn each_reason_has_its_own_slot() {
        for (i, why) in REASONS.iter().enumerate() {
            assert_eq!(slot(*why), i, "{why:?}");
        }
    }

    /// And every reason there is is on the list: as many as `PassWhy` has variants (backend/mod.rs),
    /// so that none is left without a slot.
    #[test]
    fn every_reason_is_listed() {
        const BACKEND: &str = include_str!("../mod.rs");
        let at = BACKEND.find("pub enum PassWhy {").unwrap();
        let body = &BACKEND[at..at + BACKEND[at..].find("\n}\n").unwrap()];
        let variants = body
            .lines()
            .skip(1)
            .map(str::trim)
            .filter(|l| l.ends_with(',') && l.starts_with(|c: char| c.is_ascii_uppercase()))
            .count();
        assert_eq!(variants, REASONS.len());
    }
}
