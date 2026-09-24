//! What the parameter family's messages cost, and the inbox geometry BOTH
//! service families are sized from.
//!
//! Issue 1468 -- this used to sit inside `crate::parameter_services`, and
//! phase-461 W2 had the lifecycle family read three of its numbers across the
//! module boundary. The two modules carry DIFFERENT feature gates
//! (`param-services` and `lifecycle-services`), so `lifecycle-services`
//! without `param-services` -- a legal, shipped configuration; it is what
//! `examples/native/rust/lifecycle-node` builds -- did not compile at all.
//!
//! The split is the fix and it is also the honest shape. Two things shared one
//! file: the parameter SERVICES, which need the `rcl_interfaces` message crate
//! that `param-services` brings, and the parameter family's message GEOMETRY,
//! which is arithmetic over `crate::config`'s declared shapes and
//! `nros_params`' resolved capacities and needs neither. Only the first is a
//! capability a consumer picks; the gate belongs on it alone.
//!
//! So the numbers keep meaning exactly what phase-461 W2 said they mean. The
//! lifecycle family's inbox slot IS the parameter family's, because the
//! lifecycle payloads are strictly smaller and a second knob would be a second
//! number saying less; the depth is the same 1 for the same reason. What
//! changes is only that both families can now say so.
//!
//! Not the build script, and not [`crate::config`], for the reason
//! `param_service_buffer_bytes` gives: the bound depends on the store's
//! capacities, which `nros-params`' build script resolves into `nros_params::
//! MAX_*`. A `const fn` over those consts sees every rung with no second
//! reader of any knob; a build script cannot read another crate's consts at
//! all.

use nros_params::{MAX_STRING_VALUE_LEN, SetParameterResult};

// The configured fallback, and what the executor-side assert below prices.
// Imported privately so `parameter_services`' own `pub use crate::config::
// PARAM_SERVICE_BUFFER_SIZE` stays the one public spelling of it.
use crate::config::PARAM_SERVICE_BUFFER_SIZE;

// ---------------------------------------------------------------------------
// wire caps
// ---------------------------------------------------------------------------

/// Capacity of every sequence member in the generated `rcl_interfaces` types
/// (`heapless::Vec<_, 64>`).
///
/// The streaming readers enforce it so an over-long sequence fails the request
/// exactly where the generated `Deserialize` would have — `CapacityExceeded`
/// before the handler sees anything, no reply sent. Drift guard:
/// `wire_capacities_match_the_generated_messages`.
pub(crate) const WIRE_SEQ_CAP: usize = 64;

/// Capacity of every string member in the generated `rcl_interfaces` types
/// (`heapless::String<256>`). Same enforcement, same drift guard.
pub(crate) const WIRE_STRING_CAP: usize = 256;

// phase-446 F2 -- a description longer than the describe reply's string would
// reach the wire EMPTY (`fit_or_empty`, mirroring the oracle's all-or-nothing
// `push_str`), after being stored whole and never reported. Refuse the knob
// instead, where the cap is known: this is the one crate that knows it.
const _: () = assert!(
    nros_params::MAX_PARAM_DESCRIPTION_LEN <= WIRE_STRING_CAP,
    "NROS_MAX_PARAM_DESCRIPTION_LEN is larger than the 256-byte string an \
     rcl_interfaces ParameterDescriptor carries, so the extra bytes could never \
     reach `ros2 param describe`. State 256 or less (phase-446 F2)."
);

// ---------------------------------------------------------------------------
// reason table
// ---------------------------------------------------------------------------

