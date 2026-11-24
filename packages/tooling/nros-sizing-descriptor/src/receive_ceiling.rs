//! Issue 1368 — the largest sample an image is built to receive, against the
//! largest sample its zenoh-pico transport can deliver.
//!
//! zenoh-pico reassembles a fragmented sample into a buffer of
//! `Z_FRAG_MAX_SIZE` bytes. A sample that reassembles past it is DROPPED inside
//! the transport on a SUCCESS return (`_z_transport_unicast_handle_frag`,
//! zenoh-pico `src/transport/unicast/rx.c`; the multicast path repeats it), and
//! the log line beside the drop compiles to nothing in every build here. So the
//! subscription looks like one that never fires.
//!
//! The image's receive provisioning — the payload class it routes its largest
//! subscription into — is either derived from the message bounds or stated by
//! whoever owns the board, and the reassembly ceiling is a transport knob
//! somebody else chose. This module compares the two numbers. It does not
//! derive either one: the issue gives the reasons a frag ceiling is not sized
//! from a message bound.
//!
//! One rule with two callers, matched to where the ceiling is DECIDED:
//!
//! * the Zephyr configure (`zephyr/cmake/nros_cargo_build.cmake`,
//!   `_nros_rx_ceiling`), because on Zephyr the cmake module compiles
//!   zenoh-pico with the value it resolved there;
//! * `nros-rmw-zenoh`'s build script, for every road where `zpico-sys`
//!   compiles zenoh-pico itself and publishes the ceiling it baked on its
//!   `links = "zpico"` channel.
//!
//! cmake cannot call Rust, so the Zephyr arm restates the rule; the message
//! names the same knobs in both.
//!
//! ## Scope: derived on a hosted POSIX board, refused everywhere else
//!
//! Issue 1368 left the reassembly ceiling underived on purpose, for three
//! reasons: fragmentation is a property of the transport and the link (the
//! path MTU), not of a message; the reassembly buffer is one allocation of the
//! whole ceiling out of the platform's arena, so a derived ceiling raises the
//! RAM demand of every image; and a bound taken over the linked closure
//! over-states what an image actually receives. Those reasons hold on an
//! embedded board, which keeps the refusal above.
//!
//! On a HOSTED POSIX board (issue 2853, user ruling 2026-11-24) the second
//! reason does not apply: the buffer comes from the process heap, not a
//! sized arena. There the ceiling IS derived ([`hosted_frag_max`]), as a rung
//! below every stated value, from the image's RECEIVE class (the payload class
//! its largest subscription routes into, the same need the refusal compares),
//! never from the whole closure: max(the builtin, that class plus
//! [`REASSEMBLY_HEADROOM_BYTES`]). An image that
//! receives a type with no size bound and states no class gets a build
//! warning ([`unpriced_receive_warning`]) rather than a refusal, since nothing
//! states a number to compare.
//!
//! Both arms count zenoh's headers: a sample fits when the class plus
//! [`REASSEMBLY_HEADROOM_BYTES`] does (issue 2943). A class equal to the
//! ceiling leaves no room for them and is dropped.

use crate::{Fact, SizingDescriptor};

/// The receive side of the comparison: a STATED number and the knob it came
/// from. `None` when no rung stated one (a crate builtin), so a default is
/// never checked against a default.
pub type ReceiveNeed<'a> = Option<(usize, &'a str)>;

/// The ceiling a sample must fit: `max(frag, batch)`, and which knob that is.
///
/// zenoh-pico announces `Z_BATCH_UNICAST_SIZE` as this peer's batch size in
/// its INIT and reads into a zbuf of that size, so a sample that fits one batch
/// arrives whole and never enters the defragmentation buffer. Taking the larger
/// of the two is the direction that cannot invent a failure.
#[must_use]
pub fn receive_ceiling(frag_max_size: usize, batch_unicast_size: usize) -> (usize, &'static str) {
    if batch_unicast_size > frag_max_size {
        (batch_unicast_size, "NROS_ZENOH_BATCH_UNICAST_BYTES")
    } else {
        (frag_max_size, "NROS_ZENOH_FRAG_MAX_BYTES")
    }
}

