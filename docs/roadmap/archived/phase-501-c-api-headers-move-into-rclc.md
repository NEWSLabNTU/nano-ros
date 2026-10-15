# Phase 501 — The C API's headers move into `rclc`

**Status (2026-10-16). Complete — W1–W6 landed in #2270; the index pins release 0.6.0-nros2.** Implements the
[RFC-0089](../../design/0089-ros2-api-adoption-and-the-compile-or-conform-rule.md)
amendment "the C headers live under `rclc/`" (2026-10-15). Follows
[phase-499](phase-499-user-surface-entry-and-cpp.md), which left
`<nros/*.h>` internal "to be renamed into `rclc` later".

## Why

After phase-492 and phase-499 a C user includes only upstream's paths, and
every `rclc/*.h` / `rcl/*.h` is a 22-line shim that includes
`<nros/nros.h>` plus `<nros/rcl_compat.h>`. The declarations still live in
`packages/api/nros-c/include/nros/`, so the C API has two homes: the one the
user includes and the one that holds the content. A reader who follows an
include lands in `nros/`, and the directory name is the last `nros` spelling
on the C surface.

Measured on `origin/main` (`a39b85c5de`, 2026-10-15):

- `packages/api/nros-c/include/nros/` holds 39 headers plus `zephyr/app_config.h`.
- About 500 live include sites name one of them. The largest groups are inside
  nros-c itself (`nros.h` 69, `rcl_compat.h` 56, `types.h` 20), the message
  codegen templates (`rosidl-codegen`: `types.h` 38, `nros_config_generated.h`
  25, `cdr.h` 19, `view.h` 7) and the entry generator (`nros-cli-core`).
