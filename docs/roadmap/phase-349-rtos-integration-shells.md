# Phase 349 — RTOS integration: make FreeRTOS an imported library like the rest

**Status (2026-10-18). Rewritten after a relevance review; W1 LANDED, W2
ABSORBED, W3 in progress.** The review measured each wave against `main` at
`6486216ab7`, two months after the phase was written. The principle still holds
and the remaining work is narrower. See "Relevance review (2026-10-18)" below
for what changed and why.

**Implements:** [RFC-0072](../design/0072-rtos-integration-nano-ros-is-a-guest.md).

**The principle.** nano-ros is a library the user's project imports; the RTOS
owns its own build. Zephyr ships as a west module (`zephyr/`) and NuttX as an
`apps/external/` app (`integrations/nuttx/`). FreeRTOS on a vendor IDE already
works this way too: `integrations/s32ds/` takes the kernel, the drivers and the
network stack from the user's S32 Design Studio project and contributes only
nano-ros (RFC-0064). The anomaly that remains is nano-ros's OWN FreeRTOS boards,
whose cmake files compile the kernel and lwIP from a source list we maintain
(`nros_freertos_build_kernel()` / `nros_freertos_build_lwip()`), even though
both upstream projects now ship a build system.

---

## Relevance review (2026-10-18)

| wave | as written (2026-08) | measured now | decision |
| --- | --- | --- | --- |
| W1 | platform stops naming a stack | landed | done |
| W2 | stack becomes a declared fact | boards declare `supported_netstacks`, a project picks one under `[board_config.<board>] netstack`, and `resolve_netstack` refuses an unsupported pair (phase-351). Issue 1063 removed the unread `NROS_NETSTACK`. Issue 1143 picks the zenoh-pico platform define from the board's `ip_stack` capability | absorbed, except `[build.zenoh]` in the platform files (now W2b) |
| W2 | `freertos_plus_tcp` becomes reachable | still unreachable; no board lists it and no user has asked | dropped, see "Deliberately not here" |
| W3 | retire the kernel builder | four board cmake files still call it; 19 `portable/` references in our cmake | **kept, the main work** |
| W4 | a FreeRTOS integration shell | `integrations/s32ds/` exists for one vendor IDE; nothing for upstream FreeRTOS | kept, re-scoped on the s32ds precedent |
| W5 | `nros emit` IDE drop-in | does not exist; no user is asking | parked |
| W6 | `nros init --from-cube` | does not exist; no user is asking | parked |

The original W2 investigation (the carrier measurements, the `[env]` discovery
table, the corrosion lanes) is kept in git history at `6486216ab7` and in
[phase-351](archived/phase-351-board-facts-and-site-config.md), which acted on
it.

---

## W1 — The platform stops naming a stack — **LANDED**

- [x] `config/freertos-lwip/` → `config/freertos/` (now
      `packages/platform/nros-platform-freertos/`), with
      `names = ["freertos", "freertos-lwip"]` so every existing spelling
      resolves.
- [x] `names` on all seven platform descriptors.
- [x] `platform` joins `check-provider-announcements.py`'s `FAMILIES`.
- [x] `package.xml` provisions for all seven platform packages (phase-348 W2's
      remaining half).

`resolve_alias()` sits in `PlatformsTree::chain()`, which three of the four
lookups funnel through. `declared_arch_names()` does not, and
`freertos_lwip_resolves_both_declared_arches` addresses the platform by its
alias on purpose to keep that covered.

## W2 — The stack becomes a declared fact — **ABSORBED**

Delivered by other work, so nothing here remains open:

- **The board states which stacks it supports.**
  `supported_netstacks = ["lwip"]` in `nros-board.toml`. The project picks one
  under `[board_config.<board>] netstack`, and `BoardDescriptor::resolve_netstack`
  refuses a stack the board does not list (phase-351 W4).
- **No value without a reader.** Issue 1063 removed `NROS_NETSTACK`, which
  nothing read; each board's cmake file picks its own stack, and the validation
  is the half that does work.
- **The zenoh-pico platform header follows the board.** Issue 1143 replaced the
  lwIP-only define with `defines_conditional` on the `ip_stack` capability.

### W2b — Platforms name no backends

- [ ] `[build.zenoh]` leaves every platform file (the five under
      `packages/platform/nros-platform-*/` and `config/{bare-metal,generic}/`).
      The zenoh backend's own descriptor holds those rows, keyed by platform and
      capability. Platforms keep `[capabilities]`, `[arch.*]`, `compile` and
      `required_env`.

*Acceptance:* `git grep -l '^\[build\.' -- '*/nros-platform.toml'` is empty, and
the zenoh-pico define set of each built fixture is unchanged (compare the
compile commands before and after).

This extends phase-347's *core names no backends* to the platform layer. It is
a keying change in `nros-zpico-build` and the cmake readers, not a schema design.

## W3 — Retire the kernel builder — **IN PROGRESS (two of three landed)**

- [x] The CMake road builds the FreeRTOS kernel with upstream's own
      `CMakeLists.txt`: `add_subdirectory(${FREERTOS_DIR})` with a
      `freertos_config` INTERFACE target, `FREERTOS_PORT` in upstream's enum and
      `FREERTOS_HEAP`. Upstream names its library `freertos_kernel`, the same
      name every consumer already links. A path spelling is mapped to the enum
      through upstream's own port table (`portable/CMakeLists.txt`, parsed at
      configure), and a vendor port under `portable/` goes through upstream's
      `A_CUSTOM_PORT` hook.
