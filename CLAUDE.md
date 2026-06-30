# nano-ros

Lightweight ROS 2 client for embedded RTOS (Zephyr, FreeRTOS, NuttX, ThreadX). `no_std`.

This file is a **router + agent practices + pitfall index**, kept short because it is loaded
every session. Design rationale lives in RFCs, operational detail in `AGENTS.md` and `docs/`.

**Docs convention — three numbered series, do not mix them:**
- **Design decision** → an RFC in [`docs/design/`](docs/design/README.md) (`NNNN-slug.md`,
  living docs; `Draft`→`Stable`→`Superseded`). Whole-system view = `ARCHITECTURE.md`.
- **Planned / in-flight work** → a phase doc in [`docs/roadmap/`](docs/roadmap/) (work items +
  acceptance; names the RFC it implements; completed → `archived/`).
- **Known bug / limitation / tech-debt** → an issue in [`docs/issues/`](docs/issues/README.md)
  (`NNNN-slug.md` + frontmatter; `status: open`→`resolved`/`wontfix`; resolved → `archived/`).
  Issues cross-link the RFCs/phases that inform or close them.

**When you learn something durable, file it in the right series above and add only a one-line
pointer here — never grow CLAUDE.md with design/impl detail.**

## Where things live

| You need… | Go to |
| --- | --- |
| Finalized whole-system design | [docs/design/ARCHITECTURE.md](docs/design/ARCHITECTURE.md) |
| A specific design decision (stable vs evolving) | [docs/design/](docs/design/README.md) — numbered RFCs |
| A known bug / limitation / tech-debt (troubleshooting) | [docs/issues/](docs/issues/README.md) — numbered issues (open) + `archived/` |
| Build / test / SDK tiers / jobserver / zephyr versions | [AGENTS.md](AGENTS.md) + [docs/development/](docs/development/) + `just/*.just` |
| `nros setup` / provisioning / `nros-sdk-index.toml` | RFC-0014 + AGENTS.md “Toolchain & SDK Provisioning” |
| Feature axes (RMW × platform × ROS edition) | ARCHITECTURE §2 + RFC-0005, RFC-0006 |
| Platform/RMW impl notes + deep pitfalls | [docs/reference/platform-implementation-notes.md](docs/reference/platform-implementation-notes.md) |
| C/C++ integration shape | AGENTS.md “C/C++ Integration” + RFC-0018/0019 + [docs/reference/c-api-cmake.md](docs/reference/c-api-cmake.md) |
| User-facing workflow | [book/src/](book/src/) (`just book`) |
| Phase history / current work items | [docs/roadmap/](docs/roadmap/) (active) + `archived/` |
| Periodic tech-debt / antipattern / UX audit | [docs/development/codebase-audit-checklist.md](docs/development/codebase-audit-checklist.md) |
| Profile a build's time (passive, read-only) | `just profile <dir>` → `nros-build-profile` (phase-251); [book](book/src/user-guide/build-profiling.md) |

## Naming
- **nano-ros** — project name (prose, docs)
- **nros** — code shorthand (crates, Rust/C idents, `CONFIG_NROS_*`)
- **nano_ros** — C header dir, CMake targets (`NanoRos::NanoRos`), CMake fn (`nros_generate_interfaces()`)

Workspace: `packages/{core,zpico,xrce,dds,boards,drivers,interfaces,testing,verification,reference,codegen,cli}/`,
`examples/`, `third-party/` (gitignored SDKs), `zephyr/` module. Run `ls packages/` for the current
crate list. Layer map → RFC-0001; `packages/drivers/` category split → RFC-0012.

