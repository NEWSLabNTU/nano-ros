//! The descriptor-driven SUBSCRIPTION half of the executor arena model —
//! `build.rs`'s, moved here so `tests/arena_model_in_place.rs` can reach it.
//!
//! Issue 1340: an `in_place` row claims the entry struct and no receive region,
//! and that saving reaches the arena only through the functions below. A build
//! script has no test harness, so before this they were checkable only by
//! building an image and reading `arena_model::REQUIRED` out of `OUT_DIR`; the
//! test now drives them over a descriptor in the shape `nros sync` writes.
//!
//! `build.rs` includes this file with `#[path]`, and so does the test, so there
//! is one copy of the arithmetic, not a mirror of it. It must stay free of
//! anything only a build script has (`env`, `OUT_DIR`): the one input that is
//! a build-environment fact, whether this build's backends claim in-place
//! dispatch, is a PARAMETER of [`descriptor_subscriptions`].

/// `TripleBuffer::SLOT_COUNT` — the slot count a `KEEP_LAST(<=1)` history uses.
pub(crate) const TRIPLE_BUFFER_SLOTS: usize = 3;

/// One SUBSCRIPTION row of the descriptor, as the arena derivation needs it —
/// phase-454 W5.a, RFC-0100 D4/D5.
///
/// The three facts a subscription's arena claim is a function of, on ONE road.
/// Before this they arrived on three env carriers —
/// `NROS_ENTITY_DECLARED_DEPTHS` (a `type|topic=depth` string),
/// `NROS_SUBSCRIBED_TYPE_BOUNDS` (a `type=bytes` string) and nothing at all for
/// the registration path — and the first two could only carry a table by
/// encoding it in a string, which is the transport RFC-0100 D4 replaces:
///
/// > It is also the only transport that can carry per-endpoint structure
/// > without encoding it in a string.
///
/// The env carriers are still read, BELOW this, and W9 retires them. Two roads
/// is what issue 1199 is about, so the descriptor is preferred wherever it can
/// answer, and an image with no descriptor derives byte-identically to before.
pub(crate) struct SubEndpoint {
    /// For diagnostics only — a refusal that does not say WHICH endpoint costs
    /// a reader the same hand-decode `check-default-gates-run-somewhere` names.
    pub(crate) topic: String,
    /// `None` when the row states none, or `keep_all` refused it.
    pub(crate) depth: Option<u32>,
    /// Issue 1319's table, applied: the bytes this row's registration actually
    /// claims per slot. `Refused`/`Absent` when the path is not known, never a
    /// guess — the two closure-buffer rows are an UNDER-size.
    pub(crate) slot: nros_sizing_descriptor::Fact<usize>,
    /// Can this row claim `RX_BUF`? True for a path that does AND for one the
    /// descriptor could not state, which price the same way.
    pub(crate) may_claim_closure: bool,
    /// phase-457 W3 (issue 1340) — does this row claim NO receive region at all?
    ///
    /// The `in_place` row: the backend dispatches out of its own ring, so the
    /// registration allocates the entry struct and nothing more. `true` only from
    /// a STATED path, which since W3 requires a per-endpoint observation of the
    /// registration — a refused path budgets the region, which is the direction
    /// that cannot ship `NodeError::BufferTooSmall`.
    pub(crate) claims_no_region: bool,
}

