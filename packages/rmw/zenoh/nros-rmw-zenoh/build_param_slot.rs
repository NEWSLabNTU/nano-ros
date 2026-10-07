//! Issue 1352 -- the parameter family's inbox slot on zenoh, as a pure rule.
//!
//! Included by `build.rs` (`#[path]`) and by `tests/builtin_inbox_slot.rs`, so
//! the rule is TESTED: a build script carries no `#[cfg(test)]` module that
//! anything runs. No `std::env`, no `cargo::` output: the build script reads
//! the inputs and acts on the verdict.

/// What the builtin family's slot is, and where the number came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuiltinSlot {
    /// A rung stated `NROS_PARAM_SERVICE_INBOX_BYTES`. nros-node const-asserts
    /// the same statement against the request it prices.
    Stated(usize),
    /// The contract's declared parameters bound it (`param_request_max_from`).
    Derived(usize),
    /// Nothing declared the parameters, so nothing can price them: the floor
    /// the single table always had, never the unfloored user-service demand.
    Unpriced(usize),
    /// The contract DECLARED the parameters and this crate cannot price them
    /// (a string or array parameter needs the store's caps, which are
    /// nros-params'). Guessing here is how a declared, well-formed request was
    /// dropped while nros-node priced it correctly, so the build stops and
    /// names the knob.
    Refused(String),
}

impl BuiltinSlot {
    /// Did a statement or a declaration put a number on the family's requests?
    pub fn is_priced(&self) -> bool {
        matches!(self, BuiltinSlot::Stated(_) | BuiltinSlot::Derived(_))
    }
}

/// The user-service ring slot, once the builtin family is known.
///
/// The builtin TABLE exists only when the application's own queryable count is
/// known (`app_count_known`); otherwise the parameter and lifecycle services
/// draw user-service rings, so a priced builtin slot must reach those rings or
/// it reaches nothing. Measured before this rule: `BUILTIN_INBOX_BYTES` 4096
/// stated, no table, and a 2,411-byte `set_parameters` dropped by a 1,024 B
/// user ring (issue 1352). Raises, never lowers.
pub fn user_slot_without_builtin_table(
    app_count_known: bool,
    builtin_priced: bool,
    builtin_slot: usize,
    user_slot: usize,
) -> usize {
    if !app_count_known && builtin_priced {
        user_slot.max(builtin_slot)
    } else {
        user_slot
    }
}

/// The builtin (parameter + lifecycle) family's slot.
///
/// * `stated` -- `NROS_PARAM_SERVICE_INBOX_BYTES` from a rung, if any;
/// * `shape` -- the descriptor's `[params] service_shape` token, if stated;
/// * `user_slot` -- the user-service slot this image resolved;
/// * `floor` -- the slot the single shared table always had (1,024 B).
pub fn builtin_slot(
    stated: Option<usize>,
    shape: Option<&str>,
    user_slot: usize,
    floor: usize,
) -> BuiltinSlot {
    if let Some(n) = stated {
        return BuiltinSlot::Stated(n);
    }
    let Some(raw) = shape.map(str::trim).filter(|s| !s.is_empty()) else {
        // Issue 1352 -- W3 unfloored `user_slot` to the user services' DEMAND
        // (one `AddTwoInts` server: ~20 B). That is a fact about other
        // services, so it may raise this slot and never lower it.
        return BuiltinSlot::Unpriced(user_slot.max(floor));
    };
    // A malformed token is nros-node's to refuse (it owns the grammar and
    // panics naming it); here it only means "cannot answer".
    let well_formed = raw.split(',').all(|node| {
        let f: Vec<_> = node.split(':').collect();
        f.len() == 9 && f.iter().all(|v| v.trim().parse::<usize>().is_ok())
    });
    if !well_formed {
        return BuiltinSlot::Unpriced(user_slot.max(floor));
    }
    match param_request_max_from(raw) {
        Some(n) => BuiltinSlot::Derived(n),
        None => BuiltinSlot::Refused(format!(
            "nros-rmw-zenoh: the contract declares this image's parameters (`[params] \
             service_shape` = `{raw}`), and at least one is a string or an array. The size \
             of such a request depends on the parameter store's caps \
             (NROS_MAX_STRING_VALUE_LEN and the array caps), which only nros-params \
             resolves, so this crate cannot price the inbox slot a `set_parameters` lands \
             in.\n\
             Falling back would guess, and the guess was measured wrong: 25 declared \
             parameters, one a string, and a 25-name `set_parameters` was DROPPED by a \
             {floor}-byte slot with no reply while nros-node had priced it (issue 1352).\n\
             State NROS_PARAM_SERVICE_INBOX_BYTES (environment, Kconfig \
             CONFIG_NROS_PARAM_SERVICE_INBOX_BYTES, or the board's executor rung). \
             nros-node checks the statement against the request it prices from the same \
             declaration and the store's caps, and refuses the build if it is short."
        )),
    }
}

/// The price itself is SHARED (issue 1722): XRCE sizes its request buffer from
/// the same token, and a second copy of this arithmetic in another build script
/// is issue 1025's defect. What zenoh DOES with the price -- the floor, the
/// refusal, the knob it names -- stays here (RFC-0100 D5).
pub use nros_sizing_descriptor::param_request_max_from;
