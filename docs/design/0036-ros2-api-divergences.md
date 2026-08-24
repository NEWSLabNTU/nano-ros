---
rfc: 0036
title: "Divergences from the ROS 2 standard client APIs (rclrs / rclcpp / rclc)"
status: Draft
since: 2026-06
last-reviewed: 2026-08
implements-tracked-by: [phase-379]
supersedes: []
superseded-by: null
---

# RFC-0036 — Divergences from the ROS 2 standard client APIs

## Summary

nano-ros deliberately mirrors the ROS 2 client libraries — Rust ≈ rclrs 0.7.0
(settled 2026-08-25; the recorded surface is the `v0.7.0` release tag),
C ≈ rclc, C++ ≈ rclcpp — so a ROS 2 developer can read and write nano-ros code.
But `no_std` / embedded / no-allocator / no-exceptions constraints force a set of
**deliberate divergences**. Today they are scattered across RFC-0018/0021/0022/
0002 and prose notes, with no single reference — and stale notes still label the
Rust error `RclrsError` when it is actually `NanoRosError`. This RFC is the
**one authoritative catalog** of what differs, why, and what a porting user must
adjust. It is a reference, not a new decision: each row points to the RFC that
owns the decision.

## Motivation / problem

- A user evaluating or porting to nano-ros needs one place that answers "how is
  this different from the ROS 2 API I know?" Scattered notes don't serve that.
- The divergences are load-bearing API contracts; an authoritative list keeps
  future RFCs from silently re-diverging or accidentally converging.

## Design

Each divergence: **what ROS 2 does → what nano-ros does → why → owner**.

### Type system

| ROS 2 | nano-ros | why | owner |
|---|---|---|---|
| `std::vector<T>` / `rosidl_runtime_rs::Sequence<T>` (unbounded) | `heapless::Vec<T, N>` (owned), `alloc::Vec<T>` / `nros::HeapSequence<T>` (heap), `&'a [T]` (borrowed) — per-field via `nros-codegen.toml` | no implicit heap on MCU; capacity is a local choice (invisible on CDR wire) | RFC-0033 |
| `std::string` / `rosidl_runtime_rs::String` | `heapless::String<N>` / `nros::HeapString` / `&'a str` | same | RFC-0033 |
| computed type hash (Iron+) | `TYPE_HASH = "TypeHashNotSupported"` (Humble baseline) | Humble predates type hashing; Iron deferred | — |

### Errors

| ROS 2 | nano-ros | why | owner |
|---|---|---|---|
| rclcpp throws `std::exception` subclasses | C++ returns `nros::Result` + `NROS_TRY(expr)` early-return macro | `-fno-exceptions` on Zephyr/FreeRTOS/bare-metal | RFC-0018 |
| rclrs `RclrsError` | Rust `NanoRosError { code: RclReturnCode, context, nested }`; `RclReturnCode` mirrors `rcl_ret_t` numerics as a Rust enum | result-only, `no_std`, C-ABI-compatible codes | `nros-core/src/error.rs` |
| `rcl_ret_t` int | C `nros_ret_t` enum (`NROS_RET_OK=0`, …); RMW layer `rmw_ret_t` (`0 … -18`) | explicit numeric ABI | RFC-0035 (rmw) |

> **Naming note:** older prose still calls the Rust error `RclrsError`. The
> actual type is `NanoRosError` (+ `RclReturnCode`). Treat this RFC as the
> authority; correct any surviving stale reference.

### Domain ID

- ROS 2: `ROS_DOMAIN_ID` read from the environment at runtime, everywhere.
- nano-ros: **compile-time-baked on embedded** (`CONFIG_NROS_DOMAIN_ID` Kconfig /
  per-example `config.toml` → `app_config.h`); **runtime env only on native/host**
  (`nros_tests::unique_ros_domain_id()`). A runtime `ROS_DOMAIN_ID` does **not**
  reach an embedded backend (no libc `getenv` trampoline on e.g. native_sim).
- Why: embedded backends have no runtime env; the domain must be linked in.
  (CLAUDE.md "QEMU Networked Tests".)

### Naming / namespacing

- `nros::` not `rclcpp::`; `nros_*` C fns not `rcl_*`/`rclc_*`; `CONFIG_NROS_*`
  build config. Signals the embedded variant while mirroring the surface.
- Topic key conventions (`rt/`, `rq/`, `rr/`) preserved for rmw_zenoh
  interop; `QosSettings.avoid_ros_namespace_conventions` toggles them.

### Execution & blocking model

| ROS 2 | nano-ros | why | owner |
|---|---|---|---|
| `rclcpp::spin` with multiple executors / spinner threads | **one `Executor` per RTOS task**, shared by all nodes in that tier; FIFO callback dispatch | avoid OS priority-slot starvation; mixed-criticality via priority tiers | RFC-0002, RFC-0015 |
| blocking `client->async_send_request(...).get()` internally drives the loop | non-blocking `Promise<T>`; **every blocking helper takes the executor and drives it** (`promise.wait(&mut executor, timeout)`) | single source of I/O; reentrancy-safe; reliable RTOS timeouts | RFC-0021 |
| action client `wait_for_result()` blocks | spin-driven `Promise` poll/`wait(&mut executor)` | same; no deadlock on single-threaded transports | RFC-0021 |
| `rclcpp::Node` shared via `std::shared_ptr<Node>` (`Arc<Node>` in rclrs) | `&mut Executor` + short-lived `NodeCtx<'_>`; **no `Arc<Node>`**; entities owned, outlive the handle | zero allocation; two live node handles = borrow error by construction | RFC-0022 |