/// The subscription rows this image's descriptor states, or `None` when they
/// cannot be attributed per endpoint.
///
/// Two guards, and both are RFC-0100 D6's own:
///
///   * **basis** — `closure` rows describe the whole link graph, not this
///     image's endpoints. *"Never silently widens the basis … that publishes
///     the wrong row while every status still reads 'derived'."*
///   * **`undeclared_endpoints`** — a non-zero count means some endpoint that
///     could carry a per-endpoint fact carried none, so a sum over this table
///     is a sum over PART of the image. Absence is not zero, so a refused or
///     absent count refuses too.
///
/// Everything below a passed guard is per-FIELD: a row may still refuse its
/// depth (`keep_all`) or its path, and the caller decides what that costs.
///
/// `in_place_trusted` is `build.rs`'s `in_place_dispatch_trusted()` (issue
/// 1577): whether THIS build's linked backends claim in-place dispatch. `false`
/// prices an `in_place` row at a full receive region and says so.
pub(crate) fn descriptor_subscriptions(
    desc: &nros_sizing_descriptor::SizingDescriptor,
    rx_buf_size: usize,
    rx_recv_size: usize,
    in_place_trusted: bool,
) -> Option<Vec<SubEndpoint>> {
    use nros_sizing_descriptor::{Basis, EndpointKind};
    if desc.meta.basis != Basis::Contract {
        return None;
    }
    if desc.meta.undeclared_endpoints().get() != Some(0) {
        return None;
    }
    let rows: Vec<SubEndpoint> = desc
        .endpoints
        .iter()
        .filter(|ep| ep.kind == EndpointKind::Subscription)
        .map(|ep| {
            // Issue 1577 — `in_place` is a fact about the backend the descriptor
            // was PRICED for, and one descriptor can serve builds that link
            // another: a single-package leaf's fixture rows switch backend by
            // cargo feature over one image, so its cyclonedds row reads the
            // descriptor its `system.toml` (zenoh) wrote. Cyclone buffers, so
            // honouring the row would price a subscription at no receive region
            // the executor then claims — `BufferTooSmall` at registration.
            let overridden = !in_place_trusted && ep.claims_no_receive_region();
            if overridden {
                println!(
                    "cargo::warning=nros-node: subscription `{}`: the sizing descriptor says \
                     `registration_path = \"in_place\"`, but this build's backends do not \
                     claim in-place dispatch (`nros-rmw/in-place-dispatch` must be on and \
                     `nros-rmw/buffered-dispatch` off), so it is priced at a full receive \
                     region (issue 1577)",
                    ep.topic
                );
            }
            SubEndpoint {
                topic: ep.topic.clone(),
                depth: ep.depth().get(),
                slot: ep.claimed_slot_bytes(rx_buf_size, rx_recv_size),
                may_claim_closure: ep.may_claim_closure_buffer(),
                claims_no_region: ep.claims_no_receive_region() && in_place_trusted,
            }
        })
        .collect();
    Some(rows)
}

/// The slot bytes one row is priced at, and the warning that owes the reader an
/// explanation when it could not be derived.
///
/// RFC-0100 D6's second half — *"always the safe direction and always loud"* —
/// and here the safe direction is the CLOSURE buffer: `RX_BUF` is an upper
/// bound on every one of the three rows (`_RX <= RX_BUF`,
/// `min(framed(bound), RX_BUF) <= RX_BUF`, and `in_place` claims nothing at
/// all), so a row whose path is unknown is priced at the one number none of
/// them can exceed. (Five rows before phase-456 W8 collapsed the two that named
/// a caller rather than a property of the registration.)
pub(crate) fn row_slot_bytes(row: &SubEndpoint, rx_buf_size: usize) -> usize {
    let (slot, why) = row
        .slot
        .or_report("endpoint.registration_path", rx_buf_size);
    if let Some(why) = why {
        println!(
            "cargo::warning=nros-node: subscription `{}`: {why}; its receive slot is priced at \
             the closure buffer ({rx_buf_size} B), which over-states a typed registration rather \
             than under-sizing a schemaless one (issue 1319)",
            row.topic
        );
    }
    slot
}

