//! What a parameter-service REQUEST costs on the wire, from a node's declared
//! parameter shape and the parameter store's capacities.
// Issue 2702.
//! The ONE formula for the request side. nros-node's
//! `param_sizing::node_bound` prices every parameter-service message with it
//! (the inbox slot and the build-time check that a stated slot fits), and
//! `nros build` prices the same requests from the contract to suggest a slot
//! size before any build script runs. Two readers, one function: a second
//! spelling of this arithmetic is how one formula's inputs drift apart.
//!
//! Pure arithmetic over counts. It reads no `MAX_*` of this crate, so a caller
//! that has not built the store (the CLI) passes the capacities it would build
//! with.

/// Capacity of every sequence member in the generated `rcl_interfaces` types
/// (`heapless::Vec<_, 64>`). nros-node holds the drift guard against the
/// generated messages (`wire_capacities_match_the_generated_messages`).
pub const WIRE_SEQ_CAP: usize = 64;

/// Capacity of every string member in the generated `rcl_interfaces` types
/// (`heapless::String<256>`). Same drift guard.
pub const WIRE_STRING_CAP: usize = 256;

/// The CDR encapsulation header.
pub const CDR_HEADER: usize = 4;
/// A sequence's length word, worst-case padded.
pub const CDR_SEQ: usize = 3 + 4;
/// A string's length word and NUL, worst-case padded.
pub const CDR_STR: usize = 3 + 4 + 1;
/// An 8-byte word, worst-case padded.
pub const CDR_WORD: usize = 7 + 8;
/// One `ParameterValue` with every member empty.
pub const CDR_VALUE_BASE: usize = 53;

/// Field indices of one node's shape: the nine counts
/// `nros_cli_core::entity_inventory::ParamServiceShape::token` writes, in order.
pub const SHAPE_PARAMS: usize = 0;
pub const SHAPE_NAME_BYTES: usize = 1;
pub const SHAPE_PREFIXES: usize = 2;
pub const SHAPE_PREFIX_BYTES: usize = 3;
pub const SHAPE_STRINGS: usize = 4;
pub const SHAPE_BYTE_ARRAYS: usize = 5;
pub const SHAPE_BOOL_ARRAYS: usize = 6;
pub const SHAPE_WORD_ARRAYS: usize = 7;
pub const SHAPE_STRING_ARRAYS: usize = 8;

/// The value capacities a request reaches the wire at: the store's, clamped
/// to the `rcl_interfaces` message's own caps ([`Self::clamped`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestCaps {
    pub string: usize,
    pub array: usize,
    pub string_array: usize,
    pub byte_array: usize,
}

const fn min_usize(a: usize, b: usize) -> usize {
    if a < b { a } else { b }
}

impl RequestCaps {
    /// The store's capacities as they reach the wire.
    pub const fn clamped(
        string: usize,
        array: usize,
        string_array: usize,
        byte_array: usize,
    ) -> Self {
        Self {
            string: min_usize(string, WIRE_STRING_CAP),
            array: min_usize(array, WIRE_SEQ_CAP),
            string_array: min_usize(string_array, WIRE_SEQ_CAP),
            byte_array: min_usize(byte_array, WIRE_SEQ_CAP),
        }
    }
}

/// The three parameter-service requests' worst size, for one node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParamRequests {
    /// `get_parameters`, `describe_parameters`, `get_parameter_types`: the
    /// node's every name.
    pub names: usize,
    /// `list_parameters`: its prefixes and a depth.
    pub list: usize,
    /// `set_parameters` (and the atomic one): every name with its value.
    pub set: usize,
}

impl ParamRequests {
    /// The largest of the three: what one inbox slot must hold.
    pub const fn max(&self) -> usize {
        let mut m = self.names;
        if self.list > m {
            m = self.list;
        }
        if self.set > m {
            m = self.set;
        }
        m
    }
}

/// The bytes a node's values occupy in a `set_parameters` request (and a
/// `get_parameters` reply), at capacities `c`.
pub const fn values_bytes(s: &[usize; 9], c: RequestCaps) -> usize {
    s[SHAPE_PARAMS] * CDR_VALUE_BASE
        + s[SHAPE_STRINGS] * c.string
        + s[SHAPE_BYTE_ARRAYS] * c.byte_array
        + s[SHAPE_BOOL_ARRAYS] * c.array
        + s[SHAPE_WORD_ARRAYS] * (7 + 8 * c.array)
        + s[SHAPE_STRING_ARRAYS] * c.string_array * (CDR_STR + c.string)
}

/// The bytes a node's names occupy in a request.
pub const fn names_bytes(s: &[usize; 9]) -> usize {
    s[SHAPE_NAME_BYTES] + s[SHAPE_PARAMS] * CDR_STR
}

/// The bytes a node's prefixes occupy in a `list_parameters` request.
pub const fn prefixes_bytes(s: &[usize; 9]) -> usize {
    s[SHAPE_PREFIX_BYTES] + s[SHAPE_PREFIXES] * CDR_STR
}

/// One node's three requests, at capacities `c`.
pub const fn param_requests(s: &[usize; 9], c: RequestCaps) -> ParamRequests {
    let head = CDR_HEADER + CDR_SEQ;
    let names = names_bytes(s);
    ParamRequests {
        names: head + names,
        list: head + prefixes_bytes(s) + CDR_WORD,
        set: head + names + values_bytes(s, c),
    }
}

/// The largest request any of `shapes`' nodes can produce at capacities `c`,
/// rounded up to the 4-byte slot alignment the inbox uses.
pub const fn worst_request_slot(shapes: &[[usize; 9]], c: RequestCaps) -> usize {
    let mut worst = 0;
    let mut i = 0;
    while i < shapes.len() {
        let m = param_requests(&shapes[i], c).max();
        if m > worst {
            worst = m;
        }
        i += 1;
    }
    worst.next_multiple_of(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A set request carries every name and every value; a string array costs
    /// its element count times one string each.
    #[test]
    fn a_set_request_carries_every_name_and_value() {
        // 2 params (names 5 + 6 bytes), one string, one string array.
        let shape = [2, 11, 1, 0, 1, 0, 0, 0, 1];
        let c = RequestCaps::clamped(16, 4, 4, 8);
        let r = param_requests(&shape, c);
        let head = CDR_HEADER + CDR_SEQ;
        assert_eq!(r.names, head + 11 + 2 * CDR_STR);
        assert_eq!(
            r.set,
            head + 11 + 2 * CDR_STR + 2 * CDR_VALUE_BASE + 16 + 4 * (CDR_STR + 16)
        );
        assert_eq!(r.max(), r.set);
        assert_eq!(worst_request_slot(&[shape], c), r.set.next_multiple_of(4));
    }

    #[test]
    fn capacities_are_clamped_to_the_wire() {
        let c = RequestCaps::clamped(10_000, 10_000, 3, 10_000);
        assert_eq!(
            c,
            RequestCaps {
                string: 256,
                array: 64,
                string_array: 3,
                byte_array: 64
            }
        );
    }
}
