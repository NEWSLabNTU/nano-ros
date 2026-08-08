---
id: 481
title: "Readiness greps use string literals, so a wrong marker burns the whole
  timeout in silence — 8 fixed + gated, 35 baselined"
status: open  # 8 sites fixed + gate landed 2026-08-08; 35 baselined
type: tech-debt
area: testing
related: [issue-0471, phase-342, phase-277]
---

## Symptom

A test waits for a process's readiness marker, greps the wrong string, waits out
the FULL timeout, and continues as if nothing happened. It passes. Nothing is
logged. The only evidence is wall-clock time, and only if someone is looking.

Found by measurement, not by a failure: phase-342 W1 split
`native_example_pubsub` into per-cell tests, and `rust_cyclone` stood out at
34.1 s against `cpp_cyclone` at 5.2 s with every other cell at 4–5 s. Same
recipe, same assertions.

```
30 s timeout  +  2 s settle sleep  +  ~2 s delivery  =  34 s
```

The settle path greped the literal `"Waiting for"`. The C and C++ demos print
`"Waiting for messages (Ctrl+C to exit)..."`; the Rust listener never does — it
prints `"Subscriber created for topic: /chatter"`. Fixed in `564a5b0e3` by using
the constants that already existed (`LISTENER_READY_MARKER`,
`WS_C_LISTENER_READY_MARKER`): **34.1 s → 4.0 s**, and the whole binary
95.1 s → 7.9 s.

## Why it is silent

Two mechanisms compound, and neither is a bug on its own:

1. **The result is discarded.** Every affected site is `let _ = …
   wait_for_output_pattern(…)` or an ignored return. The wait is a courtesy, so
   its failure is not an error.
2. **The return value could not be trusted anyway** — issue **0471**:
   `wait_for_output_pattern` returns `Ok` on TIMEOUT whenever the process printed
   anything at all. So even a site that checked the `Result` would not learn that
   the marker never appeared.

Together: a wrong marker is indistinguishable from a slow start, forever.

## The rule this violates

CLAUDE.md, already: **"Test greps use `nros_tests::output::*` constants, never
literal strings."** Both spellings were already constants. The literal matched
one language by luck and two by nothing.

Phase-277 is the same class in its loud form — banners were slimmed and ~10 tests
that greped literals timed out. This is its quiet form: the grep is wrong from
the start and nothing ever changes to expose it.

## The audit — 12 sites

`git grep 'wait_for_output_pattern("Waiting for'` over
`packages/testing/nros-tests/tests`:

| file:line | timeout | waits on |
| --- | --- | --- |
| `action_multigoal.rs:60` | 10 s | `"Waiting for action"` |
| `actions.rs:40` | 5 s | `"Waiting for action"` |
| `c_riscv_nuttx_e2e.rs:72` | 10 s | C binary — likely fine |
| `declarative_bridge_zenoh_to_xrce.rs:115` | 8 s | ? |
| `deployed_native_system_e2e.rs:69` | 8 s | ? |
| `entry_e2e.rs:464` | 10 s | ? |
| `esp32_emulator.rs:211` | 60 s | `"Waiting for messages..."` |
| `esp32_emulator.rs:324` | 10 s | `native_proc` — **suspect** |
| `esp32_emulator.rs:397` | 60 s | `"Waiting for messages..."` |
| `esp32_emulator.rs:528` | 10 s | `native_proc` — **suspect** |
| `executor.rs:138` | 5 s | `listener` — **suspect** |
| `executor.rs:200` | 5 s | `listener` — **suspect** |

**Not swept blind, deliberately.** Most of these wait on C/C++ binaries that DO
print the literal, so a `sed` would change nothing for them and could break the
ones it touched. A wrong marker here is silent, so each site needs the same
evidence the fixed one got: which binary, what it actually prints, and the
before/after time. The suspects are the sites whose target is a Rust listener
(`executor.rs`) or an unqualified `native_proc` (`esp32_emulator.rs`).

Upper bound if every suspect is wrong: 4 sites × 5–10 s = 20–40 s per full run,
plus whatever the unqualified ones cost.

## Fixed (2026-08-08)

Eight sites corrected, all measured rather than pattern-matched. Six were the
real defect — every one waits on `build_native_listener()`, i.e.
`examples/native/rust/listener`, which prints `LISTENER_READY_MARKER`
("Subscriber created for topic:") and never the literal:

