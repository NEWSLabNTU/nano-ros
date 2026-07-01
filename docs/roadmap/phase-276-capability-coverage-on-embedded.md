# Phase 276 — Capability coverage on embedded (lifecycle / params / safety / QoS / multihost)

Status: **Blocked (partial) — 2026-07-02** · Implements issue #102 (H1) · Informs RFC-0026,
RFC-0006 (feature axes).

> **Blocker found (issue #126).** The `nros::main!` **Zephyr** emit branch wires only
> register+spin — it emits none of `param_services_call` / `lifecycle_call` / `run_tiers` (those
> live only in the `OwnedSpin` arms). So **W1 (params), W2 (tiers), W3 (lifecycle) on Zephyr are
> blocked at the macro level**, not the fixture level — adding a fixture cannot express the
> capability. They need the Zephyr arm extended to OwnedSpin parity (params/lifecycle are a small
> emit change; tiers needs a `ZephyrBoard::run_tiers`). **W4 (safety/CRC), W5 (QoS), W6 (multihost)
> are node-level pub/sub and remain achievable on Zephyr today.** W1/W2 also land on FreeRTOS
> (OwnedSpin), though the phase's "Zephyr = richest embedded target" rationale wants Zephyr. See
> #126 for the exact `main_macro.rs` line evidence + fix direction. Zephyr provisioned + verified
> building on the dev host (native_sim `c/talker` links).
>
> **Reconciliation with the execution-model convergence (RFC-0015 Model 1).** W2 (tiers-on-Zephyr)
> is not a standalone fixture — it IS the embedded execution-model convergence: **phase-274 W3**
> (embedded C/C++ `run_tiers`, incl Zephyr `k_thread`) plus the Rust-Zephyr `run_tiers` gap (#126).
> Do it once there, for both language sides; don't duplicate a tiers demo here. Likewise W1/W3
> (params/lifecycle) on Zephyr wait on the `Framework::Zephyr` arm gaining `param_services_call` /
> `lifecycle_call` (#126, the cheaper half). **Net:** phase-276's Zephyr scope reduces to the
> pub/sub capabilities (W4/W5/W6); tiers/params/lifecycle-on-Zephyr fold into the phase-274 W3 /
> #126 convergence track.

> **Goal.** Every advanced runtime capability is currently exercised on **`native` only**. This
> phase adds embedded fixtures + runtime tests so each core capability is proven on at least one
> real embedded platform — closing the largest remaining #102 hole (H1). Scope is *capability × 
> embedded platform* coverage, not new example projects; where an example already exists it gets
> an embedded fixture + assertion, otherwise a minimal capability demo is added.

## Why (2026-07-01 re-audit of #102)

After Phase 275 closes the mechanical holes, the substantive gap is that **lifecycle, parameters,
safety/CRC, QoS-overrides, and multihost each have exactly one `native` fixture and no embedded
coverage.** A capability that only runs on the host desktop is not proven for the project's actual
target (embedded RTOS). Partial progress already exists to build on:
- **RT-tiers** reached FreeRTOS (`orch_tiers_freertos` + `orchestration_tiers_freertos.rs`) — the
  template for how a capability crosses to embedded.
- Basic pub/sub **workspace-entry** e2e reaches zephyr/freertos/nuttx/threadx (Phase 263 C2x) — the
  transport/boot path is proven, so capability demos can ride on it.

## Coverage target (capability → embedded platform)

Pick the two CI-runnable embedded targets as the default coverage pair, extend opportunistically:
- **Zephyr `native_sim`** — richest, runs the full RMW; primary embedded target for capabilities.
- **FreeRTOS QEMU** — second target; already carries the RT-tiers precedent.

| Capability | native (have) | Embedded target(s) to add |
| --- | --- | --- |
| RT scheduling tiers | ✓ | FreeRTOS ✓ (done) → add Zephyr |
| parameters (+ param services) | ✓ | Zephyr, FreeRTOS |
| lifecycle (managed node) | ✓ | Zephyr (uses cpp lifecycle wrapper, phase-270) |
| E2E safety / CRC | ✓ | Zephyr |
| QoS overrides | ✓ | Zephyr |
| multihost | ✓ | Zephyr + one peer (or two QEMU instances) |

## Work items

Each capability is one work item: an embedded fixture (row in the platform's fixture mechanism) +
a runtime test under `packages/testing/nros-tests/tests/` that asserts the capability actually
works on-target (not just builds). Model on `orchestration_tiers_freertos.rs`.

- **W1 — parameters on embedded.** Declare/get/set + param services on Zephyr `native_sim` (and
  FreeRTOS if cheap). Assert a param round-trips and a param-service query returns.
  *Depends on:* #80 (param persistence) is orthogonal — this exercises the runtime param API, not
  persistence; note the interaction.
- **W2 — RT-tiers on Zephyr.** Extend the FreeRTOS tiers demo to Zephyr; assert tier assignment /
  scheduling behavior.
- **W3 — lifecycle on embedded.** Managed-node transitions on Zephyr, using the phase-270 C++
  lifecycle wrapper. Assert configure→activate→deactivate→cleanup.
- **W4 — E2E safety / CRC on embedded.** Safety-listener CRC path on Zephyr; assert a corrupted
  frame is rejected and a good frame passes (mirror the native safety-e2e).
- **W5 — QoS overrides on embedded.** QoS-override pub/sub on Zephyr; assert the override takes
  effect (e.g. reliability/history behavior differs from default).
- **W6 — multihost on embedded.** Zephyr entry + a peer (native or a second QEMU); assert
  cross-host delivery. Heaviest — sequence last.

## Sequencing

W2 (extend existing tiers pattern — lowest risk) → W1 (parameters) → W3 (lifecycle, on phase-270) →
W5 (QoS) → W4 (safety/CRC) → W6 (multihost, heaviest). Each is independently landable; land the
fixture + test together so the capability is asserted, not merely built.

## Constraints

- Fixtures must be build- **and** run-verified on a **known-good machine** — the current dev host
  has failing RAM (issue #115); embedded QEMU runs there are doubly untrustworthy.
- Keep CI cost in mind (Phase 253 CI-lane tiering): embedded capability tests are heavier; tier
  them appropriately rather than loading the fast lane.

## Cross-links

Issue #102 (H1) · Phase 275 (mechanical gap-fill, #102 H2–H6) · Phase 270 (C++ lifecycle wrapper —
prereq for W3) · Phase 263 (workspace-entry e2e — the boot/transport path these ride on) ·
Phase 253 (CI lane tiering) · Phase 162 (RT scheduling harness — W2) · Issue #80 (param
persistence — orthogonal to W1's runtime param API).
