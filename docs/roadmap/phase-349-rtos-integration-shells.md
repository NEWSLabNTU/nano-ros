# Phase 349 — RTOS integration: make FreeRTOS an imported library like the rest

**Status (2026-08-13). W1 LANDED — with it, [phase-348](phase-348-source-time-provider-discovery.md)
is complete. W2–W6 open.**

**Implements:** [RFC-0072](../design/0072-rtos-integration-nano-ros-is-a-guest.md).

**The principle.** nano-ros is a library the user's project imports; the RTOS
owns its own build. Three of five platforms already work that way — Zephyr as a
west module, NuttX as an `apps/external/` app, ESP-IDF as a component, each
being build glue + Kconfig + the host's package manifest. FreeRTOS is the
anomaly: no shell, and `nros_freertos_build_kernel()` compiling the kernel from
`FREERTOS_DIR` + `FREERTOS_PORT`.

---

## W1 — The platform stops naming a stack — **LANDED**

- [x] `config/freertos-lwip/` → `config/freertos/`, with
      `names = ["freertos", "freertos-lwip"]` so every existing spelling
      resolves. No behaviour change.
- [x] `names` on all seven platform descriptors.
- [x] `platform` joins `check-provider-announcements.py`'s `FAMILIES` — one row,
      not a new gate. 19 providers across 3 families, 44 names.
- [x] `package.xml` provisions for all seven platform packages — **phase-348 W2's
      remaining half is now done.**

*Acceptance, met:* `nros ws providers --kind platform` lists 8 provisions from 7
packages, `--resolve platform:freertos-lwip` resolves to `config/freertos`, and
the zpico drift gate + `nros-board-common` (28 tests) pass unchanged.

**`requires_capabilities` deferred to W2**, deliberately. `PlatformConfigFile`
is `deny_unknown_fields`, so the field must exist before a file may name it —
but nothing consumes a *requirement* until the integration declares what it
provides, and adding an unread field invites the dead-config problem
[#529](../issues/0529-zephyr-platform-knobs-never-resolve.md) is about.

### Alias resolution is the mechanism, and one lookup was blind to it

`names` alone would have broken every `freertos-lwip` lookup, because
`PlatformsTree` keys by DIRECTORY. `resolve_alias()` sits in
`PlatformsTree::chain()`, the one point `capabilities()`, `resolve_tx()` and
`capability_check()` all funnel through, so callers need no alias awareness.

**Except `declared_arch_names()`, which does not use `chain()`** — the arch
table is merged across all files and addressed separately, so it stayed
alias-blind. `freertos_lwip_resolves_both_declared_arches` caught it by
continuing to address the platform by its alias, which is why that test still
does so on purpose. The claim that `chain()` was the single funnel was written
in a comment before it was checked; it is true of three lookups out of four.

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

## Prerequisites — both RESOLVED 2026-08-13

* **`FREERTOS_PORT`'s two vocabularies** —
  [#530](../issues/archived/0530-freertos-port-two-vocabularies.md). The builder
  now accepts upstream's enum as well as our path fragment, so W3's move to
  upstream's `CMakeLists.txt` changes nothing for anyone already writing
  `GCC_ARM_CM3`.
* **Zephyr unselectable by the zpico resolver** —
  [#529](../issues/0529-zephyr-platform-knobs-never-resolve.md). Resolver fixed;
  the two knob sources are now compared by `check-zephyr-knob-agreement`.

  **The severity stated in this section's first draft was wrong.** It claimed
  the phase-290 15–20× streaming promotion never applies on Zephyr. It does —
  the C lane gets it from `zephyr/Kconfig` defaults forwarded by
  `nros_rmw_zenoh.cmake` — and there is no ABI split either, because
  `build_c_shim` is skipped on Zephyr and `rust_consts()` never emits
  `tx_batch`. The real defect was two sources for one fact, agreeing only by
  coincidence.

## Deliberately not here

* **A `netstack` provider kind with selection.** Every vendor has welded its
  choice; there is nothing to select. Capabilities give the decoupling, and a
  selector can wait for a real second stack on one RTOS.
* **Prebuilt Rust staticlib distribution** — W5 needs one binary; a published
  triple × feature matrix is a separate decision.