/// The reason string every `SetParametersResult` carries for each outcome —
/// the ONE table, used by the streaming handlers and `to_rcl_set_result`.
#[inline]
pub(crate) const fn set_result_reason(result: SetParameterResult) -> &'static str {
    match result {
        SetParameterResult::Success => "",
        SetParameterResult::ReadOnly => "Parameter is read-only",
        SetParameterResult::TypeMismatch => "Type mismatch",
        SetParameterResult::OutOfRange => "Value out of range",
        SetParameterResult::NotFound => "Parameter not found",
        SetParameterResult::StorageFull => "Parameter storage full",
        // Issue 1151 — rclrs's wording, verbatim.
        SetParameterResult::Undeclared => UNDECLARED_REASON,
        SetParameterResult::InvalidRange => "Invalid range",
        // phase-417 W4.a — an on-set-parameters callback refused it. rclcpp
        // lets the callback supply its own reason string; ours cannot, because
        // a `fn` pointer with no allocator has nowhere to put one, so the
        // reason names WHO refused rather than why.
        SetParameterResult::Rejected => "Rejected by an on-set-parameters callback",
    }
}

/// What a remote set reports for a name the node never declared (issue
/// 1151). rclrs `parameter.rs` `validate_parameter_setting`, verbatim.
pub const UNDECLARED_REASON: &str =
    "Parameter was not declared and undeclared parameters are not allowed";

// ---------------------------------------------------------------------------
// buffer bytes
// ---------------------------------------------------------------------------

/// Issue 1270 / phase-446 F3 -- how many bytes EACH half of the shared buffer
/// pair holds.
///
/// A size a rung STATES wins: `NROS_PARAM_SERVICE_BUFFER_SIZE` in the
/// environment, in Kconfig, or in the board's executor rung. Otherwise, when
/// the contract declares every node's parameters, it is the largest message
/// those declarations can put through one of the six services
/// ([`param_service_bound`]). With no declaration, or a refused one, the
/// configured default stands.
///
/// Finished HERE and not in the entity inventory, because the bound depends on
/// the store's capacities -- how long a string, an array or a description may
/// be. Those are board facts, resolved in exactly one place: `nros-params`'
/// build script, where the environment, Kconfig and the `[knobs.params]`
/// board rung meet (phase-446 W4 put the capacity refusal there for the same
/// reason). `nros_params::MAX_*` IS that resolution, so a `const fn` over them
/// sees every rung with no second reader of any knob. The inventory carries
/// the half only the contract decides (`DECLARED_PARAM_SERVICE_SHAPES`), and
/// the formula sits beside the serializers it bounds, where the test that
/// serializes the worst messages holds the two together.
///
/// A function rather than a second constant because the buffers are sized at
/// run time (`ParamServiceBuffers::with_capacity`, in `crate::parameter_services`).
#[inline]
pub(crate) const fn param_service_buffer_bytes() -> usize {
    match crate::config::DECLARED_PARAM_SERVICE_SHAPES {
        Some(shapes) if !crate::config::PARAM_SERVICE_BUFFER_STATED => {
            param_service_bound(shapes, ParamWireCaps::THIS_BUILD).total()
        }
        _ => PARAM_SERVICE_BUFFER_SIZE,
    }
}

// ---------------------------------------------------------------------------
// the bound + the inbox
// ---------------------------------------------------------------------------

// -- phase-446 F3: the bound ------------------------------------------------
//
// Field indices of one `DECLARED_PARAM_SERVICE_SHAPES` row, in the order
// `ParamServiceShape::token` (nros-cli-core) writes them.
pub(crate) const SHAPE_PARAMS: usize = 0;
pub(crate) const SHAPE_NAME_BYTES: usize = 1;
pub(crate) const SHAPE_PREFIXES: usize = 2;
pub(crate) const SHAPE_PREFIX_BYTES: usize = 3;
pub(crate) const SHAPE_STRINGS: usize = 4;
pub(crate) const SHAPE_BYTE_ARRAYS: usize = 5;
pub(crate) const SHAPE_BOOL_ARRAYS: usize = 6;
pub(crate) const SHAPE_WORD_ARRAYS: usize = 7;
pub(crate) const SHAPE_STRING_ARRAYS: usize = 8;

