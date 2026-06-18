# Phase 260 — drop macOS host support

Status: **In progress (2026-06-18)** · Closes the §C macOS item of
[issue 0076](../issues/0076-followups-config-ssot-and-safety-e2e-arc.md).

> **Decision (2026-06-18).** nano-ros drops **macOS** as a supported host platform.
> Supported hosts are **Linux** (primary) and *BSD (POSIX path). Rationale:
> - **Unvalidatable.** There is no macOS CI runner, so the macOS-specific link
>   paths (`-force_load` cyclone branches, darwin cross-builds) ship un-run — the
>   exact gap the 0076 §C item flagged. Carrying platform code we can't test is a
>   standing wrong-copy / link-drift hazard.
> - **No demand.** The embedded RTOS targets + the Linux host cover the project's
>   audience; macOS was a dev-convenience target, not a deploy one.
> - **Maintenance.** Every link/section/codegen path forks for APPLE (libc++ vs
>   stdc++, macho `__mod_init_func` vs ELF init_array, force_load vs whole-archive).
>   Dropping it removes a whole conditional axis.
>
> POSIX-shared code stays; only macOS/APPLE/darwin-specific branches go.

## Work items

- [x] **W1 — release CI: drop darwin targets.** Remove the `x86_64-apple-darwin`
  (macos-13) + `aarch64-apple-darwin` (macos-14) matrix rows from `release.yml`.
  Keep linux x86_64 + aarch64. **DONE** (this phase).
- [x] **W2 — CMake: remove APPLE branches.** Root `CMakeLists.txt` — drop the two
  `elseif(APPLE)` cyclone `-force_load` branches (the Linux/BSD whole-archive path
  is the keeper) and the `NOT APPLE` guards on the `stdc++` propagation (simplify to
  the threadx guard only). **DONE** (this phase). Also sweep `cmake/*.cmake` +
  `cmake/platform/nano-ros-posix.cmake` for residual APPLE/Darwin handling.
- [ ] **W3 — Rust: remove macos cfg.** `nros-rmw-cffi/src/section.rs` (macho
  `__DATA,__mod_init_func` ctor branch), `nros-cli-core` planner.rs/generate.rs
  (`build.target.contains("apple"/"darwin")` target routing — make an apple target
  an explicit "unsupported host" error, not a silent route), `nros-zpico-build/
  src/runner.rs` (`#[cfg(target_os = "macos")]`).
- [ ] **W4 — Docs: stop advertising macOS.** ~10 book files
  (`introduction.md`, `concepts/platform-model.md`, `porting/overview.md`,
  `platform-guides/native-posix.md`, `reference/{supported-boards,platform-api}.md`,
  `user-guide/configuration.md`, `internals/rmw-backends.md`, …) list "Linux / macOS"
  as POSIX hosts — change to Linux (+ *BSD where accurate).
- [x] **W5 — Record the supported-host policy** in AGENTS.md (+ this decision is the
  SSoT). **DONE** (this phase).

## Acceptance
- No `apple`/`darwin`/`APPLE` branch in nano-ros source/CMake/CI (third-party
  vendored trees excluded).
- `release.yml` builds Linux targets only.
- Docs claim Linux/*BSD hosts, not macOS.
- 0076 §C macOS item closed (the unvalidatable branch is gone, not pending a runner).

## Notes
W1/W2/W5 land now (the load-bearing CI + link removal that closes 0076 §C); W3
(rust cfg) + W4 (doc sweep) are mechanical follow-ups tracked here.
