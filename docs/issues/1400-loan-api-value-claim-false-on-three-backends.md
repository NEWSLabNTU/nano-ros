---
id: 1400
title: "The loan API presents an optimisation on three of four backends where it
  is a heap allocation, an encode and a copy — strictly worse than `publish_raw`"
status: open
type: bug
area: [rmw, api]
severity: medium
found: 2026-09-21
related: [issue-0814, issue-0812, issue-0813, issue-0781, phase-417, rfc-0010, rfc-0089]
---

## What the surface claims

`nros_publisher_loan` / `_commit` / `_discard` (C), `Publisher::loan` (C++),
`EmbeddedRawPublisher::try_loan` (Rust) are documented as the zero-copy publish
path. `EmbeddedRawPublisher`'s own doc comment
(`packages/core/nros-node/src/executor/handles.rs:632-641`) states it as a
two-way choice:

> - `publish_raw`: user supplies a `&[u8]`, backend memcpys into its outbound
>   buffer. One copy.
> - `try_loan`: … Zero-copy on backends with native lending (Phase 99:
>   zenoh-pico `unstable-zenoh-api`, XRCE-DDS); single-memcpy fallback on
>   backends without (uORB).

## What is true, measured 2026-09-21 on `origin/main`

**One backend of four fills the vtable slot.** `borrow_loaned_message` is
declared once (`packages/core/nros-rmw-abi/include/nros/rmw_vtable.h:564`) and
filled once:

| backend | slot | site |
| --- | --- | --- |
| zenoh | `Some(zenoh_pub_loan)` | `packages/rmw/zenoh/nros-rmw-zenoh/src/lib.rs:439` |
| Cyclone | `nullptr` | `packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/vtable.cpp:386` |
| XRCE | `NULL` | `packages/rmw/xrce/nros-rmw-xrce/src/vtable.c:80` |
| uORB | `nullptr` | `packages/rmw/uorb/nros-rmw-uorb/src/vtable.cpp:90` |

So the doc comment above is wrong about XRCE as well as about the value: XRCE
NULLs the loan trio and fills `publish_streamed` instead, for the reason issue
0814 §2 measured (`uxr_prepare_output_stream` has no cancel and is not
contiguous past one history block).

**On the other three the loan is an allocation, not a saving.**
`CffiPublisher::try_lend_slot` (`packages/rmw/cffi/src/lib.rs:3493-3521`) takes
the documented fallback when the slot is NULL:

```rust
let mut staging = alloc::boxed::Box::new(ArenaStaging {
    buf: alloc::vec![0u8; len],
});
```

Two allocations per loan — the `Box` and the `Vec`. The caller then encodes into
that buffer exactly as it would have encoded into its own, and
`commit_slot` (`:3609-3621`) ends at

```rust
return Publisher::publish_raw(self, bytes);
```

which is the plain path, copy included. Net against `publish_raw`: the same
encode, the same backend copy, **plus** a malloc/free pair per message. There is
no step it removes. On uORB that is especially clear, because uORB never wanted
a CDR stage at all: `publisher_publish_raw`
(`packages/rmw/uorb/nros-rmw-uorb/src/publisher.cpp:116-146`) checks
`len >= state->meta->o_size` and hands the caller's bytes straight to
`orb_publish`, and `o_size` is `sizeof(message struct)`
(`packages/rmw/uorb/nros-rmw-uorb/src/uorb_abi.hpp:42`).

**It is `alloc`-only, so it refuses permanently on the targets it was written
for.** The `#[cfg(not(feature = "alloc"))]` arm of the same function
(`packages/rmw/cffi/src/lib.rs:3522-3547`) returns
`Err(TransportError::Unsupported)`, surfacing as `NROS_RET_NOT_ALLOWED` —
correctly PERMANENT since issue 0814's fix, and correctly documented as such in
the shipped header. But "correct refusal" is the whole behaviour on a
freestanding image with Cyclone, XRCE or uORB, which is the RAM-tight,
copy-count-sensitive deployment the feature's stated benefit is about.

**Zero callers.** A tree-wide grep for `nros_publisher_loan`,
`nros_subscription_borrow`, `try_loan`, `loan_with_timeout` and `.loan(` finds
call sites only in `packages/testing/nros-tests/tests/loan_e2e.rs` and
`packages/rmw/cffi/tests/{loan_native,loan_no_alloc}.rs`. **No example, no
template, no book chapter, no in-tree application calls it** — the same
measurement issue 0814 recorded on 2026-09-03, still true 18 days on.