/// The capacities a value or a descriptor reaches the wire at: the store's,
/// clamped to the `rcl_interfaces` message's own caps. A stored value past
/// those replies NOT_SET (`value_fits_wire`), and a request past them is
/// refused before any handler runs (`read_wire_string`, `read_wire_seq_len`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParamWireCaps {
    pub(crate) string: usize,
    pub(crate) array: usize,
    pub(crate) byte_array: usize,
    pub(crate) description: usize,
    /// phase-417 W4.a -- `additional_constraints`' own capacity. Default 0, so
    /// an image that states nothing adds nothing to the describe reply.
    pub(crate) constraints: usize,
}

const fn min_usize(a: usize, b: usize) -> usize {
    if a < b { a } else { b }
}

impl ParamWireCaps {
    /// This build's store capacities -- `nros_params::MAX_*`, every rung
    /// resolved -- at the wire caps. The description needs no clamp: F2's
    /// compile-time assertion above keeps it within [`WIRE_STRING_CAP`].
    pub(crate) const THIS_BUILD: Self = Self {
        string: min_usize(MAX_STRING_VALUE_LEN, WIRE_STRING_CAP),
        array: min_usize(nros_params::MAX_ARRAY_LEN, WIRE_SEQ_CAP),
        byte_array: min_usize(nros_params::MAX_BYTE_ARRAY_LEN, WIRE_SEQ_CAP),
        description: nros_params::MAX_PARAM_DESCRIPTION_LEN,
        // Clamped like the value strings: a board may state more than a wire
        // string can carry, and the reply is bounded by what CDR can hold.
        constraints: min_usize(nros_params::MAX_PARAM_CONSTRAINTS_LEN, WIRE_STRING_CAP),
    };
}

// CDR costs, XCDR1: the service writer emits it (`CdrWriter::new_with_header`
// in `handle_request_raw`), and it pads more than XCDR2 -- 8-byte fields align
// to 8, not 4 -- so a bound on it bounds a request in either.
/// The 4-byte encapsulation header both halves begin with.
const CDR_HEADER: usize = 4;
/// A sequence length: up to 3 bytes of padding to 4, then a `u32`.
const CDR_SEQ: usize = 3 + 4;
/// A string beyond its bytes: up to 3 bytes of padding, the `u32` length
/// (which counts the NUL), and the NUL.
const CDR_STR: usize = 3 + 4 + 1;
/// An 8-byte field after anything: up to 7 bytes of padding, then 8.
const CDR_WORD: usize = 7 + 8;
/// One `ParameterValue` with no data in it, field for field as
/// `write_parameter_value` writes it: `type` and `bool_value` (1 + 1), an
/// `integer_value` (padding + 8), a `double_value` (8, already aligned), an
/// empty `string_value`, and five empty sequences.
///
/// Summing the per-field worsts (`1 + 1 + CDR_WORD + 8 + CDR_STR + 5*CDR_SEQ`
/// = 68) is 15 bytes loose, because those paddings are NOT independent: a
/// start that costs the `integer_value` its full 7 bytes of alignment leaves
/// the `string_value` and the sequences already aligned, and vice versa. 53
/// is the worst over all eight start alignments -- reached at an offset of 7
/// mod 8, where `integer_value` pads to +9 and the trailing `align(4)` still
/// costs 3 -- and a value's DATA is added on top of it by [`node_bound`],
/// which is exact because that trailing `align(4)` is already priced at its
/// maximum here. Held by `the_worst_messages_fit_the_derived_bound`.
const CDR_VALUE_BASE: usize = 53;
/// One `ParameterDescriptor` less the bytes of its name and description, as
/// `write_descriptor` writes it: the name, `type`, the description, an empty
/// `additional_constraints`, two flags, both range sequences, and ONE range (a
/// descriptor holds at most one): padding, then three 8-byte fields.
///
/// A descriptor always STARTS 4-aligned -- every field it ends on is a
/// sequence length or an 8-byte range word -- so its own leading `align(4)` is
/// free, and the `align(4)` before the second range sequence is free too when
/// the first carried 24 bytes of 8-aligned words. That leaves four paddings
/// that can really cost, and the fields come to `4 + 1` (name), `1` (type),
/// `3 + 4 + 1` (description), `3 + 4 + 1` (additional_constraints), `2`
/// (flags), `3 + 4` (a sequence length), `4 + 24` (align(8) and the three
/// range words) and `4` (the other sequence length): 64 in all. The name's
/// and the description's BYTES are added by [`node_bound`].
const CDR_DESCRIPTOR_BASE: usize = 64;