| site | timeout burned |
| --- | --- |
| `executor.rs:138` | 5 s |
| `executor.rs:200` | 5 s |
| `esp32_emulator.rs:324` | 10 s |
| `esp32_emulator.rs:528` | 10 s |
| `c_riscv_nuttx_e2e.rs:72` | 10 s |
| `entry_e2e.rs:464` | 10 s |

**50 s per full run**, in silence. Two more (`declarative_bridge_zenoh_to_xrce.rs:115`,
`deployed_native_system_e2e.rs:69`) wait on the int32 sink, which DOES print
`"Waiting for Int32"` — correct by luck, moved to `INT32_SINK_READY_MARKER`
anyway.

Cleared of suspicion by measurement: `actions.rs` and `action_multigoal.rs` wait
on the Rust action SERVER, which does print `"Waiting for action goals"`. Only
the listener lacks a `Waiting for…` banner.

### The gate — `check-readiness-marker-literals`

In `check-fast` (buildless, reads sources only). It flags a literal that is a
strict PREFIX of two or more `output::` constants, or that EQUALS one outright.
`"Waiting for"` is a prefix of FOUR, which is precisely why it matched some
binaries and not others.

**Not "no literals in `wait_for_output_pattern`":** 92 of 185 call sites pass a
literal and most wait for ordinary runtime output no constant defines. A gate
that flags those is noise, and noise gets suppressed. An earlier substring rule
was also rejected — it matched `"Listener"` against
`CONTRACT_MONITOR_DIAGSINK_READY_MARKER` by coincidence.

35 pre-existing sites are baselined in
`scripts/readiness-marker-literal-baseline.txt`, the shrinking-backlog shape
`check-leaf-lockfiles` uses: a listed site that stops matching must be deleted,
so the file cannot become a permanent exemption. Both arms verified to fail
before being trusted.

## The 35 audited (2026-08-08) — 3 more defects, 32 harmless

Audited all 35 rather than assuming. The decisive split is not which literal is
used, it is **whether the caller checks the result**:

| | sites | can it be silently wrong? |
| --- | --- | --- |
| CHECKED (`.expect` / `is_err()` / `unwrap_or_else`) | 26 | **No.** A wrong marker fails LOUDLY, and the suite is green — so those markers demonstrably match. Ambiguous, not broken. |
| DISCARDED (`let _ =` or unhandled) | 9 | Yes — only these can hide. |

Of the 9 discarded, six wait on a process that DOES print a `Waiting for…`
line, verified by reading each binary's source rather than its name:

- `zero_copy.rs:45` — `message-info-observer` prints BOTH
  `"Subscriber created for topic:"` AND `"Waiting for messages with
  MessageInfo…"`.
- `xrce.rs:209`, `xrce.rs:253` — `native/rust/serial-listener` prints BOTH
  `"Subscriber created on /chatter"` and `"Waiting for messages..."`. The serial
  listener is not the plain listener; the name suggested otherwise.
- `actions.rs:76` (`"Sending goal"`, an exact `ACTION_SENDING_GOAL_MARKER`
  duplicate), `params.rs:144`, `nano2nano.rs:66` (waits `"Publishing"` and DOES
  test the result).

**Three were real**, all resolving to `native/rust/listener`, which prints only
`"Subscriber created for topic:"`:

| site | timeout burned |
| --- | --- |
| `xrce.rs:96` | **30 s** |
| `zephyr.rs:1050` | 5 s |
| `xrce_ros2_interop.rs:176` | 5 s |

40 s more silent waiting, on top of the 50 s already fixed. Fixed; baseline
shrank 35 → 32.

The remaining 32 are ambiguous literals whose markers work — worth converting for
the rule's sake, worth nothing for runtime. They are not a defect backlog.

**The generalisable finding:** `build_xrce_listener()`, `xrce_listener_binary`
and `native_listener` all resolve to the same `native/rust/listener`. Three names
for one binary is why "which language does this wait on" could not be answered by
reading the call site, and why the audit had to resolve every builder to a
source path.
- **Issue 0471 is the deeper half, and this gate does not touch it.** While a
  timeout can return `Ok`, no amount of marker correctness makes these waits
  self-reporting: the gate stops a KNOWN-ambiguous literal landing, it cannot
  notice a marker that is unique but simply wrong. Fixing 0471 would turn every
  one of these from silent into loud.
