//! Issue 2410 - a backend status on a path whose trait answer has no error
//! channel.
//!
//! `Subscription::has_data`, `ServiceTrait::has_request`,
//! `Session::next_deadline_ms`, the status-event polls and the in-place probe
//! answer `bool` / `Option`, so a failing vtable slot has to become SOME
//! answer: "no data", "no deadline", "no event", "not supported". That answer
//! is the same one a healthy, quiet backend gives, and from outside the
//! process the two are indistinguishable. Issue 2403 is what that cost: on
//! NuttX every `orb_check` failed with EBADF, `has_data` read "no data", and
//! the subscription silently never delivered.
//!
//! [`status_ok`] is the ONE spelling of that mapping. It still answers
//! `rc == NROS_RMW_RET_OK` (the trait cannot carry more), but the first error
//! a site sees is reported through `nros_log`, naming the site and the status,
//! so a broken backend says so once instead of reading as an idle one. Once
//! per site, not per call: these run on every executor pass. A hand-written
//! `rc == NROS_RMW_RET_OK && flag` is refused by
//! `scripts/check-cffi-swallowed-status.py`.

use crate::{NrosRmwRet, generated::NROS_RMW_RET_OK};
use portable_atomic::{AtomicBool, Ordering};

/// One call site that folds a status into a no-error-channel answer.
pub(crate) struct StatusSite {
    what: &'static str,
    reported: AtomicBool,
}

impl StatusSite {
    pub(crate) const fn new(what: &'static str) -> Self {
        Self {
            what,
            reported: AtomicBool::new(false),
        }
    }

    #[cfg(test)]
    pub(crate) fn has_reported(&self) -> bool {
        self.reported.load(Ordering::Relaxed)
    }
}

/// `true` iff `rc` is OK. An error is reported the first time `site` sees one.
///
/// `load` then `store`, not `swap`: thumbv6m has no compare-and-swap, and a
/// race here costs at most one duplicate line.
pub(crate) fn status_ok(rc: NrosRmwRet, site: &StatusSite) -> bool {
    if rc == NROS_RMW_RET_OK {
        return true;
    }
    if !site.reported.load(Ordering::Relaxed) {
        site.reported.store(true, Ordering::Relaxed);
        // issue 2410
        nros_log::log_error!(
            nros_log::get_logger("nros_rmw_cffi"),
            "rmw backend: {} failed (status {}); answering as if idle. \
             Reported once per site",
            site.what,
            rc
        );
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_is_true_and_not_reported() {
        static SITE: StatusSite = StatusSite::new("test ok");
        assert!(status_ok(NROS_RMW_RET_OK, &SITE));
        assert!(!SITE.has_reported());
    }

    #[test]
    fn error_is_false_and_reported_once() {
        static SITE: StatusSite = StatusSite::new("test err");
        assert!(!status_ok(crate::generated::NROS_RMW_RET_ERROR, &SITE));
        assert!(SITE.has_reported());
        // Still false on a repeat; the report latch stays set.
        assert!(!status_ok(crate::generated::NROS_RMW_RET_ERROR, &SITE));
        assert!(SITE.has_reported());
    }
}