- [x] The CMake road builds lwIP from upstream's `src/Filelists.cmake` lists
      (core, IPv4 core, API) plus the FreeRTOS `sys_arch.c` from contrib,
      instead of our source list. The lists are **parsed as text, never
      included**: including the file `configure_file()`s `init.h` and
      `lwip.Doxyfile` into the lwIP source tree.
- [ ] `FREERTOS_PORT` takes upstream's enum. Each board states its port in that
      vocabulary, and the path-fragment spelling becomes a deprecated alias with
      a warning, retired one release later (the phase-484 W8 pattern).

**The ports, checked against upstream 10.6.2's table before any deletion** (the
risk the original W3 named):

| board | our port (path) | upstream enum | in upstream's table |
| --- | --- | --- | --- |
| mps2-an385-freertos | `GCC/ARM_CM3` | `GCC_ARM_CM3` | yes |
| mps3-an536-freertos | `GCC/ARM_CRx_No_GIC` | `GCC_ARM_CRX_NOGIC` | yes, with `portASM.S` |
| s32z270-freertos | `GCC/ARM_CRx_No_GIC`, or NXP's `GCC/ARM_CR52_GIC` | `GCC_ARM_CRX_NOGIC`, or `A_CUSTOM_PORT` | yes; the NXP port goes through upstream's custom-port target |
| freertos-posix | `ThirdParty/GCC/Posix` | `GCC_POSIX` | yes, with `utils/wait_for_event.c` and `Threads::Threads` |

*Acceptance:* every FreeRTOS fixture builds and its QEMU / POSIX tests pass,
with nano-ros compiling no kernel or lwIP source list of its own:
`git grep -nE 'tasks\.c|MemMang|src/core/|src/api/' -- 'cmake/**' 'packages/api/nros-c/cmake/**'`
finds nothing. (The original check, zero `portable/` mentions, cannot hold:
the port-table parser has to name `portable/CMakeLists.txt`.)

**Measured for the two landed bullets (2026-10-18).** All nine CMake-road
FreeRTOS workspace fixtures build: mps2-an385 C, C++, mixed and the two
realtime C++ entries; mps3-an536 and s32z270 C++ (R-profile, port assembly
included); freertos-posix C and C++. `entry_matrix`'s three mps2 cells, the
mps3-an536 CycloneDDS entry and both freertos-posix entries pass.
`libfreertos_kernel.a` holds upstream's seven kernel units plus `heap_4` and the
port. The definitions that change `FreeRTOSConfig.h` moved to the
`freertos_config` INTERFACE (`nros_freertos_config_define()`), because
upstream builds the port as its own OBJECT library that never sees
`freertos_kernel`'s usage requirements (issue 1197's every-TU rule).

**Scope: the CMake road only, by design.** The cargo road builds a pure-Rust
FreeRTOS image through `nros-board-freertos/build.rs`, which compiles the kernel
and lwIP with `cc`. There is no user project on that road: the board crate is
the host, so "the RTOS owns its own build" has nobody to own it. Driving
upstream's CMake from a build script would add CMake to a road that does not
otherwise need it. This is a decision, not an oversight. Revisit it if the two
source lists drift (a third port, or a kernel bump that adds a source file).

## W4 — A FreeRTOS integration shell, on the s32ds precedent

- [ ] `integrations/freertos/` for a user CMake project that already defines
      `freertos_kernel`, `freertos_config` and an lwIP target, which is exactly
      what upstream's own CMake produces. The shell consumes those targets and
      contributes nano-ros, the way `integrations/s32ds/` does for an S32DS
      project.
- [ ] Configure-time diagnostics named in RFC-0072 §7 ("What fails, and how"): a required capability
      nothing provides, an include dir that does not exist.

After W3, nano-ros's own boards produce those same targets through upstream's
CMake, so the shell and the boards share one consumption path. The in-tree
fixture is then the shell's first user, and that is the acceptance:

*Acceptance:* `mps2-an385-freertos` builds through `integrations/freertos/`, and
a configure with a missing include dir fails naming it.

## W5, W6 — PARKED

`nros emit` (an IDE drop-in with `nano_ros.mk`) and `nros init --from-cube`
(reading a CubeMX `.cproject`) have no user asking for them. If either is
revived, it must first answer the canonical-build-path question: an IDE
drop-in is a fourth build ROAD, and a road must name how every knob reaches its
compiler. The `idf.py` road was deleted (RFC-0065 D3) because it carried none.
The original design notes stay in RFC-0072 §7 ("Host build is not CMake") and in git history.

---

## Order

```
W1 (landed) ──► W3 ──► W4
W2 (absorbed) ──► W2b      (independent of W3)
```

W3 first: it is the concrete defect, and W4's shell consumes what W3 produces.

## Deliberately not here

* **`freertos_plus_tcp`.** zenoh-pico ships the port, and no board, fixture or
  user needs it. `resolve_netstack` already refuses a stack a board does not
  list, so naming it today fails loudly rather than mis-pairing. Add it when a
  board does.
* **A `netstack` provider kind with selection.** Every vendor has welded its
  choice; there is nothing to select.
* **Prebuilt Rust staticlib distribution** — a separate decision.
* **The cargo road's kernel build** — see W3's scope note.

## Prerequisites — RESOLVED 2026-08-13

* **`FREERTOS_PORT`'s two vocabularies** —
  [#530](../issues/archived/0530-freertos-port-two-vocabularies.md). The
  builder accepts upstream's enum as well as our path fragment.
* **Zephyr unselectable by the zpico resolver** —
  [#529](../issues/archived/0529-zephyr-platform-knobs-never-resolve.md).