/// The one reason a failed `SetParametersAtomically` carries.
pub(crate) const ATOMIC_FAILURE_REASON: &str = "One or more parameters could not be set";

/// The wire text of `ValueConversionError::CapacityExceeded`.
///
/// Issue 1468 -- the two conversion reasons are STRINGS here and the enum that
/// returns them stays in `parameter_services`, where the conversions live.
/// The bound has to price them and the enum is part of the parameter
/// SERVICES' surface (`to_rcl_value` and its callers), so the string is what
/// crosses, not the type. `ValueConversionError::reason` returns these two and
/// nothing else, which the reason table below is the other half of.
pub(crate) const CAPACITY_EXCEEDED_REASON: &str = "Value exceeds this node's parameter capacity";

/// The wire text of `ValueConversionError::UnknownType`. See
/// [`CAPACITY_EXCEEDED_REASON`].
pub(crate) const UNKNOWN_TYPE_REASON: &str = "Unknown parameter type";

/// The longest reason a `SetParametersResult` can carry: every
/// [`set_result_reason`] arm and every `ValueConversionError::reason`.
/// Listed by variant: `set_result_reason`'s match is exhaustive, so a new
/// variant stops the build there first -- add it here beside it.
pub(crate) const LONGEST_SET_REASON: usize = {
    let reasons = [
        set_result_reason(SetParameterResult::Success),
        set_result_reason(SetParameterResult::ReadOnly),
        set_result_reason(SetParameterResult::TypeMismatch),
        set_result_reason(SetParameterResult::OutOfRange),
        set_result_reason(SetParameterResult::NotFound),
        set_result_reason(SetParameterResult::StorageFull),
        set_result_reason(SetParameterResult::Undeclared),
        set_result_reason(SetParameterResult::InvalidRange),
        set_result_reason(SetParameterResult::Rejected),
        CAPACITY_EXCEEDED_REASON,
        UNKNOWN_TYPE_REASON,
    ];
    let mut longest = 0;
    let mut i = 0;
    while i < reasons.len() {
        if reasons[i].len() > longest {
            longest = reasons[i].len();
        }
        i += 1;
    }
    longest
};

/// phase-446 F3 -- the worst case of each message the six services exchange
/// over one executor's declared parameters. Each field is a whole CDR message,
/// encapsulation header included: what one half of the buffer pair must hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ParamServiceBound {
    /// A `get_parameters` / `describe_parameters` / `get_parameter_types`
    /// request naming every declared parameter.
    pub(crate) names_request: usize,
    /// The `get_parameters` reply: every declared value at capacity.
    pub(crate) get_reply: usize,
    /// The `describe_parameters` reply: every descriptor, its description at
    /// capacity and one range.
    pub(crate) describe_reply: usize,
    /// The `get_parameter_types` reply.
    pub(crate) types_reply: usize,
    /// A `list_parameters` request filtering on every declared prefix.
    pub(crate) list_request: usize,
    /// The `list_parameters` reply: every name and every prefix.
    pub(crate) list_reply: usize,
    /// A `set_parameters` or `set_parameters_atomically` request setting every
    /// declared value at capacity (the two requests are the same message).
    pub(crate) set_request: usize,
    /// The `set_parameters` reply with the longest reason on every result, or
    /// the atomic reply, whichever is larger.
    pub(crate) set_reply: usize,
}

impl ParamServiceBound {
    /// Every field, for the field-wise operations below.
    const fn fields(&self) -> [usize; 8] {
        [
            self.names_request,
            self.get_reply,
            self.describe_reply,
            self.types_reply,
            self.list_request,
            self.list_reply,
            self.set_request,
            self.set_reply,
        ]
    }

