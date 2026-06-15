# Phase 251 — eliminate `--allow-multiple-definition`

Status: **DONE (2026-06-16)** · Implements RFC-0042 §D3 ("no papering-over" /
deterministic linking). Related:
[#70](../issues/archived/0070-staticlib-duplicate-symbols-gate-red.md),
[#71](../issues/0071-cpp-workspace-multi-std-staticlib-dup.md),
[#62](../issues/archived/0062-d3-completion-one-registration-path-and-link-manifest.md),
[phase-241](phase-241-d3-single-runtime.md).

> **OUTCOME.** Zero `--allow-multiple-definition` in the build system; the gate
> enforces it (empty allowlist). Waves: **W0** gate + audited allowlist (wired into
> `just check`); **W1** riscv64 board flag removed — `just threadx_riscv64 build`
> links clean (strong board mem over weak compiler_builtins, no flag); **W2** APPLE
> cyclonedds flag removed (force_load mirrors the flag-free Linux whole-archive;
> reasoned, no macOS runner to run-validate); **W3** the #70 link-determinism test
> rewritten for the single archive (one `libnros_c.a` links with `-u`, no flag, one
> `REGISTRY`) — `just check-staticlib-symbols` green, #70 resolved. The cpp-workspace
> multi-`std` case (**#71**, open) is a link *failure* being fixed without the flag,
> not a flag use — independent.

> **Invariant (goal):** ZERO `--allow-multiple-definition` in the nano-ros build
> system. A duplicate *defined* symbol at link is a **build error** — the safe
> default. The flag is dangerous precisely because it lets **two different
> functions with the same name** coexist and silently binds callers to whichever
> copy the linker happens to pick first (archive order / `--gc-sections`
> dependent) — the #48-class "registered into the wrong instance" / wrong-code
> hazard. It is forbidden, and enforced by a CI gate.

## Why

The single-runtime model (241.D3-rev, RFC-0042 §D3) already removed the flag from
the main C/C++ umbrella path: the umbrella `libnros_c.a` / `libnros_cpp.a` is the
ONE Rust staticlib on the link line (the zenoh/xrce backend is *bundled in*), so
`std` + `compiler_builtins` + the cffi `REGISTRY` appear once — no duplicates, no
flag. The flag now survives only as **scattered, unguarded exceptions** to that
invariant. Each is a real dup the flag masks; each masks the wrong-copy hazard.

Surfaced by the CI reorg: making `just check` the fast-gate SSoT exposed that the
link-determinism test (#70) is stale (it asserts the pre-single-runtime 2-archive
pair) and that the flag persists in three places.

## Surviving sites (the work)

| site | dup class | CI-testable | wave |
| --- | --- | --- | --- |
| `cmake/board/nano-ros-board-riscv64-qemu.cmake:268,431` | board's **strong** `memset/memcpy/memmove` vs compiler_builtins' **weak** ones | yes (`just threadx_riscv64 build`) | W1 |
| `CMakeLists.txt:264` (APPLE cyclone) | `force_load` + C++ runtime/dup | no (no macOS runner) | W2 |
| cpp workspace multi-staticlib (`libnros_cpp.a` + per-package FFI staticlib → two bundled `std`) | two Rust staticlibs → `rust_begin_unwind` etc. | yes | **#71 (owned elsewhere)** |

## Waves

### W0 — forbid-gate + allowlist (lock the invariant)
- `scripts/check-no-allow-multiple-def.sh` + `just check-no-allow-multiple-def`
  (wired into `just check`): grep the build system (`CMakeLists.txt`, `cmake/**`,
  `scripts/**`, `just/**`, board cmake, `*.cmake`) for `allow-multiple-definition`;
  fail on any occurrence NOT in `scripts/allow-multiple-def-allowlist.txt`. Each
  allowlist entry = `path # reason (owning issue)`. Mirrors the #50
  weak-symbol-allowlist SSoT pattern. Initial allowlist = the 3 sites above
  (riscv64 → W1, APPLE → W2, cpp → #71). Target: empty.

### W1 — eliminate riscv64 board flag
- The board's `memset/memcpy/memmove` (startup.c) are strong; compiler_builtins'
  are weak → strong-over-weak resolves with NO flag. Drop `--allow-multiple-definition`
  from `nros_threadx_compose_platform` LINK_OPTIONS + the `nros_board_link_app`
  INTERFACE note. Relink (`just threadx_riscv64 build`). If a residual dup
  surfaces, identify its class + fix properly (`-u`, source-exclude, or
  strong/weak) — never re-add the flag. Drop the riscv64 allowlist entry.

### W2 — eliminate APPLE cyclone flag
- `force_load` already pulls every cyclone object; the trailing
  `--allow-multiple-definition` is likely a leftover. Drop it (keep `force_load`).
  Not CI-testable (no macOS runner) — reason about it from the dup class (cyclone
  is C/C++, no Rust core closure, like the Linux path that links flag-free). If
  not provably safe without a macOS run, keep the allowlist entry with a reason +
  a follow-up issue rather than guessing. Drop the entry when removed.

### W3 — re-point the #70 link-determinism test (single-archive)
- Rewrite `packages/testing/nros-tests/tests/staticlib_duplicate_symbols.rs` for
  the single-runtime model: prove the one umbrella `libnros_c.a` links a host
  binary via `-u nros_rmw_zenoh_register`, NO `--allow-multiple-definition`,
  exactly one `REGISTRY`. Drop the obsolete 2-archive dup-diff. The fixture
  (`scripts/build/link-determinism-fixture.sh`) already builds the single archive.
  Greens `just check-staticlib-symbols` / closes #70's gate red.

### (parallel) cpp workspace — #71
- One Rust staticlib for the workspace cpp Entry (owned by #71). When it lands,
  drop the cpp allowlist entry → W-gate allowlist empty → invariant fully enforced.

## Acceptance

- `scripts/allow-multiple-def-allowlist.txt` reduced to empty (or only entries
  with a documented reason + open follow-up issue, e.g. an untestable macOS path).
- `just check-no-allow-multiple-def` green; gate in `just check`.
- `just check-staticlib-symbols` green on the single-archive flag-free link-proof.
- `just threadx_riscv64 build` green with the flag removed.
- No `--allow-multiple-definition` reachable in any default (non-allowlisted)
  link path.
