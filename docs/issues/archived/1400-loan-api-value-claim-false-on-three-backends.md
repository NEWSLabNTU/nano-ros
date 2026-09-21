---
id: 1400
title: "The loan API presents an optimisation on three of four backends where it
  is a heap allocation, an encode and a copy — strictly worse than `publish_raw`"
status: wontfix
type: bug
area: [rmw, api]
severity: medium
found: 2026-09-21
resolved: 2026-09-21
resolved_in: "issue 0814 — folded into that issue, which stays open"
related: [issue-0814, issue-0812, issue-0813, issue-0781, phase-417, rfc-0010, rfc-0089]
---

> **FOLDED INTO issue 0814 — 2026-09-21, the day it was filed.** Not a
> duplicate, and nothing here was wrong: 0814 asks whether the `lending` surface
> is EXERCISED, this asked whether the thing it does where it IS exercised is
> worth doing. They are the same SURFACE, which is the test for folding, and
> keeping two open issues over one surface means the next reader has to
> reconcile two per-backend studies that cite the same files.
>
> **0814 has the number and stays `open`** — the defect measured here is not
> fixed, it is recorded under the other id. The content is carried into
> 0814 § "Folded in: the VALUE claim", not summarised: the four-row vtable table,
> the two-allocations reading of `try_lend_slot` / `commit_slot`, the `alloc`-only
> permanent refusal, the zero-callers grep, the zenoh arena measurement, the three
> options and the acceptance.
>
> Two things this file contributed that 0814's own study did not have, and which
> are the reason the fold preserves rather than summarises:
>
> * **uORB as the fourth backend.** 0814's per-backend study read zenoh-pico,
>   Cyclone and XRCE, and named uORB once in passing. uORB is the one whose
>   native shape IS upstream's typed loan — `publisher_publish_raw`
>   (`packages/rmw/uorb/nros-rmw-uorb/src/publisher.cpp:116-146`) checks
>   `len >= state->meta->o_size` and hands the caller's bytes straight to
>   `orb_publish`, with `o_size` = `sizeof(message struct)`
>   (`packages/rmw/uorb/nros-rmw-uorb/src/uorb_abi.hpp:42`), so there is no CDR
>   stage at all — and it leaves `borrow_loaned_message` NULL
>   (`packages/rmw/uorb/nros-rmw-uorb/src/vtable.cpp:90`) and takes a `Box`ed
>   staging buffer instead.
> * **The VALUE claim as the framing.** 0814's recommendation (C) fixed the
>   honesty defects one at a time — the `Ok(None)` transient,
>   `can_loan_messages` derived from the vtable, the header's availability note,
>   the no-std compile ratchet — and none of them addressed whether a path that
>   only ADDS cost should keep advertising itself as the zero-copy one. 0814's
>   residue note rules the `ArenaStaging` allocation not worth closing in place,
>   which is a decision about the ALLOCATION, not about the claim.
>
> One measurement MOVED between filing and folding and is recorded in 0814 rather
> than carried forward silently: the caller set is larger than the grep here
> found. `packages/rmw/cffi/tests/loan_fallback.rs` and the C++ compile guard
> `packages/api/nros-cpp/tests/compile/publisher_publish_guards_initialized.cpp:139`
> also call the surface. Both are tests, so "no example, template, book chapter
> or application calls it" is unchanged — but a sweep must grep `try_lend_slot`
> and `.loan(` as well as `try_loan`, or it misses them.
>
> Everything else here re-resolved at the cited lines on `origin/main` at
> `3da6315e2`, including all four vtable rows, both `try_lend_slot` arms, the
> `publish_raw` tail of `commit_slot`, and the `can_loan_messages` derivation at
> `packages/rmw/cffi/src/lib.rs:2554`.

**Read issue 0814**, section "Folded in: the VALUE claim". This file is a stub so
that the id resolves and so that a reader who arrives by the number learns where
the record went; it deliberately carries no second copy of the argument, because
two copies of a per-backend study is how they drift.
