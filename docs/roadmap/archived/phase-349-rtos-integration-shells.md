# Phase 349 — RTOS integration: make FreeRTOS an imported library like the rest

**Status (2026-10-19): ARCHIVED. W1 LANDED, W2 ABSORBED (W2b LANDED), W3
LANDED, W4 LANDED; W5/W6 parked — no user has asked for either, and W5 would
be a fourth build road.** Rewritten 2026-10-18 after a relevance review, which
measured each wave against
`main` at
`6486216ab7`, two months after the phase was written. The principle still holds
and the remaining work is narrower. See "Relevance review (2026-10-18)" below
for what changed and why.

**Implements:** [RFC-0072](../../design/0072-rtos-integration-nano-ros-is-a-guest.md).

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
[phase-351](phase-351-board-facts-and-site-config.md), which acted on
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

### W2b — Platforms name no backends — **LANDED (2026-10-18)**

- [x] `[build.zenoh]` leaves every platform file (the five under
      `packages/platform/nros-platform-*/` and `config/{bare-metal,generic}/`).
      The zenoh backend's own descriptor holds those rows, keyed by platform and
      capability. Platforms keep `[capabilities]`, `[arch.*]`, `compile` and
      `required_env`.

*Acceptance:* `git grep -l '^\[build\.' -- '*/nros-platform.toml'` is empty, and
the zenoh-pico define set of each built fixture is unchanged (compare the
compile commands before and after).

This extends phase-347's *core names no backends* to the platform layer.

**How it landed.**

- **Where the rows went.** Each block is a `[port.<platform>]` table in
  `packages/rmw/zenoh/nros-rmw-zenoh/nros-rmw.toml`, moved as text with its
  comments. The key is the platform's canonical name; an alias resolves, and a
  row for a platform nobody declares is a load error.
- **What stayed on the platform.** `arch` became the top-level `arch_profiles`,
  because `nros-board-common`'s FreeRTOS kernel and lwIP builds read it with no
  backend involved. `compile` and `required_env` stayed as top-level keys. The
  list cannot be derived from the `[arch.*]` keys: `bare-metal` defines four
  profiles and admits two.
- **Schema.** `PlatformConfigFile` lost `build`, so `deny_unknown_fields` makes a
  leftover `[build.*]` a load error. Rows parse as `PortRow`, also
  `deny_unknown_fields`, so a row naming `arch`, `compile` or `required_env` is
  refused rather than dropped. `PlatformsTree::as_platform_manifest(&rows)` joins
  the two halves into the same `PlatformEntry` every consumer resolved before.
  The `[rmw.*]` line parser in `cargo-nano-ros` skips the new tables, which a
  test checks against the real descriptor.
- **Proof the move changed nothing.** Every platform's parsed block is
  identical before and after, and so is every platform's resolved
  `for_platform()` output (651 lines of debug output on each side, diffed).
  That is the input `build_zenoh_pico_unified` compiles from, so the define set
  cannot have moved. It is stronger than comparing one fixture's compile
  commands.
- **Gates retargeted** (each was reading the platform files):
  - `check-zenoh-lane-ownership` reads `compiled_by` from the rows.
  - `check-arch-profile-resolution` reads `arch_profiles`.
  - `check-zenoh-platform-macros` attributes macros by row key.
  - `check-path-env-fingerprints` scans the backend descriptors too.
  - `check-capability-conditionals` joins the rows with each platform's
    capabilities.
- **Two of those gates went silently vacuous during the move** and printed OK.
  `check-zenoh-platform-macros` read no macros, and
  `check-capability-conditionals` counted 0 capability-gated rows. The second
  now fails on zero: the FreeRTOS row gates its netstack, so zero means the scan
  is broken, never that the tree is clean.
- **The drift gate's negative control is a unit test.** It used to be two
  build-stage rows (`zpico_drift_*`) that corrupted the posix platform file
  through `NROS_PLATFORMS_DIR`. That override cannot reach the rows, and a
  second override for one file is what phase-412 deleted. The check moved into
  `nros-zpico-build`'s `check_port_includes`, tested with a corrupted row and
  against the real rows. The fixture and its test are retired.
- **Correction.** The "cmake readers" this paragraph used to name do not exist:
  the cmake lane reads `zenoh-sources.txt`, never a platform file.

**Found for W3's third box.** The FreeRTOS port row puts
`{source:freertos-kernel}/portable/{env:FREERTOS_PORT}` on zenoh-pico's include
path. That is a second reader that assumes the port is a path, beside the
cargo road's `build.rs`. Moving the boards to upstream's enum must fix it too.

## W3 — Retire the kernel builder — **LANDED (2026-10-18)**

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
- [x] `FREERTOS_PORT` takes upstream's enum. Each board states its port in that
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

