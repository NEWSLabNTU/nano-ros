# Phase 349 — RTOS integration: make FreeRTOS an imported library like the rest

**Status (2026-08-12). PROPOSED — no code landed.** W1 is independently
valuable and unblocks the platform half of
[phase-348](phase-348-source-time-provider-discovery.md) W2.

**Implements:** [RFC-0072](../design/0072-rtos-integration-nano-ros-is-a-guest.md).

**The principle.** nano-ros is a library the user's project imports; the RTOS
owns its own build. Three of five platforms already work that way — Zephyr as a
west module, NuttX as an `apps/external/` app, ESP-IDF as a component, each
being build glue + Kconfig + the host's package manifest. FreeRTOS is the
anomaly: no shell, and `nros_freertos_build_kernel()` compiling the kernel from
`FREERTOS_DIR` + `FREERTOS_PORT`.

---

## W1 — The platform stops naming a stack

- [ ] `config/freertos-lwip/` → `config/freertos/`, with
      `names = ["freertos", "freertos-lwip"]` so every existing spelling
      resolves. No behaviour change.
- [ ] `names` on all seven platform descriptors; `requires_capabilities` where
      known.
- [ ] `platform` joins `check-provider-announcements.py`'s `FAMILIES` — one row,
      not a new gate.
- [ ] `package.xml` provisions for the platform packages (phase-348 W2's
      remaining half).

*Acceptance:* `nros ws providers --kind platform` lists seven; the gate compares
provisions against `names`; every board resolving `freertos-lwip` today still
resolves.

**Independent of the rest of this phase.** It is the naming fix alone, and it
is what unblocks phase-348.

## W2 — The stack becomes a declared fact

- [ ] `[board.integration]` carries `rtos` and `netstack` as facts, with
      `capabilities`.
- [ ] The six lwIP-specific lines leave `config/freertos*/` for the zenoh
      backend descriptor as a `(rtos, netstack)` port row.
- [ ] `freertos_plus_tcp` gains its row and becomes reachable — zenoh-pico
      already ships `system/freertos/freertos_plus_tcp/network.c`; nothing in
      `config/` can select it today.
- [ ] `[build.zenoh]` leaves **every** platform file. Platforms keep
      `[capabilities]`, `[arch.*]`, `compile`, `required_env`.

*Acceptance:* a fixture builds against `freertos_plus_tcp` without a new
platform directory — the property the old naming made impossible.

**This is where "platforms name no backends" lands**, extending phase-347's
*core names no backends*. Expect the same shape of work: a keying change, not a
schema design.

## W3 — Retire the kernel builder

- [ ] `nros_freertos_build_kernel()` / `nros_freertos_build_lwip()` deleted; the
      mps2-an385 fixture adopts upstream's own `CMakeLists.txt`
      (`add_subdirectory` + a `freertos_config` target + `FREERTOS_PORT` /
      `FREERTOS_HEAP` cache vars).
- [ ] `FREERTOS_PORT` stops being ours. Upstream owns that name and takes an
      **enum** (`GCC_ARM_CM3`); we take a **path fragment** (`GCC/ARM_CM3`)
      under the same name today, which fails confusingly for anyone arriving
      from upstream documentation.

*Acceptance:* the FreeRTOS fixtures build and pass with nano-ros compiling no
kernel source; `git grep -c 'portable/'` in our cmake is zero.

**Risk.** Upstream's port table is 1356 lines of generator expressions and only
*warns* on a cross build with no `FREERTOS_PORT`. Our two ports (`GCC_ARM_CM3`,
`GCC_ARM_CM7`) must be verified against it before deleting ours, not after.

## W4 — A FreeRTOS integration shell

- [ ] `integrations/freertos/` joins the other three, so the FreeRTOS row stops
      being the exception: CMake glue + Kconfig-equivalent + a manifest.
- [ ] Configure-time diagnostics RFC-0072 §5.3 names: unprovided capability, a
      `(rtos, netstack)` pair with no backend port, a missing include dir.

The last is not padding — the ST case has six include paths, several of which
are submodules that may be uninitialised, and the failure without a check is a
compiler error deep in vendor headers.

## W5 — The IDE drop-in

- [ ] `nros emit --board <b> --out <dir>` producing `include/`, `src/` (C to be
      compiled by their project), `lib/libnros_rust.a`, `nano_ros.mk`, and a
      `README-INTEGRATION.md` rendered with their paths.

The split is forced by the ABI rule, not chosen: the Rust half talks only
through the stable C ABI (RFC-0054) and prebuilds per triple; the C half must
see the user's own `FreeRTOSConfig.h` and `lwipopts.h`, and compiling it against
anything else is issue 0135's silent ABI break.

## W6 — The scaffolder (the UX half)

- [ ] `nros init --from-cube <project>` reading `.cproject` linked-resource XML;
      the MCUXpresso equivalent reading `prj.conf`.

**Why this is a wave and not a nicety.** The include set is not guessable: ST's
port directory is not derivable from the core (Cortex-M7 H7 examples use
`ARM_CM4F`), both config headers are application files, and two different files
are named `cmsis_os.h` with `-I` order deciding silently. Asking a user to get
that right by hand is the difference between "imported a library" and "spent an
afternoon".

---

## Order

```
W1 ──► W2 ──► W3 ──► W4 ──► W5
                       └──► W6
```

W1 stands alone and should land first regardless of the rest.

## Prerequisites

Two live defects would be inherited:

* **`FREERTOS_PORT`'s two vocabularies** — W3 fixes it, but anyone touching the
  FreeRTOS path before then should know.
* **Zephyr can never be selected by the zpico platform resolver.** `use_zephyr`
  is absent from the `platform_name` chain in `nros-zpico-build/src/runner.rs`
  and zephyr targets match `is_embedded_target()`, so the resolver returns
  `None` and falls to the env-only branch. `config/zephyr/nros-platform.toml`
  is the **only** platform file carrying `[knobs.zenoh.tx]` — `batch = true`, a
  phase-290 W5 promotion measured at 15–20× streaming — and it never applies.
  Neither knob has a `KCONFIG_KNOBS` row, so there is no fallback either.
  **Not yet filed as an issue.**

## Deliberately not here

* **A `netstack` provider kind with selection.** Every vendor has welded its
  choice; there is nothing to select. Capabilities give the decoupling, and a
  selector can wait for a real second stack on one RTOS.
* **Prebuilt Rust staticlib distribution** — W5 needs one binary; a published
  triple × feature matrix is a separate decision.
