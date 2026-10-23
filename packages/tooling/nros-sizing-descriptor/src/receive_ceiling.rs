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
        (batch_unicast_size, "NROS_ZENOH_BATCH_UNICAST_SIZE")
    } else {
        (frag_max_size, "NROS_ZENOH_FRAG_MAX_SIZE")
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
        (Some(n), Some(size)) if n > 0 => Some((size, "NROS_ZENOH_SUBSCRIBER_LARGE_SIZE")),
        _ => subscriber_buffer_size.map(|size| (size, "NROS_SUBSCRIBER_BUFFER_SIZE")),
    }
}

/// `Some(message)` when the image is built to receive a sample its transport
/// drops; `None` when it fits or nothing was stated.
#[must_use]
pub fn receive_ceiling_refusal(
    need: ReceiveNeed<'_>,
    frag_max_size: usize,
    batch_unicast_size: usize,
) -> Option<String> {
    let (need, need_knob) = need?;
    let (ceiling, ceiling_knob) = receive_ceiling(frag_max_size, batch_unicast_size);
    if need <= ceiling {
        return None;
    }
    Some(format!(
        "this image is built to receive a sample zenoh-pico cannot deliver (issue 1368).\n  \
         largest receivable sample     = {need} ({need_knob})\n  \
         NROS_ZENOH_FRAG_MAX_SIZE      = {frag_max_size}\n  \
         NROS_ZENOH_BATCH_UNICAST_SIZE = {batch_unicast_size}\n  \
         receive ceiling               = {ceiling} ({ceiling_knob})\n\n\
         A fragmented sample that reassembles past NROS_ZENOH_FRAG_MAX_SIZE is dropped \
         inside the transport: it reaches no callback and raises no error, so the \
         subscription looks like one that never fires. The ceiling above is the LARGER \
         of the two, because a frame that fits one unicast batch whole is never \
         defragmented.\n\n\
         Raise NROS_ZENOH_FRAG_MAX_SIZE to at least {need} (the board's \
         `[knobs.zenoh.wire] frag_max_size`, or the env knob). If {need_knob} \
         OVER-states what this image receives, state that class yourself instead \
         (NROS_SUBSCRIBER_BUFFER_SIZE / NROS_ZENOH_SUBSCRIBER_LARGE_SIZE): a stated \
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
            msg.contains("= 1496 (NROS_SUBSCRIBER_BUFFER_SIZE)"),
            "{msg}"
        );
        assert!(msg.contains("= 1400 (NROS_ZENOH_FRAG_MAX_SIZE)"), "{msg}");
    }

    #[test]
    fn a_class_at_the_ceiling_passes() {
        let need = receive_need(None, None, Some(2048));
        assert_eq!(receive_ceiling_refusal(need, 2048, 1024), None);
    }

    #[test]
    fn the_large_class_binds_when_the_image_routes_into_it() {
        let need = receive_need(Some(2), Some(16384), Some(1024));
        assert_eq!(need, Some((16384, "NROS_ZENOH_SUBSCRIBER_LARGE_SIZE")));
        assert!(receive_ceiling_refusal(need, 2048, 1024).is_some());
        // Zero large blocks: the large class is not a receive path.
        let need = receive_need(Some(0), Some(16384), Some(1024));
        assert_eq!(need, Some((1024, "NROS_SUBSCRIBER_BUFFER_SIZE")));
        assert_eq!(receive_ceiling_refusal(need, 2048, 1024), None);
    }

    #[test]
    fn a_batch_larger_than_the_frag_buffer_is_the_ceiling() {
        let need = receive_need(None, None, Some(1400));
        assert_eq!(
            receive_ceiling(1024, 1500),
            (1500, "NROS_ZENOH_BATCH_UNICAST_SIZE")
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