    /// phase-461 W2 -- the largest REQUEST of all: what one slot of the
    /// family's inbox ring must hold.
    ///
    /// Three of the eight fields are requests; the other five are replies,
    /// which go out through the executor-side buffer and never touch the
    /// inbox. That asymmetry is the whole saving: on the island's worst node
    /// ([8, 170]) the largest request is a 669 B `set_parameters` while
    /// [`total`](Self::total) is dominated by `describe_reply` at capacity, so
    /// sizing the inbox by `total` would pay for bytes that can only ever
    /// travel the other way.
    pub(crate) const fn request_max(&self) -> usize {
        let mut max = self.names_request;
        if self.list_request > max {
            max = self.list_request;
        }
        if self.set_request > max {
            max = self.set_request;
        }
        max
    }

    /// The largest message of all: one half of the buffer pair.
    pub(crate) const fn total(&self) -> usize {
        let f = self.fields();
        let mut max = 0;
        let mut i = 0;
        while i < f.len() {
            if f[i] > max {
                max = f[i];
            }
            i += 1;
        }
        max
    }

    const fn max_with(self, o: Self) -> Self {
        Self {
            names_request: max_usize(self.names_request, o.names_request),
            get_reply: max_usize(self.get_reply, o.get_reply),
            describe_reply: max_usize(self.describe_reply, o.describe_reply),
            types_reply: max_usize(self.types_reply, o.types_reply),
            list_request: max_usize(self.list_request, o.list_request),
            list_reply: max_usize(self.list_reply, o.list_reply),
            set_request: max_usize(self.set_request, o.set_request),
            set_reply: max_usize(self.set_reply, o.set_reply),
        }
    }
}

const fn max_usize(a: usize, b: usize) -> usize {
    if a > b { a } else { b }
}

/// phase-446 F3 -- the largest message each service can carry for these
/// declared parameters at these capacities, over every node: each request
/// addresses ONE node's six, so the worst node decides each message.
///
/// The executor's one buffer pair serves every node it spins, and the shapes
/// are the whole image's, so an image with several executors sizes each for
/// the worst node anywhere -- an upper bound, never an under-size. A request
/// naming what the image does not declare can be larger; it is refused and
/// logged by the issue-1271 path rather than sized for.
pub(crate) const fn param_service_bound(
    shapes: &[[usize; 9]],
    caps: ParamWireCaps,
) -> ParamServiceBound {
    let mut out = ParamServiceBound {
        names_request: 0,
        get_reply: 0,
        describe_reply: 0,
        types_reply: 0,
        list_request: 0,
        list_reply: 0,
        set_request: 0,
        set_reply: 0,
    };
    let mut i = 0;
    while i < shapes.len() {
        out = out.max_with(node_bound(&shapes[i], caps));
        i += 1;
    }
    out
}

/// One node's worst messages. Every line mirrors a serializer above; the
/// serialize-and-compare test (`the_worst_messages_fit_the_derived_bound`)
/// is what keeps it honest.
const fn node_bound(s: &[usize; 9], c: ParamWireCaps) -> ParamServiceBound {
    let n = s[SHAPE_PARAMS];
    // Every declared name as a CDR string, and every prefix.
    let names = s[SHAPE_NAME_BYTES] + n * CDR_STR;
    let prefixes = s[SHAPE_PREFIX_BYTES] + s[SHAPE_PREFIXES] * CDR_STR;
    // Every declared value at capacity: the base, plus the one member each
    // type fills. Integer and double arrays pad to 8 before their words.
    let values = n * CDR_VALUE_BASE
        + s[SHAPE_STRINGS] * c.string
        + s[SHAPE_BYTE_ARRAYS] * c.byte_array
        + s[SHAPE_BOOL_ARRAYS] * c.array
        + s[SHAPE_WORD_ARRAYS] * (7 + 8 * c.array)
        + s[SHAPE_STRING_ARRAYS] * c.array * (CDR_STR + c.string);
    // The header, then the outer sequence's length.
    let head = CDR_HEADER + CDR_SEQ;
    let set_reply = head + n * (1 + CDR_STR + LONGEST_SET_REASON);
    let atomic_reply = CDR_HEADER + 1 + CDR_STR + ATOMIC_FAILURE_REASON.len();
    ParamServiceBound {
        names_request: head + names,
        get_reply: head + values,
        // phase-417 W4.a — `+ c.constraints`, because `additional_constraints`
        // can carry TEXT now. Until W4.a the store had nowhere to keep it, so
        // it was always the empty string and `CDR_DESCRIPTOR_BASE` counted its
        // four-byte length and its padding and none of its bytes. It has its
        // OWN capacity, default 0, so an image that states nothing adds
        // nothing here — which is the reason it is not the description's knob.
        describe_reply: head
            + s[SHAPE_NAME_BYTES]
            + n * (CDR_DESCRIPTOR_BASE + c.description + c.constraints),
        types_reply: head + n,
        list_request: head + prefixes + CDR_WORD,
        list_reply: head + names + CDR_SEQ + prefixes,
        set_request: head + names + values,
        set_reply: max_usize(set_reply, atomic_reply),
    }
}