/// The receive provisioning, chosen the way the Zephyr arm chooses it: the
/// LARGE class when the image routes into it (a stated non-zero large count and
/// a stated large size), else the small class when that was stated.
#[must_use]
pub fn receive_need<'a>(
    max_large_subscribers: Option<usize>,
    subscriber_large_size: Option<usize>,
    subscriber_buffer_size: Option<usize>,
) -> ReceiveNeed<'a> {
    match (max_large_subscribers, subscriber_large_size) {
        (Some(n), Some(size)) if n > 0 => Some((size, "NROS_ZENOH_SUBSCRIBER_LARGE_BYTES")),
        _ => subscriber_buffer_size.map(|size| (size, "NROS_SUBSCRIBER_BUFFER_BYTES")),
    }
}

/// The large-class split's builtin, bytes: a subscription whose bound exceeds
/// it routes into the large class.
pub const LARGE_THRESHOLD_DEFAULT: usize = 2048;

/// An image's stated receive need, and why its classes could not be derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatedReceive<'a> {
    /// [`receive_need`] over the stated rungs: a stated knob, else the class
    /// the descriptor derives.
    pub need: ReceiveNeed<'a>,
    /// The descriptor's refusal of the payload classes (a subscription whose
    /// type has no size bound, for one), when no class was stated either.
    pub unpriced: Option<String>,
}

/// The image's receive need, from the sizing descriptor's payload classes and
/// the stated knobs, the ONE computation the refusal and the hosted
/// derivation share. `knob(env_name, derived)` is the stated value of a knob
/// with `derived` as its lowest rung (`nros_zephyr_build::knob(..).rung(..)
/// .stated()`), so a builtin is never mistaken for a statement.
#[must_use]
pub fn stated_receive<'a>(
    desc: Option<&SizingDescriptor>,
    knob: &dyn Fn(&str, Option<usize>) -> Option<usize>,
) -> StatedReceive<'a> {
    let threshold = knob("NROS_ZENOH_SUBSCRIBER_LARGE_THRESHOLD_BYTES", None)
        .unwrap_or(LARGE_THRESHOLD_DEFAULT);
    let (classes, unpriced) = match desc.map(|d| crate::subscriber_payload_classes(d, threshold)) {
        Some(Fact::Stated(c)) => (Some(c), None),
        Some(Fact::Refused(reason)) => (None, Some(reason)),
        _ => (None, None),
    };
    let need = receive_need(
        knob(
            "NROS_ZENOH_MAX_LARGE_SUBSCRIBERS",
            classes.map(|c| c.large_count),
        ),
        knob(
            "NROS_ZENOH_SUBSCRIBER_LARGE_BYTES",
            classes.and_then(|c| (c.large_count > 0).then_some(c.large_max)),
        ),
        knob(
            "NROS_SUBSCRIBER_BUFFER_BYTES",
            classes.and_then(|c| (c.small_max > 0).then_some(c.small_max)),
        ),
    );
    StatedReceive {
        unpriced: unpriced.filter(|_| need.is_none()),
        need,
    }
}

/// Bytes a reassembled zenoh message carries beyond the sample's payload
/// class: the push header, its extensions and the RMW attachment.
///
/// MEASURED (issue 2853, Linux talker/listener over a local `rmw_zenohd`, a
/// `uint8[100000]` message whose receive class is 100,012 B): a reassembly
/// ceiling of 100,095 B delivered nothing, 100,100 B delivered every sample,
/// so the overhead was about 88 B. Rounded up for margin. The ceiling a
/// hosted image derives is the class plus this, never the class alone, which
/// drops every sample.
pub const REASSEMBLY_HEADROOM_BYTES: usize = 256;

/// The reassembly ceiling a HOSTED POSIX image derives (see the module doc):
/// `Some(need + REASSEMBLY_HEADROOM_BYTES)` when that exceeds
/// `max(frag, batch)` at the builtins, else `None` (the builtin stands).
/// Applied only when no rung stated the frag size, so a stated value always
/// wins.
#[must_use]
pub fn hosted_frag_max(
    need: ReceiveNeed<'_>,
    frag_builtin: usize,
    batch_unicast_size: usize,
) -> Option<usize> {
    let (need, _) = need?;
    let want = need.saturating_add(REASSEMBLY_HEADROOM_BYTES);
    (want > receive_ceiling(frag_builtin, batch_unicast_size).0).then_some(want)
}