- The `nros_` identifiers a user could still write are already gone. The 88
  `nros_*` names in the hand-written headers are port seams
  (`nros_board_*`, `nros_platform_*`), generated-entry internals (`main.h`,
  `boot_config.h`, `app_main.h`), FFI glue (`nros_cpp_*`), or stale comments
  naming retired forwarders (`nros_node_init`, `nros_executor_add_*`). In user
  code (examples and the book's C blocks outside `porting/`), the non-seam
  hits are `nros_init` and `nros_call_service`, both in book snippets.

## Rulings (maintainer, 2026-10-15)

1. **Layout: `rclc/` owns the content.** `nros/<x>.h` merges into
   `rclc/<x>.h` where rclc has that header. A header with no upstream
   counterpart becomes `rclc/<x>.h` too. Generated, configuration and entry
   or port glue goes to `rclc/detail/`. No `nros/` directory remains in nros-c.
   No forwarding headers stay behind.
2. **Identifiers: only what a user writes.** Sizing, configuration, entry and
   port-seam names keep `nros_` / `NROS_`. The entry and boot names that
   phase-492 deferred (`nros_app_main`, `NROS_ENTRY_*`) became internal in
   phase-499, so they stay.
3. **C only.** nros-cpp's `include/nros/*.hpp` → `rclcpp/` is a follow-up.

## Header map

| From `nros/` | To | Why |
|---|---|---|
| `nros.h` (umbrella) | `rclc/rclc.h` | upstream's umbrella; its guard is already `RCLC_H` |
| `init.h` `node.h` `publisher.h` `subscription.h` `service.h` `client.h` `timer.h` `executor.h` `types.h` | `rclc/<same>.h` | rclc has the header; the content replaces the shim |
| `action.h` | `rclc/action.h`; `rclc/action_{client,server,goal_handle}.h` stay shims | rclc splits action across three; ours is one file |
| `lifecycle.h` | `rclc_lifecycle/rclc_lifecycle.h` | upstream's lifecycle package |
| `guard_condition.h` `clock.h` `log.h` `check.h` `component.h` `rosout.h` `bridge.h` | `rclc/<same>.h` | no upstream counterpart; user-reachable through the umbrella |
| `visibility.h` | `rclc/visibility_control.h` | upstream's name |
| `app_main.h` `main.h` `entry_config.h` `boot_config.h` `baked_rmw.h` `app_config.h` `zephyr/app_config.h` `sched_context.h` `tier_task_memory_{freertos,zephyr}.h` | `rclc/detail/<same>.h` | entry and board glue the build and generated entries include |
| `cdr.h` `view.h` `serialization_format.h` `declared_qos.h` `node_pkg.h` | `rclc/detail/<same>.h` | support for generated message and component code |
| `rcl_compat.h` | `rclc/detail/rcl_compat.h` | scaffolding with a demolition date (RFC-0089 "no compat layer survives") |
| `nros_generated.h` (cbindgen, committed) | `rclc/detail/nros_generated.h` | generated |
| `nros_config_generated.h`, `nros_config_generated_buildless.h` (per-build sizes header and its snapshot) | `rclc/detail/<same>.h` | generated; basenames kept because the sizes-header mirror gates, the NuttX `DEP_NROS_C_CONFIG_INCLUDE` channel and nros-cpp all name them |

Headers with no content of their own (`init.h`, `node.h`, `lifecycle.h`,
`clock.h`, `guard_condition.h`, `sched_context.h` held only an include of
`types.h`) were deleted; their includers now name `<rclc/types.h>`, or the
upstream path where one exists.

Every upstream-named content header (`<rclc/publisher.h>`, `<rclc/node.h>`,
...) still brings the whole API: its own declarations sit inside its guard,
and an include of `<rclc/rclc.h>` follows the guard, so the declarations are
complete before the umbrella reads them. `<rclc/types.h>` is the exception,
because every other header includes it first.

Generated basenames keep `nros_`: they are build artifacts no user types,
and renaming them would move every mirror gate for no reader.

## Work items

- **W1 — hand-written content moves.** `git mv` each non-generated header per
  the map, merge into the existing shims, rewrite every include in nros-c,
  nros-cpp, `c-stubs/`, tests, scripts and the cmake templates. Include
  guards take the new path. Acceptance: `just ci gate`; no tracked file
  includes a removed `nros/` path (`git grep` over the old names).
- **W2 — generated headers move.** cbindgen's output path, the per-build sizes
  header (cargo `nros-c-generated/rclc/detail/`, the cmake mirror in
  `NanoRosConfigHeaderMirror.cmake`, the `links` channel, the buildless
  snapshot), and the declared-QoS / declared-params headers that
  `NanoRosNodeRegister.cmake` writes. nros-cpp's own `nros_cpp_config_generated.h`
  stays under `nros/` with the C++ follow-up. (As landed: the declared-QoS and
  declared-params headers stay under `nros/` too — see Progress.) Acceptance: `just ci gate`,
  `check-sizes-header-mirrors`, one NuttX and one Zephyr C image build.
- **W3 — the emitters follow.** `rosidl-codegen`'s C templates and the
  `nros-cli-core` entry generator emit `rclc/...` includes. Bump
  `NROS_CODEGEN_VERSION` so stale generated trees regenerate (issue 1360).
  Acceptance: native C e2e and the C workspace fixtures pass on freshly
  generated trees.
- **W4 — identifiers.** Book snippets drop `nros_init` / `nros_call_service`
  for their rclc spellings. Comments that name retired forwarders are fixed
  or removed.
- **W5 — gates.** `check-user-surface` and `check-c-api-freestanding-headers`
  derive "internal" from `rclc/detail/` (plus nros-cpp's `nros/`). A new rule
  in `check-user-surface`: user code never includes `rclc/detail/`. Each gate
  keeps its negative control.
- **W6 — docs.** `book/src/` (porting pages included), `docs/reference/c-api-cmake.md`,
  AGENTS.md "C/C++ Integration", RFC-0089's "1. Headers" paragraph. Archived
  docs keep their historical paths.

## Out of scope

- nros-cpp's `include/nros/*.hpp` (ruling 3).
- Port-seam headers other packages own (`<nros/platform.h>`,
  `<nros/rmw_vtable.h>`, `<nros/transport.h>`): written by porters, they stay.
- Renaming `nros_*` C symbols the library exports (ruling 2).

## Progress

### 2026-10-15 — W1, W3–W6

- W1: 39 headers moved or deleted by one script; 349 tracked files had an
  include or path rewritten (`docs/`, `CHANGELOG.md` and archived records
  excluded). The `<rclc/rclc.h>` umbrella now also includes
  `<rclc/detail/rcl_compat.h>`, which every shim used to include beside it.
- W3: `check-codegen-version-surface` keys the surface on the header path,
  and a version-16 C tree includes `<nros/types.h>`, which no longer exists.
  So `NROS_CODEGEN_VERSION` and `NROS_CODEGEN_VERSION_MIN` are both 17 (16
  went to issue 1913 on `main` while this was in review; the
  13 → 14 precedent: a C-only change moves the minimum for all three
  languages). Regenerated: the surface baseline, the golden corpus, both
  buildless snapshots' anchors, the committed interface crates.
- W4: book snippets now use `rclc_client_call` and `rclc_support_init`, and
  four header comments that named retired forwarders now name the live ones.
- W5: `check-user-surface` derives the internal set from `rclc/detail/` too,
  and its self-test proves it refuses `<rclc/detail/app_main.h>`.
  `check-c-api-freestanding-headers` skips `rclc/detail/`: 64 public headers
  compile freestanding (57 before; the seven that became public, among them
  `rclc/log.h` and `rclc/check.h`, already passed through the umbrella).
- W6: the book's C API page, the Doxyfile inputs, the living RFCs, the
  API-parity ledger, `c-api-cmake.md` and the audit checklist.
  `docs/reference/c-api-rename-map.tsv` is left as it is: it is a frozen
  record, and its generator does not reproduce it on `origin/main` either.

### 2026-10-15 — W2, folded into the same pull request

W1's pull request was not pushed yet when W2 was ready, and shipping them
apart would have cost a second codegen bump: a C message header from W1's
bump includes `<nros/nros_config_generated.h>`, which W2 moves. One pull request,
one bump.

- `nros_build_paths::artifact` gains `C_HEADER_SUBDIR` (`rclc/detail`) and
  `CPP_HEADER_SUBDIR` (`nros`) beside the two directory names. Every writer
  already went through `Artifact::relative`, so the cargo copies moved with the
  constant. The cmake reader (`nros_generated_header_subdir`,
  `nros_generated_subpath`) and the Python one (`scripts/lib/artifact_layout.py`)
  read it from `artifact.rs`, like the directory names; the NuttX FFI build
  takes it from the crate.
- `mirror-generated-header.sh` takes `<dir>/<subdir>` as its `<gen-subdir>`
  instead of appending `nros` itself. Its callers, its self-test, the
  sizes-mirror gate's self-test and the resolved-seed test pass the full path.
- The stamp in `NanoRosConfigHeaderMirror.cmake` named its crate by walking
  three parents up from the header, which is one short under `rclc/detail/`;
  it matches the `<crate>/include/` component now.
- The committed `nros_generated.h`, sizes stub and buildless snapshot moved to
  `rclc/detail/`; nros-c has no `nros/` directory left.
- Not moved: `nros_declared_qos_generated.h` (one generated file both C and C++
  include) and `nros_declared_params_generated.h` (C++ only). Both are
  CLI-written build artifacts, not nros-c source, and they move with the C++
  follow-up rather than splitting one file's include spelling across languages.
- Three more scans keyed to the old directory, found by the second check run:
  `check-c-api-nros-names` read `include/nros/*.h` and, with that directory
  gone, passed over nothing; it walks every tracked nros-c header now (47
  include guards and build facts of the upstream-path shims entered its scope
  and were classified `build` in `c-api-rename-map.tsv`). `api-parity`'s
  ledger-arrow check globbed `include/*/*.h`, one level, and missed
  `rclc/detail/nros_generated.h`. Three synthetic stub include dirs
  (`check-cpp-hosted-minimal-libcpp`, `check-repr-memory-agreement`,
  `codegen-version-refusal-tests`) wrote their sizes stub under `nros/`.

### 2026-10-15 — release 0.6.0-nros2, and what the gate measured

The codegen bump (16 went to issue 1913 on `main` while this was in review, so
this is 17, minimum 17) put the index's `[tool.nros]` pin, release
`0.6.0-nros1` (codegen 14), outside the range this tree accepts:
`the_nros_smoke_check_expects_what_the_pinned_release_emits` refused it.
Ruling (maintainer): hold the pull request for a release. `release-nros.yml`
cut `nros-0.6.0-nros2` from this branch's pre-pin head (the tag names it), and the tag
keeps that commit reachable after the queue rebase-merges. The asset was
downloaded and its sha256 checked, and `bin/nros --codegen-version` prints 17.
The index now pins it, with `upstream` and `source.ref` at that commit.

Verification, after `play_launch` and `zenoh-pico` were checked out at their
pins (earlier runs had used wrong checkouts of both):

- `just ci gate`: the only reds in `check::build` were the smoke test above,
  green once the pin moved, and `zephyr_prjconf_requirements`, a 60 s timeout
  at load ~70 that passes alone in 1 s. `test-unit`, `test-lane-contracts` and
  `out-of-lane-deselected` were run after it stopped, and passed.
- `just test linux`, before the rebase: 2,758 passed. Of 14 reds, 4 are
  provisioning this host lacks (a NuttX tree, a patched QEMU). The other 10
  passed alone, 11 of 11.