## Practices
- **Always `just ci` after a task.** **Never `sudo`** — tell the user.
- **Green CI locally BEFORE pushing — don't iterate on remote CI.** Run `just format`
  then `just ci` (or at least `just check`) locally and fix every failure first, so the
  push passes remote CI on the first try. `just ci` = `check` (fast + build, incl. embedded
  clippy + every per-feature/per-example clippy) + `rust-rtos-link-check` + `test-all` +
  `cyclonedds-ci`. Note: `check` runs clippy with `-D warnings`, so a toolchain bump can
  surface NEW pre-existing lints (e.g. rust-1.96 `unnecessary_cast` / `drop_non_drop` /
  `not_unsafe_ptr_arg_deref`); fix them locally rather than discovering them remotely. CI
  stops at the first failing step, so one fix can unmask the next — re-run until fully green.
- **`just format` before broad changes** (Rust + C/C++ + Python).
- **Always nightly for `rustfmt` / `cargo fmt`** — `rustfmt.toml` enables nightly-only options;
  stable produces different output. Run `cargo +nightly fmt`.
- **C/C++ style:** `.clang-format` LLVM-based, 4-space indent, 100-col.
- **Linear history:** `git pull --rebase` or `git fetch` + `git rebase`. Never merge unless asked.
- **Submodule rebase on superproject pull:** if a pull advances a submodule pointer AND local work
  exists in the submodule → enter it, fetch, rebase local onto upstream, check out the
  superproject’s expected commit, record the result in the parent. Never leave a submodule at an
  older local commit when the remote pointer advanced.
- **Vendored-fork branch workflow (cyclonedds, netxduo, …):** land fixes with linear history
  (commit in submodule → `git fetch origin` + `git remote prune origin` → `git rebase origin/<branch>`
  → push). **Push the fork branch FIRST, then bump the superproject pointer** to the pushed commit.
  **By default the agent does NOT push fork remotes** (they sit outside the trusted repo →
  exfiltration guard): the agent commits + rebases locally and leaves the branch ready; the
  maintainer pushes. The agent may push only when a scoped `Bash(git -C <submodule-path> push:*)`
  allow-rule exists — never a blanket `git push:*`.
- **Codegen + orchestration CLI lives in-tree at `packages/cli/`** (a sub-workspace, own
  `Cargo.toml`/`Cargo.lock`). Edits to codegen / `colcon_nano_ros` / orchestration land there; build
  via `just setup-cli`. The retired `packages/codegen` submodule is fully gone (no stray leftover).
  `packages/cli/` itself nests three submodules under `third-party/` + `testing_workspaces/`
  (`play_launch_parser`, `ros-launch-manifest`, `ros2_rust_examples`).
- **Don’t modify vendored/generated:** `third-party/`, `packages/interfaces/*/generated/`, build
  output — unless the task explicitly requires regeneration. Preserve worktree changes.
- **Examples are standalone copy-out projects** (`examples/<plat>/<lang>/<example>/`); no workspace
  walk-up. Non-example bins live under `packages/testing/{nros-tests/bins,nros-bench,nros-smoke}/`.
  Detail → RFC-0026 + `examples/README.md` coverage matrix.
- **Messages are generated** (`nros generate-rust` from `package.xml`) — never hand-write. Detail
  → RFC-0023 + [docs/guides/message-generation.md](docs/guides/message-generation.md).
- Unused vars: `_name` + comment, or `#[allow(dead_code)]` for test struct fields.
- Reusable tests → `packages/testing/nros-tests/tests/` (Rust) or `tests/` (sh). Temp tests → Bash
  then promote. Temp files in `$project/tmp/` (gitignored), not `/tmp`; use Write/Edit not heredoc.
- **Tests must fail on unmet preconditions** (`assert!`/`bail!`/`nros_tests::skip!`). Bare
  `eprintln!`+`return` reports PASS — never. Same for runtime: panic, not silent early-return.
- **No compilation inside tests** — never `cargo`/`cmake`/`idf.py`/`west build` at run time. Compile in
  the build stage (`build-test-fixtures` + `examples/fixtures.toml`); the test consumes the prebuilt
  fixture. "Does it compile?" intent → make it a build-step fixture and assert the artifact. → AGENTS.md Testing.
