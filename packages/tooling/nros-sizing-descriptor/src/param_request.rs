//! The parameter family's largest REQUEST, priced from `[params] service_shape`.
//!
//! Issue 1722. Every transport a `set_parameters` crosses has to hold it, and
//! the price is a property of the declaration and the rcl_interfaces wire
//! format, not of any one backend. It lived in `nros-rmw-zenoh/build_param_slot.rs`
//! (issue 1352), and XRCE needed the same number; a second copy of the
//! arithmetic in another build script is issue 1025's defect (one formula,
//! inputs derived twice). So the arithmetic is here, beside the reader every
//! backend already shares, and what each backend DOES with the number — its
//! floor, its refusal, which knob it names — stays in that backend's build
//! (RFC-0100 D5).
//!
//! UNFLOORED, like everything in this crate (D7): `None` means "cannot price",
//! never a default.
//!
//! `nros-node` prices the same request from the same token plus the parameter
//! store's string and array caps, and const-asserts a stated
//! `NROS_PARAM_SERVICE_INBOX_BYTES` against it. That one needs the store's caps,
//! which only `nros-params` resolves, so it cannot live here; the two agree on
//! every token this function prices (the Safety Island's worst node,
//! `8:170:1:17:0:0:0:0:0`, is 672 B on both).

/// The largest request one node's parameter services can receive, rounded up
/// to 4, from the descriptor's `[params] service_shape` token — or `None` when
/// it cannot be priced here.
///
/// The token is one `params:name_bytes:prefixes:prefix_bytes:` record plus five
/// counts of string / array parameters per node, comma-separated. Each request
/// addresses ONE node, so the worst node decides it and the maximum is taken
/// over nodes, not summed.
///
/// `None` when:
/// * the token is empty or malformed (the grammar is `nros-node`'s to refuse);
/// * any node declares a string or array parameter — its size depends on the
///   store's caps, which only `nros-params` knows. A bound right for three
///   nodes and guessed for the fourth is not a bound, so the whole image
///   abstains.
pub fn param_request_max_from(raw: &str) -> Option<usize> {
    /// The 4-byte encapsulation header both halves begin with.
    const CDR_HEADER: usize = 4;
    /// A sequence length: up to 3 bytes of padding to 4, then a `u32`.
    const CDR_SEQ: usize = 3 + 4;
    /// A string beyond its bytes: padding, the `u32` length (which counts the
    /// NUL), and the NUL.
    const CDR_STR: usize = 3 + 4 + 1;
    /// An 8-byte field after anything: up to 7 bytes of padding, then 8.
    const CDR_WORD: usize = 7 + 8;
    /// One `ParameterValue` with no data in it. The worst over all eight start
    /// alignments; `nros-node` states why it is 53 and not the 68 the per-field
    /// worsts sum to.
    const CDR_VALUE_BASE: usize = 53;

    if raw.trim().is_empty() {
        return None;
    }
    let mut worst = 0usize;
    for node in raw.split(',') {
        let f: Option<Vec<usize>> = node.split(':').map(|v| v.trim().parse().ok()).collect();
        let f = f.filter(|f| f.len() == 9)?;
        if f[4..9].iter().any(|&n| n != 0) {
            return None;
        }
        let head = CDR_HEADER + CDR_SEQ;
        let names = f[1] + f[0] * CDR_STR;
        let prefixes = f[3] + f[2] * CDR_STR;
        let values = f[0] * CDR_VALUE_BASE;
        worst = worst
            .max(head + names)
            .max(head + prefixes + CDR_WORD)
            .max(head + names + values);
    }
    Some(worst.next_multiple_of(4))
}

#[cfg(test)]
mod tests {
    use super::param_request_max_from;

    /// Issue 1352's case: one node, 25 integer parameters with 35-byte names.
    /// A `set_parameters` naming all 25 is `11 + 875 + 25 x (8 + 53)` = 2,411 B,
    /// rounded to 2,412.
    #[test]
    fn twenty_five_integers_price_the_measured_request() {
        assert_eq!(param_request_max_from("25:875:0:0:0:0:0:0:0"), Some(2412));
    }

    /// The worst node decides; nodes are not summed.
    #[test]
    fn the_worst_node_decides() {
        let one = param_request_max_from("8:170:1:17:0:0:0:0:0").unwrap();
        assert_eq!(
            one, 672,
            "the Safety Island's worst node, as nros-node prices it"
        );
        assert_eq!(
            param_request_max_from("8:170:1:17:0:0:0:0:0,1:4:0:0:0:0:0:0:0"),
            Some(one)
        );
    }

    /// A string or array parameter anywhere needs the store's caps: abstain.
    #[test]
    fn a_string_or_array_anywhere_abstains() {
        for shape in [
            "25:875:0:0:1:0:0:0:0",
            "1:4:0:0:0:1:0:0:0",
            "1:4:0:0:0:0:1:0:0",
            "1:4:0:0:0:0:0:1:0",
            "1:4:0:0:0:0:0:0:1",
            "1:4:0:0:0:0:0:0:0,1:4:0:0:1:0:0:0:0",
        ] {
            assert_eq!(param_request_max_from(shape), None, "{shape}");
        }
    }

    #[test]
    fn empty_or_malformed_is_unpriced() {
        for shape in [
            "",
            "  ",
            "25:875",
            "a:b:c:d:e:f:g:h:i",
            "1:2:3:4:5:6:7:8:9:10",
        ] {
            assert_eq!(param_request_max_from(shape), None, "{shape:?}");
        }
    }
}
