---
id: 268
title: "freertos C lanes RED: executor register_subscription -1 + z_declare_subscriber (ring) -128 — pubsub/service/action all deliver nothing"
status: resolved
type: bug
severity: high
area: freertos
related: [issue-0196, issue-0135]
---

## Finding (2026-07-25, full-sweep debt run)

All three freertos **C** e2e lanes fail on current main with FRESH fixtures
(full `just build-test-fixtures` at head, solo runs, 3 tries each — NOT the
flake-under-load class, NOT stale fixtures):

- `test_rtos_pubsub_e2e::…Freertos::…C` — listener boots, then:
  ```
  [nros] examples/qemu-arm-freertos/c/listener/src/main.c:128
      nros_executor_register_subscription(&app.executor, &app.subscription,
      NROS_EXECUTOR_ON_NEW_DATA) -> -1
  Network ready
  zpico: z_declare_subscriber (ring) failed: -128 for
      '0/chatter/std_msgs::msg::dds_::String_/*'
  ```
  readiness pattern `Waiting for messages` never appears.
- `test_rtos_service_e2e::…Freertos::…C` — client boots to `Network ready`,
  sends its one request, gets **0 responses** (server's queryable declare
  presumably dies the same way; harness prints client side only).
- `test_rtos_action_e2e::…Freertos::…C` — same shape.

Scope: **C + freertos only.** The freertos *Rust* lanes pass; the *native* C
zenoh lanes (same nros-c + zpico shim, POSIX build) pass; threadx/nuttx C
lanes pass. So the break is in the freertos build of the C ring-subscriber
path (or its generated sizing), not in the C API or the shim generally.

## Ordering clue

`register_subscription -> -1` happens BEFORE the network declare fails —
two distinct failures. `-1` from the executor register (slot/storage
exhaustion?) points at the generated executor-storage sizing for the
freertos C mirror — the sizes-header MIRROR class (0088/0114/recent
`NROS_CPP_EXECUTOR_STORAGE_SIZE` guarded-include work touched the
freertos/nuttx mirrors). The subsequent `z_declare_subscriber (ring)`
`-128` may be a knock-on (register left the ring descriptor unset) or an
independent zenoh-pico regression (the `87f7a84d` polled-read-drain bump
landed this window).

## Candidate windows (all parallel-session, 2026-07-17..25)

- phase-294 `50c248812` — C serialize convention change (services+actions).
- phase-296 W5.x freertos work (`f38cbea5c` core-pin fail-loud, tier spec).
- zenoh-pico bump `ef6aef50f` → 87f7a84d + the ThreadX read-guard zpico.c
  edits (`e24fa4f1d`, `a2fa18f8e`).
- The `NROS_CPP_EXECUTOR_STORAGE_SIZE` freertos/nuttx mirror fix (recently
  resolved issue) — if the C freertos mirror took the same treatment, a
  stale/missing mirror header in the fixture path is exactly this failure
  shape.

## Repro

```
just build-test-fixtures   # green
cargo nextest run -p nros-tests --test rtos_e2e \
  -E 'test(test_rtos_pubsub_e2e::platform_1_Platform__Freertos::lang_2_Lang__C)' \
  --no-capture
```

Bisect start: last known-green for these lanes is unclear (lane is
toolchain-gated; skips on hosts without arm-none-eabi-gcc — the 0232
false-green class for this family). Check `main.c:128`'s register call +
the freertos C executor storage header first; then the zenoh-pico pin.

## Root cause (2026-07-25, confirmed end-to-end)

NOT a code regression — the **sizes-header MIRROR race** (the
0088/0114/0122/0123 recurring class) on the freertos C cmake-fixture build
path, triggered by `63d271f43` (296-W3b.4 rate-monitor machinery):

1. The commit grew the Rust `Executor` (`monitor_states: [MonitorState;
   MAX_MONITORS]` + violations Vec) — `NROS_EXECUTOR_STORAGE_SIZE` 80696 →
   81032.
2. Incremental fixture rebuilds regenerated the probe's header but left the
   build tree's SHADOW COPY on the include path stale — the same tree held
   BOTH values (`nano_ros/packages/core/nros-c/nros_config_generated.h` =
   81032, `…/nros-c/include/nros/nros_config_generated.h` = 80696), and the
   TU compiled against the stale one.
3. C `nros_executor_t._opaque` (sized 80696) received a placement-new of the
   81032-byte Rust Executor → **336-byte overflow** → corrupted adjacent
   memory → `nros_executor_register_subscription -> -1`, the zenoh session
   killed its own link right after the transport handshake (router log:
   handshake OK, guest-side EOF 17 ms later), `z_declare_subscriber (ring)
   -128`.
4. **Clean rebuild ⇒ one consistent header ⇒ all three freertos C lanes
   PASS** (pubsub/service/action 3/3, solo).

Full-stack bisect (fixtures + harness rebuilt per step) converged on
`63d271f43`; zenoh-pico pin (87f7a84d drain), the 294 C-serialize
convention, port values, host firewall, and TX batching were each ruled out
empirically. (An earlier harness-only bisect result of `733dfd9ed` was an
artifact — a partial old-commit fixture build re-baked locator ports and
made good/bad track port-match instead of the defect.)

### Class gap left open

The build-side stale probe self-heals via cmake/ninja incremental — but the
incremental graph does NOT refresh the include-path shadow of
`nros_config_generated.h` on this path, so a museum header keeps compiling
(exactly the issue-0196 "probe and gate must watch the same inputs" rule,
build-side). The known class fix (the `nros_c_config_header` mirror target
as a prerequisite of every consuming TU — the phase-mixed-umbrella
treatment) needs applying to the freertos/plain cmake-fixture path.
Follow-up filed as the recurrence datapoint on the mirror class rather than
a new mechanism.

Diagnostic breadcrumbs worth keeping: ZENOHD_LOG=debug gives per-port
router logs in test-logs/fixtures/ (guest handshake visibility);
`strings <fixture> | grep tcp/` reads the baked locator; a mid-bisect
partial fixture build poisons every later run's port pairing — rebuild
fixtures at the FINAL checkout before trusting any verdict.
