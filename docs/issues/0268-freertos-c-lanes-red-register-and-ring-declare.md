---
id: 268
title: "freertos C lanes RED: executor register_subscription -1 + z_declare_subscriber (ring) -128 — pubsub/service/action all deliver nothing"
status: open
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
