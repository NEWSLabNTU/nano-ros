//! phase-457 W3 — what a REGISTRATION tells a metadata probe about itself.
//!
//! # The fact, and why nothing else can state it
//!
//! A subscription's receive slot is a function of its
//! [`RegistrationPath`][path], and the third axis of that path — *does the
//! backend dispatch this sample in place* — is **half a property of the backend
//! and half a property of the CALL SITE**. The backend half is an image fact
//! (which RMW is linked) and the sizing descriptor already reads it off the
//! `rmw` name. The call-site half is
//! `SubscriptionRequest::in_place_capable`: a delivery shape that must
//! hand its callback more than borrowed bytes cannot use the capability however
//! capable the backend is, and **eleven registration entry points answer it
//! differently**.
//!
//! Before this module the sizing descriptor composed that half from the entry's
//! LANGUAGE, as evidence. Measured against these eleven sites, the evidence is
//! wrong for nine of them: a Rust GENERIC subscription, a `.message_info()` one,
//! a `.safety()` one, a borrowed view, and four of the five C/C++ entry points
//! all buffer, while `registration_path` credited every endpoint of an image on
//! zenoh or XRCE with the in-place row. That over-statement was free while the
//! row was priced at the type's bound (issue 1319); it is an **UNDER-size** the
//! moment the row is priced at what it actually claims, which is nothing
//! (issue 1340, ~9 KiB a subscription).
//!
//! So the fact has to come per ENDPOINT. The three candidate sources, and why
//! this is the one:
//!
//! * the **contract** cannot state it — `system.contract.yaml` says what an
//!   image KEEPS, and which of eleven overloads its code calls is a property of
//!   the code, not of a declaration;
//! * a **second opinion** derived in the CLI — "what would a Rust entry do" —
//!   is what is already there and already wrong, and is issue 0196's class;
//! * the **probe** runs the code. A metadata-mode build registers its
//!   subscriptions for real against a recording backend, so the one site that
//!   knows the answer can simply say it.
//!
//! # One site, because there is only one
//!
//! phase-456 W8 collapsed twelve hand-rolled registration prologues into
//! `Executor::open_subscription`, which is *"the only place in this
//! crate that `Subscription::supports_process_in_place` is read"*. That is also
//! the only place where the call site's own answer and the topic it registers
//! are both in scope, so it is where the observation belongs — every language
//! surface reaches it, and a twelfth entry point cannot be added without
//! passing through it.
//!
//! What travels is the CALL SITE's half alone, never the conjunction the
//! executor computed. A probe links the RECORDING backend, whose in-place
//! answer is its own and not the shipped one's, so reporting
//! `SubscriptionOpen::in_place` would describe the probe rather than the image.
//! The consumer composes the two halves itself.
//!
//! # Not compiled unless a probe asks for it
//!
//! Behind `registration-observer`, which only `nros`'s `metadata-mode` turns
//! on. A shipped image carries neither the global nor the call.
//!
//! [path]: https://docs.rs/nros-sizing-descriptor

use nros_rmw::sync::Mutex;

/// What one registration reports about itself.
///
/// A struct rather than a bare `bool` because the reason a shape cannot
/// dispatch in place is worth nothing to a consumer, while the topic it
/// registered is the only key the recorder can attribute it by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedRegistration<'a> {
    /// The topic name as the registration RESOLVED it — the same string the
    /// backend was handed.
    pub topic: &'a str,
    /// The message type name.
    pub type_name: &'a str,
    /// Can THIS delivery shape dispatch out of the backend's own receive slot?
    ///
    /// The call site's half of the in-place question, verbatim from
    /// `SubscriptionRequest::in_place_capable` (private: the prologue's argument
    /// is an implementation detail of this crate, and the FACT is what crosses).
    /// NOT conjoined with the backend's answer — see the module docs.
    pub in_place_capable: bool,
    /// Issue 1648 -- did this registration claim the CLOSURE buffer (`RX_BUF`)
    /// for its receive slot, rather than a bound it stated?
    ///
    /// The buffered half of the registration path, observed the way
    /// [`Self::in_place_capable`] is: the funnel holds the slot size it is
    /// about to claim (`SubscriptionRequest::slot_bytes`), so it knows which
    /// buffered row it took -- `unbounded` (the closure buffer; a Rust generic
    /// or a type-erased registration, a C/C++ site that stated no hint) or
    /// `typed_bound` (a size the call site derived from its type). The
    /// sizing descriptor used to infer this from the entry's LANGUAGE, which
    /// issue 1319's table shows is a proxy: a C/C++ site with no hint takes
    /// `RX_BUF` exactly like the Rust generic one.
    pub claims_closure_buffer: bool,
}

/// A sink a metadata probe installs to hear every subscription registration.
pub type RegistrationObserver = fn(ObservedRegistration<'_>);

static OBSERVER: Mutex<Option<RegistrationObserver>> = Mutex::new(None);

/// Install the sink. Idempotent; the last caller wins.
///
/// Called once by `nros::metadata_mode` before the component's `configure()`
/// runs. Nothing else may call it: two observers would be two answers about one
/// registration, which is the drift this module exists to remove.
pub fn set_observer(observer: RegistrationObserver) {
    OBSERVER.with(|slot| *slot = Some(observer));
}

/// Remove the sink — for a probe that resets its recorder between components.
pub fn clear_observer() {
    OBSERVER.with(|slot| *slot = None);
}

/// Report one registration. A no-op when nothing is listening, which is every
/// build that is not a probe.
///
/// Gated exactly as `super::spin` is, which is the only caller: with no RMW seam
/// compiled in there is no registration to report, and an ungated definition
/// would be dead code in precisely the configuration that proves it unused.
#[cfg(any(has_rmw, test))]
pub(crate) fn observe(reg: ObservedRegistration<'_>) {
    // The lock is released before the sink runs: the sink records into the
    // probe's own recorder, which may itself register nothing but is not this
    // module's to reason about, and holding a global across a callback is how a
    // single-threaded probe still manages to deadlock itself.
    let sink = OBSERVER.with(|slot| *slot);
    if let Some(sink) = sink {
        sink(reg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The unlistened path is what every shipped image compiles, so it is the
    /// one that must not panic or block.
    #[test]
    fn observing_with_no_sink_is_a_no_op() {
        clear_observer();
        observe(ObservedRegistration {
            topic: "/chatter",
            type_name: "std_msgs/msg/String",
            in_place_capable: true,
            claims_closure_buffer: false,
        });
    }
}
