---
id: 982
title: "Two rosidl-codegen tests were red on main, hidden because `check::build` sits behind `check::fast`"
status: resolved
area: codegen, ci
severity: medium
found: 2026-09-02
resolved: 2026-09-02
related: [0896, 0952, 0319, phase-392, phase-408]
---

# What was red

On clean `origin/main`, `cargo test -p rosidl-codegen` failed two tests:

```
generated_output_matches_the_committed_golden          FAILED
the_cpp_header_states_the_same_bound_as_the_c_header_and_the_rust_const  FAILED
```

Both trace to one change: `BoundState::classify` gained a transport-framing
allowance on the RX bound — `transport_framed(n) = n.next_multiple_of(4)` — so a
133-byte type states an RX bound of 136. TX stays exact, because we write what we
serialise and the framing underneath is the transport's business.

The allowance landed and two consumers were not moved with it:

| consumer | had | correct |
| --- | ---: | ---: |
| C golden `.h` | 136 | 136 (already right) |
| **C++ golden `.hpp`** | **133** | 136 |
| **`message_size_bound_parity` expectation** | **`max(x1,x2)`** | `transport_framed(max(x1,x2))` |

The C emitter was updated with the rule; the C++ golden and the parity test's
hand-recomputed expectation were not. That is issue 0896's own defect — one rule
with a second copy — appearing twice more: once along the LANGUAGE axis, once
along the TEST axis.

# Why nothing caught it

`cli-tests` does run these (`cargo test --manifest-path packages/cli/Cargo.toml
--workspace`), and it IS registered — in the `build` list. But `just check` runs
`check::fast` first and stops at the first failure, so any one red fast gate
silently withdraws every gate behind it, `check::build` included.

That is exactly the mechanism issue 0952 recorded ("`check::fast` runs before
`check::build` and `just` stops at the first failure, so any one of 146 fast
gates silently withdraws every backend build behind it"). 0952 named it for
backend BUILDS. The same ordering hides test LANES, and this is the first
recorded instance of that.

So the reds were reachable, in a registered lane, and still invisible for as long
as anything ahead of them was red — which during an ABI break is the normal
state.

# Fix

* The C++ golden regenerated (`NROS_UPDATE_GOLDEN=1`). The diff is exactly three
  files, RX only, two lines each (`RX_MAX_SERIALIZED_SIZE` and the
  `rx_size_bound` template that mirrors it), and every new value now equals the C
  header's for the same type — 0896 layer 2's stated invariant, and the evidence
  that the GENERATOR was right and the golden stale rather than the reverse.
* `message_size_bound_parity` now calls `bounds::transport_framed` instead of
  recomputing the max by hand. Deliberately the real function and not a local
  `next_multiple_of(4)`: a second copy of the rule is the defect being fixed, and
  a copy living in the test is no better than one living in a pack.

Verified: `cargo test -p rosidl-codegen` fully green, and the whole `cli-tests`
lane (`--workspace`) green.

# What this does NOT fix

The ordering. A red fast gate still withdraws `check::build` silently, so the
next lane to rot behind it will rot the same way. Making that visible is a
separate change — `just` stops at the first failure by design, so the fix is
either running the tiers independently in CI or reporting withdrawn lanes as
SKIPPED rather than as nothing, which is the same "no verdict is not a pass"
principle as issues 0975 and 0407.

Filed as a note here rather than fixed inline because it is a CI-shape decision,
not a codegen one.

# How it was found

Reading issue 0896 to see what remained of phase-392's W3, and running its tests
rather than trusting the issue text — which had already been stale twice on this
same issue (the arena hint and the `_with_info` gap were both described as open
and were both closed). The tests disagreed with the prose in the other direction:
work described as landed was landed, and two tests nobody had run were red.
