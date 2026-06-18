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

### W1 — threadx boards safety wiring
`nros-board-threadx-linux` and `nros-board-threadx-qemu-riscv64` expose no
`rmw-zenoh` board feature (their backend wiring is non-standard), so `[safety]` is
not advertised on them — the phase-252 descriptor gate **skips + warns** for these
boards (Wave 4 skip). To forward the axis, the threadx backend wiring must first be
understood: does threadx route through the zenoh shim (where the CRC lives) at all,
or a separate transport? If zenoh-backed, add the board `rmw-zenoh`/`safety-e2e`
feature edge so the descriptor gate advertises it; if not, document that safety-e2e
is unavailable on threadx (like cyclonedds/xrce, W2).
- **Acceptance:** `[safety]` on a threadx system either forwards (CRC validates
  e2e) or warns explicitly that the backend has no CRC path — no silent skip.

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

### W3 — C++ safety transport e2e (optional CI coverage)
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