/// The subscription half of the arena, summed from the DESCRIPTOR — phase-454
/// W5.b, closing **issue 1319**.
///
/// [`subs_arena`] prices each subscription at its type's own bound. That is the
/// right number for three of the five registration paths and 1,848 bytes per
/// subscription too small for the other two:
///
/// | path | slot |
/// | --- | --- |
/// | `typed_bound` | the type's own `_RX` — matches the model |
/// | `in_place` | no region at all — the model OVER-states (issue 1340) |
/// | **`unbounded`** | **`RX_BUF`** |
///
/// phase-456 W8 collapsed five rows to these three: two of the five named the
/// caller's LANGUAGE rather than anything about the registration, and the two
/// that took `RX_BUF` (`rust_typed_schemaless`, `c_raw_no_hint`) were one fact
/// said twice.
///
/// The last two are an UNDER-size, which lands as `NodeError::BufferTooSmall`
/// at a registration `executor::arena_oracle` passed. It is not a repair the
/// build could make before: which path a registration takes is composed from
/// the entry's LANGUAGE and whether the linked backend carries type descriptors
/// (`default_subscription_rx_bytes`'s two `cfg` arms), and a build script sees
/// neither. The descriptor carries it as an image fact (RFC-0100 D1), which is
/// issue 1319's second candidate fix; its first is a narrower repair that
/// leaves the build blind, and its third — a stated per-image margin — is
/// *"the answer that goes stale next time a path changes"*.
///
/// **Not "raise the term back to `RX_BUF`"**, which the issue rules out in the
/// same breath: that gives up issue 1255's saving on the paths where the
/// per-type bound IS what is allocated. Each row is priced at what ITS path
/// claims, so a Cyclone image keeps the bound and a C image that registers raw
/// gets the buffer it will actually ask for.
///
/// Phase-454 W5 MEASURED the table and found a fifth row: zenoh and XRCE
/// dispatch IN PLACE, so such a registration allocates no region at all (672
/// bytes of arena on `contract-monitor-sub`, against a 9,768-byte budgeted
/// region).
///
/// **phase-457 W3 takes that saving, and what made it safe is not this term.**
/// W5 left the row priced at the type's bound because the descriptor could not
/// say WHICH endpoints take it: the in-place row was credited to every endpoint
/// of an image on a zenoh or XRCE backend, while nine of the executor's eleven
/// registration entry points cannot use the capability at all. Pricing that at
/// zero would have under-sized every generic, `_info`, `_safety`, borrowed and
/// C-typed subscription — `NodeError::BufferTooSmall` at a registration the
/// oracle passed. The row is now stated only from a per-endpoint OBSERVATION of
/// the registration (`Endpoint::claims_no_receive_region`), so a row that claims
/// no region is a row somebody measured, and a row nobody measured is refused
/// and keeps its region.
///
/// `None` — keep the env road — when the table cannot answer for every
/// subscription this image declares:
///
///   * the rows are not attributable at all ([`descriptor_subscriptions`]);
///   * the row count disagrees with the declared subscription count, so the
///     table describes a different set of endpoints than the one being summed;
///   * some row states no `depth`. A receive region is sized from it and a
///     default is wrong by up to 10x in either direction, which is the same
///     refusal `set_storage_bytes` makes one layer up.
///
/// A refused PATH is not in that list on purpose: it has a safe direction and
/// the row still carries its depth, so it is priced loudly at the closure
/// buffer rather than dragging the whole image back to the worst case.
pub(crate) fn subs_arena_from_descriptor(
    rows: &[SubEndpoint],
    subs: usize,
    entry_struct: usize,
    ring_len_bytes: usize,
    rx_buf_size: usize,
) -> Option<usize> {
    if rows.len() != subs {
        return None;
    }
    let mut total = 0usize;
    for row in rows {
        // phase-457 W3 (issue 1340) — an `in_place` row claims the entry struct
        // and NO trailing region: `open_subscription` returns through an in-place
        // entry before any slot size is computed. Worth ~9.7 KiB a subscription
        // at the default depth, and it is the one term here whose reduction rests
        // on a stated fact rather than on a bound — hence the predicate, and hence
        // the descriptor stating it only from an OBSERVED registration.
        //
        // The DEPTH is still required, before the branch: a row with no depth
        // describes an endpoint this table cannot price at all, and letting an
        // in-place row through without one would make the guard depend on which
        // path the row happens to state.
        let depth = row.depth?;
        if row.claims_no_region {
            // Still the buffered entry struct, which OVER-states the in-place
            // entry (`SubInplaceEntry` is smaller than `SubBufferedEntry`).
            // Deliberate: one struct size for both keeps this term's fallback
            // arms consistent, and the over-statement is bytes rather than a
            // failed registration.
            total += entry_struct;
            continue;
        }
        total += buffered_region(
            depth as usize,
            row_slot_bytes(row, rx_buf_size),
            ring_len_bytes,
        ) + entry_struct;
    }
    Some(total)
}

/// Bytes one buffered receive region claims, mirroring
/// [`executor::arena::buffered_region_size`] — the function the allocator
/// itself calls.
///
/// `depth <= 1` is a `TripleBuffer`: three slots, no length array. Anything
/// deeper is an `SpscRing` with `depth + 1` slots (Lamport's extra slot) and a
/// `usize` length beside each.
pub(crate) const fn buffered_region(depth: usize, slot: usize, ring_len_bytes: usize) -> usize {
    if depth <= 1 {
        TRIPLE_BUFFER_SLOTS * slot
    } else {
        (depth + 1) * slot + (depth + 1) * ring_len_bytes
    }
}