- **Test names describe behavior, not phase numbers** (`zephyr_xrce_service_e2e`, not `phase212_n9_…`).
  Phases go stale; cross-ref a phase in a doc-comment, never the identifier. → AGENTS.md Testing.
- **Sweep contract:** every `just <plat>` invocation needs `source ./activate.sh` first (PATH wires
  `nros`, `play_launch_parser`, `zenohd`). `just doctor` enforces it. The pre-218
  `export PATH="$HOME/.nros/bin:$PATH"` is insufficient.

## Pitfall index

One-liners; detail in the linked doc. (Many also captured in agent memory.)

- **After clone, run ONE of** `direnv allow` / `source ./activate.sh` / `source ./activate.fish`
  else `zpico-sys/build.rs` panics `"FREERTOS_PORT not set"`. Activate files are the env/PATH SSoT.
- **Zenoh pinned 1.7.2** (rmw_zenoh_cpp compat). zenohd from `third-party/zenoh/zenoh/`; zenoh-pico
  from `packages/zpico/zpico-sys/zenoh-pico/`. Tests auto-use `build/zenohd/zenohd`.
- **Rust edition 2024:** `unsafe extern "C" {}`, `#[unsafe(no_mangle)]`, explicit `unsafe {}` in
  `unsafe fn`. `nros-c` keeps `#![allow(unsafe_op_in_unsafe_fn)]`.
- **No POSIX-style Rust ctor sections on Zephyr/native_sim** — wire backend init explicitly
  (`nros_cpp_init` registers the linked CFFI backend; weak `nros_app_register_backends` default).
- **Domain ID:** compile-time on embedded (Kconfig / per-example `config.toml`), runtime env on
  native via `nros_tests::unique_ros_domain_id()`. → platform-implementation-notes.md.
- **`zpico_spin_once` on multi-threaded platforms uses `z_sleep_ms()`, not `select()`** (else
  `Promise::wait()` burns its budget in ~39 ms). → platform-implementation-notes.md.
- **FreeRTOS:** `APP_TASK_STACK` 64 KB (inline executor arena on stack) → "Invalid mbox" otherwise;
  IP-seeded `srand()`; poll-task priority ≥ 4; manual action server needs
  `try_handle_get_result()`. → platform-implementation-notes.md.
- **Zephyr POSIX:** raise `CONFIG_MAX_PTHREAD_MUTEX_COUNT` (zenoh-pico needs ~8+; default 5 fails
  with -80). → platform-implementation-notes.md.
- **NuttX spin uses `sem_timedwait`** (pthread condvar hangs). → platform-implementation-notes.md.
- **NetX Duo BSD `SO_RCVTIMEO` takes `nx_bsd_timeval*`, not `INT` ms** (deadlock otherwise).
  → platform-implementation-notes.md.
- **smoltcp multicast:** join the GROUP addr, not `0.0.0.0`; LAN9118 needs promiscuous in QEMU.
  → platform-implementation-notes.md.
- **QEMU:** `-icount shift=auto`; use `nros_tests::qemu::qemu_system_arm_cmd()`. →
  [docs/reference/qemu-icount.md](docs/reference/qemu-icount.md).
- **Embedded Cyclone:** transient samples use `ddsrt_{malloc,calloc,free}`, never libc — RTOS heap
  is separate. → [docs/reference/cyclonedds-known-limitations.md](docs/reference/cyclonedds-known-limitations.md).
- **XRCE:** flush `uxr_buffer_request_data` immediately; reliable `STREAM_HISTORY ≥ 2`.
  → platform-implementation-notes.md.

## Verification
Kani (bounded harnesses, `just verify-kani`) + Verus (unbounded proofs, `just verify-verus`).
Patterns + the `verify = true` footgun → [docs/guides/verus-verification.md](docs/guides/verus-verification.md).
