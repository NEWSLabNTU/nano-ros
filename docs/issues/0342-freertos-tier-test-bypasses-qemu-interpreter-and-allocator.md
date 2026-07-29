---
id: 342
title: "orchestration_tiers_freertos bypasses both sanctioned seams: a hand-rolled qemu command with no bypass rationale, and the only bare port literal among 14 start_slirp call sites"
status: resolved
type: bug
severity: medium
area: testing
related: [rfc-0051, issue-0327]
---

## Finding (quick audit E8/E9 + deep audit E9, 2026-07-28)

One test file bypasses both test-harness seams, and in each case a sibling in the
**same file** shows the sanctioned way.

### 1. Hand-rolled emulator invocation (E9)

`packages/testing/nros-tests/tests/orchestration_tiers_freertos.rs:68` —
`multi_tier_freertos_firmware_builds_and_boots_run_tiers` runs

```rust
Command::new("timeout").args(["10","qemu-system-arm","-cpu","cortex-m3","-machine","mps2-an385", …])
```

outside the `nros_tests::qemu` interpreter, with **no sanctioned-bypass doc-comment**
(the E1-exception pattern used elsewhere, e.g. `zpico_drift_gate.rs:22`).

The next test in the same file (`:119`) uses
`QemuProcess::start_mps2_an385_networked` — which proves the interpreter covers this
board, so the bypass is not a capability gap. It also means this invocation misses
whatever the interpreter centralises: the `-icount shift=auto` convention
(`docs/reference/qemu-icount.md`), boot-deadline handling, and log capture.

### 2. The only bare port literal among 14 call sites (E8)

`orchestration_tiers_freertos.rs:116` — `ZenohRouter::start_slirp(7447)`.

Every one of the other 13 `start_slirp` call sites passes an allocator constant or a
derived helper. 7447 also lands **inside** the allocator's own FreeRTOS window
(7400–7799, platform index 1), so it can collide with an allocator-assigned port as
the matrix grows — the exact class phase-295 W4 eliminated by routing every baked port
through `nros_tests::alloc`. The fixture's deploy locator is hand-mirrored in a comment
at `:114` (`tcp/10.0.2.2:7447`), so the test and its fixture can silently disagree.

## Fix

1. Route the boot-only test through `qemu::QemuProcess` — add a non-networked
   `start_mps2_an385` variant with a boot deadline if one is missing — or add the
   E1-exception-style rationale the header currently lacks, naming why the interpreter
   cannot serve it.
2. Derive the port from `nros_tests::alloc::port_of(FreertosMps2, Rust, <workload>)`
   and re-bake the fixture's deploy locator from the same call, so the hand-mirrored
   comment at `:114` becomes unnecessary.

## Note

Both halves were found in the quick run and recorded in
`docs/development/audit-findings-2026-07-28.md`; the deep run re-derived the E9 half
independently and added the "sibling test in the same file uses the interpreter"
evidence. Filed now because neither half got an issue in the quick run's batch.

## Resolved (2026-07-29)

**1. The emulator bypass is gone.** The boot-only test runs through
`qemu::QemuProcess::start_mps2_an385` — which already existed, confirming the
audit's read that this was never a capability gap. The hand-rolled
`Command::new("timeout").args(["qemu-system-arm", …])` missed what the
interpreter centralises: the `-icount shift=auto` convention, boot-deadline
handling and log capture. The assertion also got stronger on the way: it now
WAITS for `Network ready.` with a deadline instead of grepping whatever a fixed
10-second `timeout` happened to capture.

**2. The bare port is gone.** `ZenohRouter::start_slirp(7447)` became
`start_slirp(ROUTER_PORT)`, where
`ROUTER_PORT = port_of(FreertosMps2, Rust, RealtimeTiers)` = **7891**. That
coordinate was free (the `Cpp` sibling is used by `freertos_core_pin_applied`)
and semantically apt.

Worth recording: 7447 was not merely unallocated. `platform_port_base` puts the
FreertosMps2 window at 7800–8199, so the literal sat inside a DIFFERENT
platform's window — a collision waiting for that platform's matrix to fill in,
which is worse than the "unallocated" the finding assumed.

**The hand-mirror is now checked.** The firmware BAKES the port into
`fixtures/orchestration_tiers_freertos/entry/Cargo.toml`, and a TOML literal
cannot call the allocator — so the pairing is exactly the kind of mirror that
rots. `assert_fixture_port()` reads that manifest and fails with a named
mismatch before booting, instead of letting the firmware dial a port nobody is
listening on and surface as `Transport(ConnectionFailed)` — which is what it
looked like mid-fix, when the prebuilt ELF still carried the old port.

Mutation-checked: reverting the fixture's locator to 7447 fails the guard
immediately (0.007s, before QEMU starts). Both tests pass after rebuilding the
firmware fixture.

## Note for whoever runs the fixture builder next

`compile-check-fixtures.sh` exits 1 on an UNRELATED fixture,
`l9_register_cpp`: `Unknown CMake command "nano_ros_auto_add_library"`. The
orch_tiers_freertos firmware itself rebuilt fine. Not investigated here — flagged
so it is not mistaken for fallout from this change.