**How the third box landed (2026-10-18).** The enum is the board fact on BOTH
roads: the three `nros-board.toml` descriptors (cargo road and board facts) and
the four board CMake files state `GCC_ARM_CM3` / `GCC_ARM_CRX_NOGIC` /
`GCC_POSIX`. The enum-to-directory pairing has one resolver per language, both
reading upstream's table: `_nros_freertos_resolve_port` in CMake (with a public
`nros_freertos_port_dir()`), and `nros_build_paths::freertos_port` in Rust. The
Rust one serves the two FreeRTOS build scripts, zenoh-pico's size probe, and a
new `{freertos_port_dir}` interpolation token, which replaced the port row's
`{source:freertos-kernel}/portable/{env:FREERTOS_PORT}` (a NAME joined onto a
directory). The size probe was a second, hand-written copy of that join and the
first fixture build after the switch found it (`portmacro.h: No such file or
directory` in `size_probe.c`), which is why it now calls the resolver. A listed
path still works with one DEPRECATION per spelling; a vendor port upstream does
not list keeps its path as its only spelling (`A_CUSTOM_PORT`).

**Measured (2026-10-18), for the third box and W4 together.** All fifteen
FreeRTOS workspace fixtures build: C, C++, mixed and the realtime entries on
mps2-an385; mps3-an536 (two) and s32z270 C++; freertos-posix C and C++; and the
four cargo-road Rust images (`rust-qemu-freertos`, the realtime one and the two
bridge robots). `entry_matrix`'s three mps2 cells, the mps3-an536 CycloneDDS
entry and both freertos-posix entries pass. The Rust images' runtime tests were
NOT run: the bridge pair needs a host `qemu-br` (sudo) and the tier test a
compile-check fixture this run did not build.

**Scope: the CMake road only, by design.** The cargo road builds a pure-Rust
FreeRTOS image through `nros-board-freertos/build.rs`, which compiles the kernel
and lwIP with `cc`. There is no user project on that road: the board crate is
the host, so "the RTOS owns its own build" has nobody to own it. Driving
upstream's CMake from a build script would add CMake to a road that does not
otherwise need it. This is a decision, not an oversight. Revisit it if the two
source lists drift (a third port, or a kernel bump that adds a source file).

## W4 — A FreeRTOS integration shell, on the s32ds precedent — **LANDED (2026-10-18)**

- [x] `integrations/freertos/` for a user CMake project that already defines
      `freertos_kernel`, `freertos_config` and an lwIP target, which is exactly
      what upstream's own CMake produces. The shell consumes those targets and
      contributes nano-ros, the way `integrations/s32ds/` does for an S32DS
      project.
- [x] Configure-time diagnostics named in RFC-0072 §7 ("What fails, and how"): a required capability
      nothing provides, an include dir that does not exist.

After W3, nano-ros's own boards produce those same targets through upstream's
CMake, so the shell and the boards share one consumption path. The in-tree
fixture is then the shell's first user, and that is the acceptance:

*Acceptance:* `mps2-an385-freertos` builds through `integrations/freertos/`, and
a configure with a missing include dir fails naming it.

**How it landed (2026-10-18).** `integrations/freertos/NanoRosFreertos.cmake`
defines `nros_freertos_integrate([KERNEL] [CONFIG] [NETSTACK <t>|NONE] [NETIFS]
[LINK_LIBS] [LINK_OPTIONS])`. It composes `freertos_platform` before
`add_subdirectory(<nano-ros>)`, which is what puts the platform module into
board-less mode. All four in-tree boards call it in place of
`nros_freertos_compose_platform`, so all eleven CMake-road images above went
through the shell, mps2-an385 among them. The cargo road has no CMake project,
so it does not use the shell. The configure checks:

* a missing target, named with how upstream produces it;
* `ip_stack`, READ from the platform descriptor's `[capabilities]`, with no
  NETSTACK provider. `NETSTACK NONE` waives it for a host kernel that owns the
  sockets (freertos-posix);
* a missing include dir, named with the target that exports it, followed
  transitively through linked targets (upstream's `freertos_kernel` carries
  none itself);
* no `FreeRTOSConfig.h` in the config target.

The first real configure caught the include-dir scan's own bug. Upstream writes
its port-header table as ONE generator expression spread over several list
elements, so a per-element `$<` test read eleven other ports' directories as
plain entries and reported them missing. The scan now carries genex depth
across elements.

`just check freertos-shell-diagnostics` (fast line, `LANGUAGES NONE`, under a
second) configures twelve probe projects:

* the shell's three passing shapes, the multi-element genex included;
* its five failing ones;
* the port resolver's four cases.

Disabling the include-dir check, the once-only deprecation guard or the genex
depth tracking each turns it red.

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
  [#530](../../issues/archived/0530-freertos-port-two-vocabularies.md). The
  builder accepts upstream's enum as well as our path fragment.
* **Zephyr unselectable by the zpico resolver** —
  [#529](../../issues/archived/0529-zephyr-platform-knobs-never-resolve.md).
