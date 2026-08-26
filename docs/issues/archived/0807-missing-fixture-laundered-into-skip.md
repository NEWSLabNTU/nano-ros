---
id: 807
title: "The Cortex-M cells relabelled EVERY resolver error as `not prebuilt` and
  skipped, so a STALE image was filtered out as a setup condition and the cell
  stopped running with nothing reporting it"
status: resolved
type: bug
area: testing
related: [issue-0584, issue-0806, issue-0445, phase-382]
---

## Problem

`tests/zephyr_cortex_m_qemu.rs:101`:

```rust
let binary = build_zephyr_cortex_m_example(lang, "talker", Rmw::Zenoh).unwrap_or_else(|e| {
    nros_tests::skip!(
        "zephyr/{}/talker for mps2_an385 not prebuilt; run \
         `just zephyr build-fixtures` first: {:?}", lang, e)
});
```

Two defects in one expression.

**1. It skips where issue 0584 requires a failure.** Since 0584 an absent
IN-LANE fixture is a hard failure: the run is gated, so the build has already
asserted the lane's fixtures exist. Out-of-lane is handled INSIDE the resolver,
which raises its own `[SKIPPED]` — so anything reaching this `Err` is real.

**2. It relabels the error it is reporting.** The resolver's other verdict is
**STALE**, not missing. This wrapper prints "not prebuilt" for both. That string
is load-bearing: `fixtures/binaries/mod.rs` filters skips on
`msg.contains("not prebuilt")`, and `_count-real-failures` drops them from the
failure count. So a stale image was laundered into a setup condition and
vanished from the summary.

The gate that caught it predicted the shape exactly: *"Something is still
laundering the resolver's Err into a `[SKIPPED]` (see the `not prebuilt` matches
in fixtures/binaries)."*

## Why it matters

It hid issue **0806** for five consecutive stale verdicts. The cell had stopped
running entirely — no runtime result at all — and every signal said "setup
condition, not your problem". A test that cannot fail is worse than a missing
test, because it reports as coverage; the whole point of `check-no-vacuous-tests`
one layer up.

## Fix (2026-08-26)

The `skip!` becomes a `panic!` naming why it is not a setup condition — the lane
gate already asserted this fixture is built — and surfaces the resolver's real
verdict instead of overwriting it with "not prebuilt".

Verified by mutation: with a genuine content edit making the fixture stale, the
cell now fails loudly with the STALE verdict quoted and **no `[SKIPPED]` marker**
anywhere in the output, where before it reported a skip and passed.

## Not fixed here, and deliberately

`fixtures/binaries/mod.rs` still routes `msg.contains("not prebuilt")` to a skip
in several rstest fixtures (`xrce_large_msg_test_binary` and siblings). Keying
control flow on substring-matching an error MESSAGE is fragile in the same way
this was, and those sites predate 0584 — but they are a separate audit with
their own blast radius, not a drive-by. Worth its own issue if a second one bites.