/// The build warning for an image whose receive class could not be derived
/// and was not stated: a sample over `ceiling` is dropped in the transport
/// with no error.
#[must_use]
pub fn unpriced_receive_warning(reason: &str, ceiling: Option<usize>) -> String {
    let over = match ceiling {
        Some(c) => format!("larger than {c} bytes"),
        None => "larger than the transport's reassembly ceiling".to_string(),
    };
    format!(
        "this image's receive class has no size: {reason}. A received sample {over} \
         is dropped inside the transport with no error. State the largest sample this \
         image receives: `[image.<id>.knobs.zenoh] subscriber_large_bytes` and \
         `max_large_subscribers` (or NROS_ZENOH_SUBSCRIBER_LARGE_BYTES / \
         NROS_ZENOH_MAX_LARGE_SUBSCRIBERS); on a host board the build then sizes the \
         reassembly ceiling from it."
    )
}

/// `Some(message)` when the image is built to receive a sample its transport
/// drops; `None` when it fits or nothing was stated. A sample fits when it
/// and its headers ([`REASSEMBLY_HEADROOM_BYTES`]) do.
#[must_use]
pub fn receive_ceiling_refusal(
    need: ReceiveNeed<'_>,
    frag_max_size: usize,
    batch_unicast_size: usize,
) -> Option<String> {
    let (need, need_knob) = need?;
    let (ceiling, ceiling_knob) = receive_ceiling(frag_max_size, batch_unicast_size);
    // Issue 2943 -- the reassembled zenoh message is the sample plus its
    // headers, so a class that only just fits the ceiling is still dropped.
    let want = need.saturating_add(REASSEMBLY_HEADROOM_BYTES);
    if want <= ceiling {
        return None;
    }
    Some(format!(
        // issue 1368.
        "this image is built to receive a sample zenoh-pico cannot deliver.\n  \
         largest receivable sample     = {need} ({need_knob})\n  \
         plus zenoh's headers          = {want} (headroom {REASSEMBLY_HEADROOM_BYTES})\n  \
         NROS_ZENOH_FRAG_MAX_BYTES      = {frag_max_size}\n  \
         NROS_ZENOH_BATCH_UNICAST_BYTES = {batch_unicast_size}\n  \
         receive ceiling               = {ceiling} ({ceiling_knob})\n\n\
         A fragmented sample that reassembles past NROS_ZENOH_FRAG_MAX_BYTES is dropped \
         inside the transport: it reaches no callback and raises no error, so the \
         subscription looks like one that never fires. The ceiling above is the LARGER \
         of the two, because a frame that fits one unicast batch whole is never \
         defragmented.\n\n\
         Raise NROS_ZENOH_FRAG_MAX_BYTES to at least {want} (the board's \
         `[knobs.zenoh] frag_max_bytes`, or the env knob). If {need_knob} \
         OVER-states what this image receives, state that class yourself instead \
         (NROS_SUBSCRIBER_BUFFER_BYTES / NROS_ZENOH_SUBSCRIBER_LARGE_BYTES): a stated \
         value wins over the derivation."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_class_above_the_ceiling_is_refused_naming_both_knobs() {
        let need = receive_need(Some(0), None, Some(1496));
        let msg = receive_ceiling_refusal(need, 1400, 1024).expect("1496 > 1400");
        assert!(
            msg.contains("= 1496 (NROS_SUBSCRIBER_BUFFER_BYTES)"),
            "{msg}"
        );
        assert!(msg.contains("= 1400 (NROS_ZENOH_FRAG_MAX_BYTES)"), "{msg}");
    }

    /// Issue 2853 -- a hosted image whose receive class exceeds the builtin
    /// ceiling derives the ceiling from it; one that fits keeps the builtin.
    #[test]
    fn a_hosted_ceiling_rises_to_the_receive_class_only() {
        let big = receive_need(Some(1), Some(100_012), Some(1024));
        let derived = hosted_frag_max(big, 65_535, 65_535).expect("100,012 B > 64 KiB");
        // The class plus the measured header headroom, never the class alone.
        assert_eq!(derived, 100_012 + REASSEMBLY_HEADROOM_BYTES);
        let fits = receive_need(Some(1), Some(16_384), Some(1024));
        assert_eq!(hosted_frag_max(fits, 65_535, 65_535), None);
        assert_eq!(hosted_frag_max(None, 65_535, 65_535), None);
        // The derived ceiling then passes the refusal it would have failed.
        assert!(receive_ceiling_refusal(big, 65_535, 65_535).is_some());
        assert_eq!(receive_ceiling_refusal(big, derived, 65_535), None);
    }

    /// A stated knob outranks the descriptor's class, and an unpriced class
    /// warns only when nothing was stated.
    #[test]
    fn a_stated_class_wins_and_silences_the_unsized_warning() {
        let none = |_: &str, d: Option<usize>| d;
        let r = stated_receive(None, &none);
        assert_eq!(
            r,
            StatedReceive {
                need: None,
                unpriced: None
            }
        );

        let stated = |k: &str, d: Option<usize>| match k {
            "NROS_ZENOH_MAX_LARGE_SUBSCRIBERS" => Some(1),
            "NROS_ZENOH_SUBSCRIBER_LARGE_BYTES" => Some(200_000),
            _ => d,
        };
        let r = stated_receive(None, &stated);
        assert_eq!(r.need, Some((200_000, "NROS_ZENOH_SUBSCRIBER_LARGE_BYTES")));
        assert!(unpriced_receive_warning("x", Some(65_535)).contains("subscriber_large_bytes"));
    }

    #[test]
    fn a_class_and_its_headers_at_the_ceiling_pass() {
        let need = receive_need(None, None, Some(2048 - REASSEMBLY_HEADROOM_BYTES));
        assert_eq!(receive_ceiling_refusal(need, 2048, 1024), None);
    }

    /// Issue 2943 -- cmake cannot read this crate, so the Zephyr configure
    /// restates the headroom; this holds the two to one number.
    #[test]
    fn the_zephyr_arm_states_the_same_headroom() {
        let cmake = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../zephyr/cmake/nros_cargo_build.cmake");
        let text = std::fs::read_to_string(&cmake)
            .unwrap_or_else(|e| panic!("read {}: {e}", cmake.display()));
        let line = text
            .lines()
            .find(|l| l.trim_start().starts_with("set(_nros_rx_headroom "))
            .expect("the Zephyr arm states its headroom");
        assert_eq!(
            line.trim(),
            format!("set(_nros_rx_headroom {REASSEMBLY_HEADROOM_BYTES})"),
            "the Zephyr arm and REASSEMBLY_HEADROOM_BYTES disagree"
        );
    }

    /// Issue 2943 -- a class equal to the ceiling leaves no room for zenoh's
    /// headers, so every sample is dropped: refused, naming the size with
    /// the headers.
    #[test]
    fn a_class_at_the_ceiling_without_header_room_is_refused() {
        let need = receive_need(None, None, Some(2048));
        let msg = receive_ceiling_refusal(need, 2048, 1024).expect("2048 + headers > 2048");
        let want = 2048 + REASSEMBLY_HEADROOM_BYTES;
        assert!(msg.contains(&format!("at least {want}")), "{msg}");
    }

    #[test]
    fn the_large_class_binds_when_the_image_routes_into_it() {
        let need = receive_need(Some(2), Some(16384), Some(1024));
        assert_eq!(need, Some((16384, "NROS_ZENOH_SUBSCRIBER_LARGE_BYTES")));
        assert!(receive_ceiling_refusal(need, 2048, 1024).is_some());
        // Zero large blocks: the large class is not a receive path.
        let need = receive_need(Some(0), Some(16384), Some(1024));
        assert_eq!(need, Some((1024, "NROS_SUBSCRIBER_BUFFER_BYTES")));
        assert_eq!(receive_ceiling_refusal(need, 2048, 1024), None);
    }

    #[test]
    fn a_batch_larger_than_the_frag_buffer_is_the_ceiling() {
        let need = receive_need(None, None, Some(1500 - REASSEMBLY_HEADROOM_BYTES));
        assert_eq!(
            receive_ceiling(1024, 1500),
            (1500, "NROS_ZENOH_BATCH_UNICAST_BYTES")
        );
        assert_eq!(receive_ceiling_refusal(need, 1024, 1500), None);
    }

    #[test]
    fn nothing_stated_checks_nothing() {
        // The crate builtins (1024 small, 16384 large x 2) against an embedded
        // 2048 frag buffer would refuse every image that states nothing.
        assert_eq!(receive_need(None, None, None), None);
        assert_eq!(receive_ceiling_refusal(None, 2048, 1024), None);
    }
}
