---
id: 702
title: "Eight tests wore the same shape — print a diagnosis, report PASS — so the class now has a gate"
status: resolved
type: tech-debt
area: testing
related: [issue-0682, issue-0683, issue-0686, issue-0691, issue-0693]
---

## The class

One day of sweeping turned up eight instances of a single shape: a test that
prints something a reader takes for "fine" and returns success, having measured
nothing.

| where | shape |
| --- | --- |
| `test_action_binaries_exist` | `Err(e) => eprintln!("[INFO] Could not build …")`, the only assertion inside `Ok` — a test named "binaries exist" passing when they did not |
| `heap_compile_check.rs` x3 | `eprintln!("SKIP: cc not found"); return;` — a file whose purpose is "the generated C compiles", concluding that without a compiler |
| `comparison_test` / `parity_test` (#0693) | `eprintln!("Skipping test: {e}"); return Ok(())` x13, reading `/opt/ros/jazzy` on a humble host — "19 tests, 0.027 s, all green" over work never done |
| `zenoh_integration` (#0682) x4 | `Ok(..) => assert.., Err(e) => println!("expected in some environments")` — green over a capability that had NEVER been present |
| `nav2_compat`, `board_agnostic_run_plan` (#0683/#0686) | skipping on a Placeholder stub whose stated reason had been wrong since phase-330 |
| `freertos_firmware_entry` (#0686) | `eprintln!("build smoke verified")` then fall through to green, over a stub |
| `nano2nano` x2, `xrce` x4 (here) | a readiness wait whose `Err` arm prints "exited early" — and the `match` is the test's last statement |
| `test_array_of_arrays` (here) | accepts all three outcomes; would stay green if the parser started emitting nonsense |

Each survived months. Each was found by a human going looking, and the looking
only happened because installing one apt package dropped a sweep from 167 skips
to 7 and made the remainder legible.

## The gate

`scripts/check-tests-can-fail.py`, on the fast line (`check-fast`), buildless.

REJECTS an `Err(..) => { … }` arm whose body prints and contains no `panic!`,
`assert*`, `skip!`, `?`, `expect(`, `bail!`, `return Err` or `unreachable!`.

Does NOT reject the honest spellings, and this matters more than the rejection:

* `nros_tests::skip!(…)` — the harness counts it, junit records it
* any arm that also asserts or propagates
* the `require_*` pattern — an arm that prints a note and yields `None`/`false`
  to a CALLER that decides. `cffi_smoke.rs`'s `router_locator()` is the live
  example. Flagging it would push authors toward deleting the note rather than
  toward asserting, which is the opposite of the point.

Self-testing (`--self-test`, and every run), because a gate for this class that
silently stopped matching would be an instance of the class. The self-test
caught two defects in the gate during development: `/* … */` comments were not
stripped, and the `require_*` exemption was missing.

## Fixed while landing it

The gate found ten, of which eight were real:

* six readiness waits (`nano2nano` x2, `xrce` x4) now ASSERT `is_running()` —
  a marker that never arrives is tolerable under load, a process that EXITED is
  the failure the test exists to catch;
* `test_array_of_arrays` pins the measured behaviour (the parser rejects
  `int32[5][10]`; ROS 2 IDL has no multi-dimensional arrays) instead of
  welcoming every outcome.

Two were the `require_*` shape, and the gate was corrected rather than the code.

## Verified

`just check tests-can-fail` — 258 test files, clean. `nano2nano` + `xrce` 15/16
(the 16th is #0682's peer-mode capability skip). `edge_case_test` 15/15.