### Reduced / omitted surface

- **No exceptions, no RTTI, no STL** in C++ (`-fno-exceptions -fno-rtti`,
  freestanding). `const char*` not `std::string`; plain fn-ptr + `void* ctx`
  callbacks not `std::function`; value types not `shared_ptr`. (RFC-0018.)
- **QoS subset** — history/reliability/durability/liveliness/deadline/lifespan
  supported; selected at compile time; no dynamic QoS negotiation.
- **No dynamic discovery** — peers static via `nros.toml` / Kconfig locator.
- **No parameter callbacks**; parameters are read/write only.
- **No lifecycle-node graph** — a simplified state model for embedded executors.

## This catalog is now checked, not only written

`scripts/api-parity.py` (phase-379) extracts both surfaces from their real
sources — clang JSON AST for the C and C++ headers, rustdoc JSON for the Rust
crates — correlates them by normalised name, and reports every item that does
not correspond. `just api-parity` runs it; `docs/reference/api-surface/*.json`
holds the recorded ROS 2 side so it runs on a host with no ROS install.

The reason is this RFC's own history. It shipped calling the Rust error
`RclrsError` when the type had been `NanoRosError` for months, and had to add a
note correcting itself. Issue 0338 is the same failure one level down:
`Executor::spin` meant the OPPOSITE of `rclcpp::Executor::spin` here, and a
person reading found it, once. A catalog of API divergences that only a reader
can check will drift, because the API moves and the prose does not.

Two things are compared, and two are deliberately not. The comparison is against
the **public** ROS 2 API only — not rclcpp's callback type erasure, rcl's
wait-set plumbing, or the generated accessors of `rcl_interfaces`, which are
codegen output on both sides and belong to RFC-0023/0033. And a divergence
applied SYSTEMATICALLY is stated once as a signature rule rather than repeated
per site: `rcl` threads an `rcl_allocator_t *` through six entry points and
nano-ros has one global allocator, so it appears in none of them — one sentence,
not six rows.

Everything the rules do not cover is expected to have a row in
`docs/reference/api-parity-ledger.json` naming the platform constraint that
justifies it. The ledger's verdicts are deliberately narrow — `divergence`
requires naming a constraint (`no_std`, no exceptions, no allocator, no runtime
env, single-threaded transport), so "we preferred it this way" cannot be
recorded as one.

**The first run also corrected the reading of where we stand.** Against rclcpp
there are ZERO argument divergences among shared names; what differs is
coverage. Against rclc+rcl there are 32, of which 24 are five systematic
decisions (no allocator, compile-time options, no argv, callbacks bound at
creation, handles that carry their node) and 8 are open. Against rclrs the
`nros` facade exports 709 items rclrs has no equivalent for. Phase 379 W2–W5
classify and settle these; until then this RFC's catalog is accurate about the
divergences it lists and silent about a much larger set it never enumerated.

The five systematic rules belong in this RFC's tables once W4 has checked each
constraint still holds; they live in `scripts/api_parity/signature_rules.py`
meanwhile, where deleting one re-opens every row it covers.

## Alternatives considered

- **Keep divergences in per-RFC notes only.** Rejected — no single porting
  reference; stale mislabels (the `RclrsError` name) go uncaught.
- **Aim for byte-for-byte ROS 2 API parity.** Rejected — impossible under
  `no_std`/no-exceptions; the divergences are the point of the project.

## Open questions

1. Should this RFC carry per-language migration snippets (rclcpp→nros side by
   side), or link to a `book/` migration chapter? Proposed: keep the catalog
   here; put runnable side-by-sides in `book/`.
2. Track convergence opportunities (e.g. a hosted-only `std`-backed mode closer
   to rclrs)? Proposed: out of scope; note if it arises.
3. ~~**Which rclrs do we mirror?**~~ **Settled 2026-08-25: the latest release,
   `v0.7.0`**, which is what this RFC already claimed. The correlator's recorded
   surface was 0.5.1 until then and is now derived from the `v0.7.0` tag
   (`docs/reference/api-surface/rclrs.json` carries the commit). The bump grew
   the reference surface from 129 records to 213 and produced two findings worth
   keeping:
   - **rclrs gained ACTIONS**, which 0.5.1 did not have, and modelled them as a
     TYPESTATE chain (`RequestedGoal` → `AcceptedGoal` → `ExecutingGoal` →
     `TerminatedGoal`). rclcpp_action uses `async_send_goal` with a shared
     `ClientGoalHandle`. **ROS 2 does not agree with itself here**, so "match
     ROS 2" has no single answer for actions and W5 has to pick a side and say
     which.
   - **rclrs converged on our timer model.** 0.7.0 has
     `create_timer_inert`/`_oneshot`/`_repeating` — the same three modes our
     `TimerMode` carries, arrived at independently. Several rows this campaign
     recorded as divergences against 0.5.1 are now shape agreements with
     different placement.

## Changelog

- 2026-08 — the catalog gained a checker (`scripts/api-parity.py`, phase-379)
  and a ledger (`docs/reference/api-parity-ledger.json`). Recorded the first
  run's finding that the C++ lane has no argument divergences at all, the C lane
  has 32, and the Rust facade over-exports; opened the rclrs-version question.
- 2026-06 — created (Draft). Consolidated the type/error/domain-id/naming/
  execution/omitted-surface divergences from RFC-0018/0021/0022/0002 + code;
  noted the stale `RclrsError` → actual `NanoRosError` naming.