// ---------------------------------------------------------------------------
// THE PARAMETER FAMILY'S OWN INBOX -- phase-461 W2, issue 1352
// ---------------------------------------------------------------------------

/// Issue 1352 -- how many bytes ONE slot of the parameter family's inbox ring
/// holds.
///
/// Two buffers stand between a `set_parameters` request and this module, and
/// until this wave only one of them was sized by anything. The executor-side
/// pair above is `param_service_buffer_bytes`, derived from the contract's
/// declared parameters since phase-446 F3. The other is the transport's INBOX:
/// the ring the read task lands a request in before any spin sees it. Every
/// queryable got the same one -- 4 slots of 1,024 B -- and a request larger
/// than a slot was not refused and not logged but dropped with a flag, which
/// is the defect half of the issue. The RAM half is the same fact from the
/// other side: on the safety island 24 of 26 queryables are parameter
/// services, and 24 x 4 x 1,024 B is most of an image that is over RAM.
///
/// Sized by the same ladder as its sibling, one rung shallower in the message:
/// a rung that STATES `NROS_PARAM_SERVICE_INBOX_BYTES` wins; otherwise the
/// contract's declared parameters bound it, through
/// `ParamServiceBound::request_max` rather than `total` (an inbox holds
/// REQUESTS; the replies go out through the buffer pair); and with no
/// declaration the executor-side fallback stands, because a family that cannot
/// price its own requests must not under-size them.
///
/// Rounded up to a multiple of 4 so the ring's slots stay word-aligned on
/// every target the tree builds for.
pub const fn param_service_inbox_bytes() -> usize {
    if crate::config::PARAM_SERVICE_INBOX_STATED {
        return crate::config::PARAM_SERVICE_INBOX_BYTES;
    }
    let derived = match crate::config::DECLARED_PARAM_SERVICE_SHAPES {
        Some(shapes) => param_service_bound(shapes, ParamWireCaps::THIS_BUILD).request_max(),
        None => param_service_buffer_bytes(),
    };
    derived.next_multiple_of(4)
}

/// phase-461 W2 -- whether [`param_service_inbox_bytes`] was DERIVED from the
/// contract rather than stated by a rung.
///
/// The const assert below is a tautology in the derived case and costs
/// nothing; it bites exactly when someone states a size the declarations
/// cannot fit into.
pub const fn param_service_inbox_derived() -> bool {
    !crate::config::PARAM_SERVICE_INBOX_STATED
}

/// Bytes one slot of the parameter family's inbox holds.
pub const PARAM_INBOX_SLOT_BYTES: usize = param_service_inbox_bytes();

