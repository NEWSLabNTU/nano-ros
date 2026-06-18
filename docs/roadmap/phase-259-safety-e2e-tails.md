# Phase 259 — safety-e2e capability tails

Status: **Planned (2026-06-18)** · Implements the §B tail of
[issue 0076](../issues/0076-followups-config-ssot-and-safety-e2e-arc.md) ·
Follows [phase-252](phase-252-capability-axis-board-lowering.md) (capability board
lowering) + issue 0073 (C/C++ safety-e2e lowering, resolved).

> **Context.** The safety-e2e axis (CRC-32 attach on publish + validate on receive)
> is landed + proven on the zenoh backend for Rust **and** C/C++ (issue 0073). What
> remains are the per-backend / per-board tails that the arc deliberately deferred —
> none block the capability; each is independently landable.

## Work items

### W1 — threadx boards safety wiring — DONE (2026-06-18)
**Investigation:** threadx (like native) is **app-level RMW** — the board crate has
no `rmw-zenoh`/`capability_features`; the resolved RMW's backend dep is emitted
directly. `render_backend_dependencies` (generate.rs) runs UNCONDITIONALLY and
`render_one_backend` adds `safety-e2e` to `nros-rmw-zenoh` whenever the backend
carries it (`backend_features`, zenoh) — so **threadx+zenoh+`[safety]` DOES forward
the CRC** (same zenoh shim the native C/C++ e2e proves), independent of board
advertisement.

The phase-252 `board_capability_features` warning ("does not declare the
capability feature; `[safety]` enables the validation surface but NOT backend CRC
on this board") was therefore a **false negative** for threadx/native — the backend
dep carries the CRC. **Fix:** removed that board-level warning (kept the
feature-add for advertised board-level-RMW boards). The accurate signal is W2's
resolved-RMW warning (`collect_plan_warnings`): threadx+cyclone/xrce+`[safety]` →
W2 warns (no CRC path); threadx+zenoh+`[safety]` → forwards, no warning.
- **Acceptance MET:** `[safety]` on threadx forwards (zenoh) or W2-warns
  (cyclone/xrce) — no silent skip, no false warning. (A runtime threadx-linux CRC
  fixture is deferred: the lowering is board-agnostic + the zenoh shim is proven by
  the native e2e; a heavy NSOS/QEMU fixture adds marginal proof.)

### W2 — cyclonedds / xrce: gate the no-CRC backends — DONE (2026-06-18)
**Landed:** `collect_plan_warnings` (planner.rs) warns per linked RMW the capability
registry doesn't list when `[safety]` is declared — SSoT = the registry
(`capability("safety").backend_supports`), no hardcoded list. Surfaces via
`check_plan_file` → `nros check`. Test: `safety_warns_on_non_crc_rmw` (cyclone/xrce
warn, zenoh + no-safety silent).

The CRC machinery lives in the **zenoh** shim only. cyclonedds and xrce have no
safety-e2e path; the axis no-ops there. Already documented in
`docs/reference/cyclonedds-known-limitations.md`. Tail: make the no-op **loud** —
when `[safety]` is declared for a system whose resolved RMW is cyclonedds/xrce,
`nros check` / the bake should WARN (the CRC is silently dead otherwise), mirroring
the rmw / dep-chain gate pattern (issue 0072 note).
- **Acceptance:** declaring `[safety]` on a cyclonedds/xrce system surfaces a
  warning at plan/check time; documented as unsupported.

### W3 — C++ safety transport e2e — DONE (2026-06-18)
**Landed:** `examples/native/cpp/safety-listener/` (CMakeLists forces
`NANO_ROS_SAFETY_E2E=ON`; main.cpp polls `Subscription::try_recv_validated`),
registered in `fixtures.toml` (native/cpp/zenoh), + `safety_e2e.rs::
test_cpp_safety_listener_validates_crc`. Verified locally: fixture builds clean,
e2e green — **`cpp safety: 3 crc-ok, 0 crc-fail`** (parity with the C path).

The native-C transport e2e (`tests/safety_e2e.rs::test_c_safety_listener_validates_crc`,
green: `c safety: 3 crc-ok, 0 crc-fail`) proves the validation path; the C++ ABI
calls the same `RmwSubscriber::try_recv_validated`, so no separate C++ e2e was
added. Add a C++ analog only if the C++ surface needs independent regression
coverage (a `examples/native/cpp/safety-listener` + a `cpp safety: N crc-ok` assert).
- **Acceptance:** decide yes/no; if yes, a green C++ CRC e2e fixture; if no, record
  the rationale (C ABI parity) and close.

### W4 — generic declared-feature config sugar
A `features = [...]` list over the `resolve_capability` registry
(cargo-nano-ros/src/capability_resolver.rs) — a generic surface for declaring
capability axes, of which `safety-e2e` is the first concrete one (RFC-0031
§Generalization future note). Lets future axes lower without per-axis plumbing.
- **Acceptance:** a system declaring `features = ["safety-e2e"]` lowers identically
  to the typed `[safety]` field; the registry drives both.

## Notes
Each W is independent. **W2** (loud no-CRC gate) is the lowest-risk, highest-signal
(prevents a silently-dead CRC). **W1** (threadx) carries the design weight (backend
wiring investigation). **W3/W4** are optional / forward-looking. On completion, the
§B boxes in issue 0076 close.
