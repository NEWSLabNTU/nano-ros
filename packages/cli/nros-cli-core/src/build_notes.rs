//! What a correct `nros build` / `nros run` prints (issue 2332).
//!
//! The Rust twin of `cmake/NanoRosDiagnostics.cmake`'s
//! `nros_message_detail`: a fact a user must act on is a warning or an error
//! and always prints; the reasoning behind a correct build (which census was
//! taken, where the sizing descriptor went, how many registrations a probe
//! observed) is DETAIL, printed only when someone asks.
//!
//! Asking is `nros build -v` / `nros run -v`, or `NROS_VERBOSE=1`, the variable
//! `-v` already hands every cmake a build runs. A verb that is NOT a driver —
//! `nros ws entity-census take` typed by hand — keeps printing its notes,
//! because there the note IS the answer the user asked for. The driver says
//! which it is with [`enter_driver`].

use std::sync::atomic::{AtomicBool, Ordering};

static IN_DRIVER: AtomicBool = AtomicBool::new(false);
static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Called by `nros build` and `nros run` before any work: from here on,
/// [`detail`] lines print only under `-v` / `NROS_VERBOSE=1`.
pub fn enter_driver(verbose: bool) {
    IN_DRIVER.store(true, Ordering::Relaxed);
    VERBOSE.store(verbose, Ordering::Relaxed);
}

/// Whether `-v` was given, or `NROS_VERBOSE=1` inherited.
#[must_use]
pub fn verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed) || std::env::var_os("NROS_VERBOSE").is_some_and(|v| v == "1")
}

/// Whether a detail line prints in this process.
#[must_use]
pub fn shows_detail() -> bool {
    decide(IN_DRIVER.load(Ordering::Relaxed), verbose())
}

fn decide(in_driver: bool, verbose: bool) -> bool {
    !in_driver || verbose
}

/// Print a detail line on stderr, if [`shows_detail`].
pub fn detail(line: impl AsRef<str>) {
    if shows_detail() {
        eprintln!("{}", line.as_ref());
    }
}

/// Print a detail line on stdout, if [`shows_detail`] — for a verb whose
/// notes are its standard output when typed by hand.
pub fn detail_out(line: impl AsRef<str>) {
    if shows_detail() {
        println!("{}", line.as_ref());
    }
}

#[cfg(test)]
mod tests {
    use super::decide;

    #[test]
    fn a_driver_hides_detail_unless_asked() {
        assert!(!decide(true, false), "a quiet build prints no detail");
        assert!(decide(true, true), "`-v` shows it");
    }

    #[test]
    fn a_verb_typed_by_hand_keeps_its_notes() {
        assert!(decide(false, false));
        assert!(decide(false, true));
    }
}