/// Requests one parameter-service queryable holds before the newest is
/// dropped (`NROS_PARAM_SERVICE_INBOX_DEPTH`, default 1).
///
/// One, because parameter traffic is one request and one reply from a client
/// that waits: `ros2 param` and rclcpp's `SyncParametersClient` send and block,
/// `AsyncParametersClient` callers await a future per call, and the six
/// services of a node are polled serially in one spin
/// (`ParameterServiceServers::process`) so a slot is drained within one spin
/// period. Two clients addressing the SAME service of the SAME node inside one
/// spin period is the only case depth 1 loses, and to the loser that is a
/// timeout and a retry -- the failure mode every ROS 2 parameter client
/// already handles. No data path, no control path and no contract field
/// depends on a parameter request landing.
///
/// A DEFAULT on a knob, not a ceiling. An image that serves a parameter
/// dashboard states 2.
pub const PARAM_INBOX_DEPTH: usize = crate::config::PARAM_SERVICE_INBOX_DEPTH;

/// phase-461 W2 -- the gate.
///
/// When the size is derived this is `x >= x` and the compiler folds it away.
/// When a rung STATES `NROS_PARAM_SERVICE_INBOX_BYTES` and the contract
/// declares its parameters, a short statement fails the BUILD here rather than
/// dropping a well-formed request at run time -- which is what the flat 1,024 B
/// slot did, silently, and is the whole of issue 1352's second half.
const _: () = assert!(
    inbox_fits(PARAM_INBOX_SLOT_BYTES, DECLARED_WORST_PARAM_REQUEST),
    "NROS_PARAM_SERVICE_INBOX_BYTES is smaller than the largest parameter request the \
     contract's declared parameters can produce (a set_parameters naming every parameter \
     of the worst-declared node). Raise it, or drop the override and let it derive."
);

/// Does a slot of `slot` bytes hold a `worst`-byte request?
///
/// A `const fn` and not the comparison written out, because with no
/// declaration `DECLARED_WORST_PARAM_REQUEST` is 0 and clippy's
/// `absurd_extreme_comparisons` refuses `x >= 0` -- correctly, as an
/// expression. As a const fn the operands are parameters, and the assert still
/// fails the build for the image that has a declaration to be short of.
const fn inbox_fits(slot: usize, worst: usize) -> bool {
    slot >= worst
}

/// The largest request the contract's declared parameters can produce, or 0
/// when nothing declared them (no declaration, nothing to check against).
const DECLARED_WORST_PARAM_REQUEST: usize = match crate::config::DECLARED_PARAM_SERVICE_SHAPES {
    Some(shapes) => param_service_bound(shapes, ParamWireCaps::THIS_BUILD).request_max(),
    None => 0,
};

/// The same gate for the EXECUTOR-side pair, which until now was only a unit
/// test (`the_derived_size_fits_the_worst_messages`).
const _: () = assert!(
    inbox_fits(PARAM_SERVICE_BUFFER_SIZE, DECLARED_WORST_PARAM_REQUEST)
        || !crate::config::PARAM_SERVICE_BUFFER_STATED,
    "NROS_PARAM_SERVICE_BUFFER_SIZE is smaller than the largest parameter request the \
     contract's declared parameters can produce. Raise it, or drop the override and let \
     phase-446 F3 derive it."
);

// ---------------------------------------------------------------------------
// service sets
// ---------------------------------------------------------------------------

/// phase-426 W3 — the executor's bound on service SETS.
///
/// One set per node, and the node table is `MAX_NODES`
/// (`NROS_EXECUTOR_MAX_NODES`), so this is that bound restated for the sets
/// rather than a second, independently-tunable number. An executor with no
/// registered node still gets one set, under its own implicit primary
/// identity, which is why the floor is 1.
///
/// Issue 1468 — was `MAX_PARAM_SERVICE_SETS`. The parameter family is not the
/// only one that counts sets this way: the lifecycle family's ring count is
/// its five queryables over exactly these sets, and it always was. The old
/// name said "parameter" about a number that is the node table's, which is
/// why the lifecycle side read it under a name it had no business naming.
pub(crate) const MAX_SERVICE_SETS: usize = if crate::config::MAX_NODES == 0 {
    1
} else {
    crate::config::MAX_NODES
};
