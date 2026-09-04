---
id: 1048
title: "Every `log::info!` on the esp32-qemu board is silently dropped, so four e2e cells grep for a marker the image cannot emit"
status: open
area: boards, testing
severity: high
found: 2026-09-04
related: [0968, 1025, 0064]
---

# The entity exists; the line announcing it does not

## Reproduced, with a bracketing probe

`examples/qemu-esp32-baremetal/rust/listener` creates its subscription and then
announces it:

```rust
let _sub = node.create_subscription_for_callback_name::<StringMsg>("on_chatter", "/chatter")?;
log::info!("Subscriber created for topic: /chatter");
```

A `println!` was placed on EACH side of that `log::info!` — via
`nros_board_esp32_qemu::esp_println`, which the board already re-exports, so no
dependency was added. Built with the row's env and run under QEMU with a zenoh
router up:

```
PROBE-A
PROBE-B

Application setup complete — entering spin loop.
```

**Both probes print. The `log::info!` between them prints nothing.** Execution
reaches the site, the macro returns, and the record never reaches a console that
the immediately adjacent `esp_println::println!` reaches.

No `log`-crate output appears anywhere in the boot — no INFO, WARN or DEBUG
line at all.

## Why this matters beyond a missing line

Four of issue 0968's twelve tier-2 failures are esp32 cells, and they fail by
GREPPING FOR THIS MARKER:

```
[xrce/cpp/Pubsub] listener received 0 sample(s), expected ≥1
esp32-qemu did not print `Subscriber created for topic:` within 60s
```

The subscription is there. The test is waiting for a line the image is
structurally unable to emit, so the cell reports a messaging failure for a
logging defect. That is CLAUDE.md's "diff the grep pattern against what the
fixture actually prints" pitfall, with the twist that the pattern is correct and
the PRINTING is broken.

## What is NOT the cause — checked, not assumed

| hypothesis | why it is out |
| --- | --- |
| execution never reaches the line | `PROBE-A` / `PROBE-B` bracket it; both print |
| no logger installed | `esp_println::logger::init_logger(LevelFilter::Info)` runs in `init_hardware`, whose own prints are in the output |
| compile-time level stripping | no `max_level_*` / `release_max_level_*` feature; resolved features are `[]` / `["std"]` |
| two `log` facades, so `set_logger` served the wrong one | there is ONE `log` in the target graph (0.4.33), the same one the example binds and esp-println installs into. (An earlier claim of two was a grep artifact: `log v[0-9.]+` also matches `nros-log v0.5.0`.) |
| esp-println built without `log-04` | the resolved features for this image include `log-04` |

## The remaining candidate, untested

`log::set_logger` succeeds at most ONCE per process. If anything installs a
logger before `init_hardware` runs, esp-println's `init_logger` fails and — as
that API returns `()` — the failure is invisible. The board registers its own
platform writer immediately above the `init_logger` call
(`nros-board-esp32-qemu/src/node.rs:357-363`), so there are two log paths in
this image and only one can win the facade.

Testing that is one line: capture `log::set_logger`'s `Result` (or call
`log::logger()` afterwards and compare) rather than assuming it took.

Issue #64 is the ancestor here — the `init_logger` call and its comment exist
because the same symptom was fixed once already ("without it the `log` crate has
no logger installed and silently drops every record"). It is back.

## Not to be confused with

`test_esp32_workspace_entry_e2e`, the fourth esp32 cell, fails BEFORE
`Application setup complete` and so is not this. The other two
(`test_esp32_to_native`, `test_native_to_esp32`) grep for markers of the same
kind and plausibly are, but that is unverified.