**The fourth backend does not rescue the claim either.** `zenoh_pub_loan`
(`packages/rmw/zenoh/nros-rmw-zenoh/src/lib.rs:348-378`) lends from our own
static `LendArena` and aliases it into the put; issue 0814 §2 measured
zenoh-pico's encode memcpy'ing into a non-expandable wire buffer regardless
(`_z_buf_encode` takes its no-copy branch only for an expandable wbuf, and every
transport TX buffer is created `_z_wbuf_make(mtu, false)`). So on zenoh the loan
saves one copy from a user buffer into our arena — against `publish_raw`, not
against the transport — and `publish_streamed`, which is unfeatured and which
zenoh fills natively, already saves it by assembling inside zenoh's own
`z_owned_bytes_t`.

## Why this is a separate defect from issue 0814

0814 is the parent study and is still open; it asks whether the surface is
EXERCISED, and its recommendation (C) fixed the honesty defects one at a time —
the `Ok(None)` transient, `can_loan_messages` derived from the vtable, the
header's availability note, the no-std compile ratchet. What no fix addressed,
and what this issue is, is the **value claim** itself: the surface is presented
as an optimisation and on three of four backends it is a pessimisation. 0814's
own residue note judges the `ArenaStaging` allocation not worth closing in
place, which is a decision about the ALLOCATION; it does not decide whether a
path that only adds cost should keep advertising itself as the zero-copy one.
uORB is also evidence 0814 did not weigh. It existed when 0814 was written
(the backend moved to `packages/rmw/uorb` on 2026-07-31) and 0814 names it
once, in passing, as one of the three backends the fallback path serves; its
three-backend study read zenoh-pico, Cyclone and XRCE and not uORB. Yet uORB is
the one whose native shape — bytes already in struct layout, no CDR stage at
all — most looks like it should be the loan's best case, and it NULLs the slot.

## Options, none chosen

1. **Fill the slots.** Cyclone 0.10.5's `dds_loan_sample` is typed and needs
   `DDS_HAS_SHM` + a live iceoryx endpoint (0814 §2); XRCE's
   `uxr_prepare_output_stream` cannot be a flat cancellable span; uORB's
   `orb_publish` takes a caller pointer and exposes no writable queue slot. So
   this is the option with no available implementation on any of the three, on
   the versions we pin.
2. **Refuse instead of staging.** Make `try_lend_slot` answer
   `TransportError::Unsupported` when the vtable slot is NULL, in the `alloc`
   arm too — i.e. treat "the backend cannot lend" as the permanent fact it is on
   every build, not only on heap-free ones. Cost: the three backends lose a
   working-but-worthless path, and any future caller has to branch. Benefit: the
   API stops claiming something it does not do, and `can_loan_messages`
   (already derived from the slot, `packages/rmw/cffi/src/lib.rs:2554`) becomes
   the complete answer.
3. **Withdraw the surface.** Retire the publish half of `lending` and name
   `publish_streamed` / `process_raw_in_place` as the only zero-copy story, as
   0814's "On the proposed test lane" already suggests as the follow-up if a
   copy-count measurement shows no difference. Cost: an ABI surface goes, and
   issue 0781's reason for keeping `take_loaned_message` (the only shape that
   can hand a caller a view outliving the call) applies to the RECEIVE half and
   would have to be preserved separately.

A decision between them needs the copy-count measurement 0814 asks for —
memcpy'd bytes on the publish path, `publish_streamed` versus `publish_raw`
versus `try_loan`, on the cells that already exist — not a lane that proves
delivery.

## Acceptance

Whichever option is taken, the same two things must hold afterwards: no shipped
doc comment or header claims a saving a measurement does not show, and the
answer a caller gets on a NULL-slot backend is the same in an `alloc` build as
in a heap-free one, because the underlying fact (this vtable has no loan slot)
is the same in both.

## Related decisions

RFC-0089's loan-family section records the classification this measurement
produced: upstream's typed loan is `absent`, our byte loan is an `extension`,
and the two cannot be unified because they are zero-copy of different things.
That section rules on the NAMES; this issue is about whether the thing behind
our name earns its documentation.
