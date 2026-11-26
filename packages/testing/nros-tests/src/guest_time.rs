//! Bounds on a QEMU guest's own clock, and the proof that a bound means
//! something (issue 2971).
//!
//! Under the board descriptors' `-icount shift=auto`, QEMU picks how much
//! guest time one instruction is worth. It raises the shift when the host
//! emulates slower than real time. So the same code read 0.79 ms of guest
//! time at a fixed `shift=5` and 1.6 to 65 ms under `shift=auto`, stepping in
//! powers of two with host load. A bound on guest time measured under
//! `shift=auto` is a bound on the host, not on the code.
//!
//! At a FIXED shift a BUSY guest's clock is its instruction count, so a bound
//! on busy work is about the code, every run. Only the `_measured` launchers
//! boot at one: `QemuProcess::start_mps2_an385_networked_measured` and
//! `start_zephyr_mps2_an385_networked_measured`. They return a
//! [`MeasuredQemu`], which hands out a [`FixedIcount`] that nothing else can
//! make. A check of a busy-work bound takes one as a parameter, so calling it
//! on a guest booted at the board's `shift=auto` does not compile:
//!
//! ```compile_fail,E0599
//! # fn check_dispatch(_icount: nros_tests::guest_time::FixedIcount, _console: &str) {}
//! # fn boot(bin: &std::path::Path) {
//! let qemu = nros_tests::qemu::QemuProcess::start_mps2_an385_networked(bin).unwrap();
//! // `QemuProcess` has no `icount()`: only a `MeasuredQemu` does.
//! check_dispatch(qemu.icount(), "");
//! # }
//! ```
//!
//! while the measured launch compiles:
//!
//! ```no_run
//! # fn check_dispatch(_icount: nros_tests::guest_time::FixedIcount, _console: &str) {}
//! # fn boot(bin: &std::path::Path) {
//! let qemu = nros_tests::qemu::QemuProcess::start_mps2_an385_networked_measured(bin).unwrap();
//! check_dispatch(qemu.icount(), "");
//! # }
//! ```
//!
//! and neither does building the witness by hand:
//!
//! ```compile_fail,E0451
//! let _ = nros_tests::guest_time::FixedIcount { shift: 5 };
//! ```
//!
//! What this does NOT cover: a span that includes `wfi`. An idle guest's clock
//! runs at wall speed even at a fixed shift (`sleep=on`, which the network
//! needs), so a sample that spans an idle carries the host's timer-wake
//! latency (`isr_wake_rtos`'s `Latency::CapOnly` has the numbers). Such a
//! bound is a cap on host jitter, whatever the shift, and is written as one.

/// Proof that a QEMU guest runs at a fixed `-icount shift=N`: a busy guest's
/// clock then counts its instructions, at `2^N` ns each.
///
/// Only [`MeasuredQemu`] hands one out, from a launch whose line carried a
/// fixed shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedIcount {
    shift: u8,
}

impl FixedIcount {
    /// The witness for an `-icount` value, if it is a fixed shift
    /// (`shift=5`, `shift=5,sleep=off`). `shift=auto`, or no shift, is none.
    pub(crate) fn of(icount: &str) -> Option<Self> {
        let shift = icount.split(',').find_map(|kv| kv.strip_prefix("shift="))?;
        shift.parse().ok().map(|shift| Self { shift })
    }

    /// The shift: one instruction is `2^shift` ns of guest time.
    #[must_use]
    pub fn shift(self) -> u8 {
        self.shift
    }

    /// The witness for a console that was RECORDED (or synthesised) at a
    /// fixed `shift`, for a fixture-free test of a check that takes one. A
    /// live guest gets its witness from its launcher, never from here.
    #[must_use]
    pub fn for_a_recorded_console(shift: u8) -> Self {
        Self { shift }
    }
}

/// A QEMU guest booted at a fixed icount, with the [`FixedIcount`] that proves it.
/// It derefs to the [`crate::qemu::QemuProcess`] it wraps.
pub struct MeasuredQemu {
    qemu: crate::qemu::QemuProcess,
    icount: FixedIcount,
}

impl MeasuredQemu {
    /// Wrap `qemu`, launched with `icount`, or refuse if `icount` is not a
    /// fixed shift. That would be a launcher bug, since only the `_measured`
    /// launchers construct one.
    pub(crate) fn new(qemu: crate::qemu::QemuProcess, icount: &str) -> crate::TestResult<Self> {
        let icount = FixedIcount::of(icount).ok_or_else(|| {
            crate::TestError::ProcessFailed(format!(
                "a measured launch must fix the shift, and `-icount {icount}` does not"
            ))
        })?;
        Ok(Self { qemu, icount })
    }

    /// The witness a guest-time bound on busy work takes.
    #[must_use]
    pub fn icount(&self) -> FixedIcount {
        self.icount
    }
}

impl std::ops::Deref for MeasuredQemu {
    type Target = crate::qemu::QemuProcess;
    fn deref(&self) -> &Self::Target {
        &self.qemu
    }
}

impl std::ops::DerefMut for MeasuredQemu {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.qemu
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_fixed_shift_is_a_witness() {
        assert_eq!(FixedIcount::of("shift=5").map(FixedIcount::shift), Some(5));
        assert_eq!(
            FixedIcount::of("shift=5,sleep=off").map(FixedIcount::shift),
            Some(5)
        );
        assert_eq!(
            FixedIcount::of("sleep=off,shift=7").map(FixedIcount::shift),
            Some(7)
        );
        assert_eq!(FixedIcount::of("shift=auto"), None);
        assert_eq!(FixedIcount::of("shift=auto,sleep=off"), None);
        assert_eq!(FixedIcount::of("sleep=off"), None);
        assert_eq!(FixedIcount::of(""), None);
    }
}
