---
id: 1676
title: "A Rust entry bakes `system_monitors.rs` and never installs it, so a
  contracted Rust image monitors nothing — and so cannot report to `/diagnostics`"
status: resolved
type: bug
area: [codegen, cli, diagnostics]
severity: medium
found: 2026-10-03
related: [issue-1635, issue-1604, issue-0514, phase-462, rfc-0052]
---

## What happens

`codegen_system` writes `<bake>/system_monitors.rs` for a contracted Rust
image: the `NROS_MONITORS` / `NROS_AGE_MONITORS` tables and an installer,

```rust
pub fn nros_install_monitors(executor: &mut ::nros_node::executor::Executor<'_>) {
    executor.set_monitor_table(NROS_MONITORS);
    executor.set_age_table(NROS_AGE_MONITORS);
}
```

(`packages/cli/nros-cli-core/src/orchestration/model_ingest.rs`). Nothing calls
it:

```
git grep -n 'nros_install_monitors' -- packages examples ':!*.golden'
# only the generator itself
```

`nros::main!` builds the executor and never reaches the baked installer, so a
Rust image's `min_rate_hz` / `max_age_ms` / `max_latency_ms` promises are
checked by nothing on target — not even the detection-time log line (issue
0514's floor), because the tables are empty. phase-462 W1's status line
recorded this as a follow-up ("Rust entries bake system_monitors.rs but
nros::main! never installs it"); no issue tracked it.

Issue 1635 made a contracted C and C++ image publish its violations on
`/diagnostics` by arming a reporter where the monitor tables are installed
(`nros_cpp_install_monitors`), with the executor feeding it at detection
(`Executor::set_violation_sink`). The Rust road has the sink but no install,
so it gets nothing from that fix.

## What a fix needs

* `nros::main!` (and the tiered setup it renders) calls the baked
  `nros_install_monitors` on each executor before entity creation, as the C/C++
  packs call `nros_cpp_install_monitors` — a publisher attaches its counter
  cell at create time.
* The same `/diagnostics` reporter: a `nros`-level twin of `nros-cpp`'s
  `src/diag.rs` (create the publisher, install it with `set_violation_sink`),
  ideally ONE implementation the C ABI then calls, so the three languages
  report through one piece of code.
* Acceptance as 1635's: a contracted Rust image observed by a `/diagnostics`
  subscriber, with a compliant control.

## Resolution

Fixed 2026-10-05 on `fix/1676-rust-entry-installs-monitors`. Three defects
stood between a contracted Rust image and `/diagnostics`, and the first one hid
the other two.

**1. Nothing installed the table.** `nros::main!` now does, on every executor it
builds, before the first node: the single-executor closure, each tier's closure
(the board runs ONE closure per tier executor, so the call picks that tier's
table at run time with `Executor::group_active` on the tier's own first
member), the Zephyr closures and the ESP32 one. The rows come from
`nros_orchestration_ir::contract_monitors` — `MonitorRow`/`AgeRow`,
`monitor_rows`/`age_rows`, `row_node_fqn` and the Rust renderer moved there
from `nros-cli-core`, so the proc-macro (which cannot dep the CLI) derives the
SAME rows the C/C++ emitters bake; `model_ingest` re-exports them and
`lower.rs` uses the shared `row_node_fqn`. Rows are sliced to the nodes the
entry deploys, and per tier to that tier's nodes (the C/C++ packs' rule). The
expansion is one `mod __nros_contract_monitors_<k>` per table — the
`system_monitors.rs` text rendered at `::nros` — plus the call. An
uncontracted entry emits nothing.

**2. One reporter, three languages.** `nros::contract` holds `DiagSink` (moved
up from nros-cpp's `diag.rs`) and `install_contract_monitors` (the refusing
`try_set_monitor_tables` + arm, a `static ContractReporter` per executor).
nros-cpp's `nros_cpp_install_monitors` arms the same `DiagSink`;
`nros-diagnostics` now rides `nros`'s `rmw-cffi` (19 tracked leaf locks gain
it, additions only, via `just lock-update`).

**3. Found by running it: the Rust publisher never bumped its cell.** The first
violating run reported `measured=0` while the talker published at 1 Hz. The
Rust component road creates an `EmbeddedRawPublisher`, which — unlike the typed
`EmbeddedPublisher` and the C++ publisher — had no cell. It now carries one
(attached by the same exact-topic rule at `create_publisher_raw_on` and
`NodeHandle::create_publisher_raw_with_qos`) and bumps it on every publish
route, both loan commits included. Unit
`a_raw_publisher_on_a_contracted_topic_bumps_its_cell`, negative-controlled
(attaching `None` fails it).

**4. And the sizing.** The reporter is a publisher created on the session's own
node before any component, and the entity inventory counted neither: the
talker's own `/chatter` then failed (`ConnectionFailed` out of
`claim_node_slot`, measured with a temporary log of the erased error). The
inventory now counts `contract_reporters()` publishers and one node for a
contracted image (`a_contracted_image_counts_its_diagnostics_reporter`).

**Measured** on a real native Rust image: `examples/workspaces/rust` copied
to an untracked scratch dir, `launch/system.contract.yaml` declaring the
talker's 1 Hz timer path, `talker.pub.chatter.min_rate_hz` and the listener's
subscription, built with this tree's CLI (`nros sync && nros build native`),
run 15 s against a private `rmw_zenohd` (`tcp/127.0.0.1:17647`) with
`ros2 topic echo --no-daemon /diagnostics` (rmw_zenoh_cpp) observing:

| contract | image log | `/diagnostics` |
| --- | --- | --- |
| `min_rate_hz: 5` (talker at 1 Hz) | `contract violation: rate-hierarchy-runtime /talker/chatter measured=999 declared=5000`, 14 publishes | one `DiagnosticArray`: level 2, `name: rate-hierarchy-runtime`, `message: measured 999 vs declared 5000`, `hardware_id: /talker/chatter`, `kind: guarantee` |
| `min_rate_hz: 0.5` (compliant control) | no violation, 14 publishes | nothing |

BEFORE: the same contracted image linked no table at all (`nros_install_monitors`
had no caller), so there was nothing to log or report. In the order the fixes
landed: with 1–2, the image did not boot (defect 4); with 1, 2 and 4, it
reported `measured=0` (defect 3); with all four, the table above.

Unit: `contract_monitor_tests` in nros-macros (uncontracted emits nothing;
single executor bakes and installs one `::nros`-rooted table; tiered slices per
tier and selects by member). `cargo test -p nros-cli-core` (all green, with a
debug `nros` built for the scaffold tests), `cargo test -p nros-macros`.

Filed: [issue 1694](1694-partial-contract-undersizes-rust-image.md) — a
contract that describes only the talker sizes the image from the contract
alone (`NROS_EXECUTOR_MAX_CBS = 0`) and it dies `ExecutorFull`; the
measurement above therefore describes every endpoint.

**Not measured:** a tiered Rust image (the per-tier selection is unit-tested,
not run); a Zephyr or other RTOS Rust image; RTIC/Embassy entries, whose
framework bodies register through `register_dispatch` and carry no
`RuntimeCtx` closure — none takes a contract today, and they are not wired.

Sweep: `git grep -n '#contract_monitors_call #declared_params_call' packages/core/nros-macros`
(5 register closures), `git grep -n 'EmbeddedRawPublisher {' -- packages` (every
constructor sets `monitor`).
