# Phase 208 — follow-ups (issues not fixed in the closing commits)

Phase 208 closed all original A/B/C/D/E items + acc.4 + acc.5 at
`89f69d911`. A handful of issues the agents surfaced are real but didn't
land in that pile — listed here for the next pass instead of leaving them
buried in per-tutorial reports under `docs/roadmap/book-audit/`.

## Mechanical / wide-scope

### F1 — Empty `[workspace]` table missing on ~80 example `Cargo.toml` — closed

**Symptom.** `cargo build` from inside `examples/<plat>/<lang>/<example>/`
fails with `current package believes it's in a workspace when it's not;
workspace: /home/aeon/repos/nano-ros/Cargo.toml` when the example is
under a nested-checkout path (worktrees, vendored copies into a user
workspace). Hit by every acc.5 agent worktree; also breaks the
"standalone copy-out" promise the README + each example's CMakeLists
make when a user vendors the example into their own workspace.

Stage-2 audit already named this as **N2** in `phase-208-audit-findings.md`
(2026-05-30 batch-2 supplement) + the per-file note in P14 for
`first-node-rust.md`. The fix is one line per example `Cargo.toml`:

```toml
[workspace]
```

Approximately 80 dirs under `examples/`. Mechanical; no behaviour change
on a canonical clone.

**Closed (2026-05-30):** appended an empty `[workspace]` table (with a
`# Phase 208.F1` comment block explaining the why) to every example
`Cargo.toml` under `examples/` that didn't already have one — 76 files;
3 already had it. Each canonical-clone build still passes (verified on
`examples/native/rust/talker` + `examples/qemu-arm-baremetal/rust/talker`:
`cargo build --release` finishes green in 8–13 s); `cargo metadata`
confirms each example is now its own `workspace_root` and is no longer
adopted by the outer `nano-ros/Cargo.toml`. Honors the README's
"standalone copy-out template" promise + closes the F6 A7 troubleshoot
branch as a doc nit rather than a real failure mode. Outer `Cargo.toml`'s
`exclude = [...]` list stays in place — belt-and-suspenders, and removing
the list would churn the workspace's own resolve.

### F2 — `nros generate-rust` pre-step never named in embedded tutorials

**Symptom.** `bare-metal.md` (acc.5) — the example's `Cargo.toml` has
`[patch.crates-io.std_msgs] path = "generated/std_msgs"`. `generated/`
is gitignored and *not* pre-populated. The build.rs invokes codegen
automatically, so a regular `cargo build` works — but a reader who runs
the canonical steps in a fresh shell sees no obvious "where did
`generated/` come from?" answer. The tutorials say nothing about it
beyond the project-layout tree.

**Fix shape.** One-line note in the Project-layout sections of
`bare-metal.md`, `freertos.md`, `threadx.md`, `esp32.md`,
`integration-nuttx.md`, `integration-zephyr.md`: *"`generated/` is
populated by the example's build.rs on first `cargo build` (or by
`nros generate-rust` directly) — not committed."* No source change.

### F3 — `just doctor` `[MISSING] cargo-tools: nros` reinstall loop — closed

**Symptom (original).** `tier=default` reported `[MISSING] cargo-tools:
nros` even when `~/.nros/bin/nros 0.3.7` was installed and working;
remediation `just workspace cargo-tools` re-ran `install-nros.sh` which
short-circuited on `command -v nros` → user looped on reinstall instead of
fixing the actual problem (PATH didn't include `~/.nros/bin`).

**Root cause was not a stale in-tree probe target — it was PATH hygiene.**
The probe used `command -v nros`; `nros` lives in `~/.nros/bin/`; the
installer never added that to the user's shell rc; `just doctor` ran in a
shell that didn't see the binary. install-nros.sh's own `command -v nros`
early-out had the same blind spot.

**Closed (2026-05-30):**

1. **`scripts/install-nros.sh` writes the PATH export rustup-style.** Same
   strategy as `rustup-init`: detect the user's shell from `$SHELL`, pick
   the right rc file (`bash` → `~/.bashrc` / `~/.bash_profile`, `zsh` →
   `~/.zshenv`, `fish` → `~/.config/fish/conf.d/nros.fish`, fallback →
   `~/.profile`), show the line that would be appended, prompt Y/n on
   `/dev/tty`. Non-interactive runs (e.g. `curl … | sh` without
   `< /dev/tty`) and runs with `NROS_NO_MODIFY_PATH=1` /
   `--no-modify-path` print the manual `export PATH=…` hint instead of
   silently mutating the user's rc. `NROS_YES=1` / `-y` auto-confirms. An
   idempotence guard greps the rc for the line first so re-runs don't
   double-append.

2. **`just/workspace.just` doctor probe is store-aware.** The cargo-tools
   loop now treats `nros` separately from `cargo-nextest` /
   `cargo-llvm-cov` / `espflash` (which `rustup` PATH-installs). The probe
   resolves PATH → `${NROS_HOME:-$HOME/.nros}/bin/nros` and emits a
   distinct `[PATH] nros at ~/.nros/bin/nros but not on PATH — add: export
   PATH=…` status when the binary is found-not-PATHed. Mirrors
   `cmake/NanoRosGenerateInterfaces.cmake`'s resolver. User sees the
   actionable hint (PATH export, not reinstall) and the loop breaks.

N4 from the batch-2 supplement is folded in — `~/.nros/bin` now opt-in
auto-PATHs through the installer rather than only being a printed hint.

## Doc cleanup

### F4 — Stale `Published: 1` caveat in `first-node-{c,cpp}.md`

Both tutorials carry a "C/C++ talkers currently pre-increment so their
first banner is `Published: 1`" caveat. Post-208.D.9 (counter convention
sweep) the first line is `Published: 0` everywhere; the caveat is wrong
and contradicts the rest of the page. Delete the paragraph.

Hit by both acc.5 batch-1 reports.

### F5 — `ros2 topic echo` QoS-mismatch hint missing from C/C++ tutorials

`ros2 topic echo /chatter std_msgs/msg/Int32` (as written) silently
delivers nothing — nano-ros publisher is BEST_EFFORT, stock
`ros2 topic echo` subscriber defaults to RELIABLE. The talker IS
discoverable (`ros2 topic list` sees it). The working command needs
`--qos-reliability best_effort`.

Add the QoS flag to the published interop snippet in
`first-node-{c,cpp}.md`. The Rust starter has the same pattern (didn't
trip because the audit's Rust agent didn't reach the ROS 2 interop
section).

### F6 — Stale strings in `troubleshooting-first-10-min.md` — closed

acc.5 troubleshoot report flagged five real-error mismatches; rewrote
the page against current code (2026-05-30). Now four labelled sections
(A. Build failures / B. Binary runs but no output / C. ROS 2 side sees
nothing / D. Doctor + last-resort) with each branch quoting the **real
stderr** a grep can match against.

Closed items:

- **A2** uses the actual cmake string
  `nros (codegen tool) not found on PATH or in ~/.nros/bin` (from
  `cmake/NanoRosGenerateInterfaces.cmake:105` +
  `cmake/NanoRosBootstrapCodegen.cmake:53` + the Zephyr sibling). Drops
  the fictional `nros-codegen not found` text the doc invented.
- **B1** quotes the Rust panic literal
  `thread 'main' panicked … Failed to open session: Transport(ConnectionFailed)`
  (verified by the acc.4 run).
- **B2** matches the C `NROS_CHECK_RET` macro:
  `NROS_CHECK failed … nros_support_init(...) -> -4` with process exit
  `1` (not the fictional `nros_init -> -3` the old page claimed).
- **B3** acknowledges the C++ `NROS_CPP_RET_TRANSPORT_ERROR = -100` is
  the *result code*, not the process exit — the POSIX exit is
  `(unsigned char)-100 = 156`. New entry explaining the conversion.
- **B4** new branch on `NROS_LOCATOR` (canonical) + `ZENOH_LOCATOR`
  (legacy alias) overrides at run time.
- **A6** new branch on the `cargo build --features rmw-cyclonedds`
  cannot-link failure — Phase 175 requires the CMakeLists path. Cite
  the linker symbol (`undefined reference to
  nros_rmw_cyclonedds_register` / `dds_create_participant`) so the user
  can grep it.
- **A7** new branch on `current package believes it's in a workspace`
  (cargo workspace-walk hits the outer `nano-ros/Cargo.toml`; F1 is the
  root fix).
- **A8** new branch on the `direnv allow` reminder (Phase 208.D.1 made
  the common build sites autoresolve but the advice still helps for
  non-D.1 build sites + `FREERTOS_PORT not set`).
- **C2** new branch on the `ros2 topic echo` QoS-mismatch (nano-ros
  publisher BEST_EFFORT vs stock echo RELIABLE). F5 closed it in
  `first-node-{c,cpp}.md`; troubleshoot now carries the symptom→fix
  branch as a more generic landing spot.
- **D2** new branch on the `[PATH] nros at ~/.nros/bin/nros but not on
  PATH` diagnostic the F3 fix added — directs users at
  `scripts/install-nros.sh --yes` (the rustup-style rc-append).

The page stays the standalone first-10-min landing under
`book/src/getting-started/`. The broader
`book/src/user-guide/troubleshooting.md` (404-line deep-dive reference)
is unchanged; the cross-link at the bottom of the page still routes
post-first-build issues there.

### F7 — `installation.md` heads-up paragraph misses cyclonedds + `~/.nros/sdk`

- Names zenoh + xrce daemons but not cyclonedds (in-process; a Pattern A
  reader on `--rmw cyclonedds` searches for an absent daemon).
- `~/.nros/sdk` store path never named in prose; only `~/.nros/bin` is.

### F8 — `bare-metal.md` `-nic socket,model=lan9118,…` not in runner

`.cargo/config.toml` runner is bare `-kernel`; L137 claims LAN9118
wiring is configured. The `just qemu talker` recipe wraps the LAN9118
flags; the bare `cargo run` doesn't. Either document that `just qemu
talker` is the only working invocation, or extend the runner.

### F9 — `freertos.md` Run flow needs an inline-zenohd fallback

`just freertos zenohd` is now fixed (`build/zenohd/zenohd` → `zenohd`
in the recipe), but the Run section in the doc still presents
`just freertos zenohd` as the only step. A copy-out reader hitting a
zenohd-not-found situation (e.g. shim absent) has no hint that
`zenohd --listen tcp/127.0.0.1:7451 --no-multicast-scouting` is the
underlying invocation.

### F10 — `threadx.md` claims that don't hold

- L11–13: claims nros-cpp doesn't target ThreadX, but `build-fixtures`
  builds `threadx_cpp_*` and `riscv64_threadx_cpp_*`.
- L117–119: claims `nros setup threadx-linux` creates `tap-tx0`. It
  doesn't — talker runs via loopback fallback.
- L170–173: "Published: 0 within 3 seconds" — assumes a warm cache; cold
  build compiles ~80 s first.

### F11 — `threadx_riscv64 build-fixtures` C cyclonedds — closed

The Batch 2 audit agent reported the C cyclonedds regenerate failure.
Re-reproduced 2026-05-30: it doesn't fail today. Two bugs the original
agent hit were closed during the Phase 203 ThreadX-Cyclone default-on
work (commits `b7334bfbb` + the follow-up cleanup):

1. **`cmake/platform/nano-ros-threadx.cmake`** mutated `CMAKE_C_FLAGS`
   with the picolibc / THREADX / NETX `-I` paths, but Cyclone's nested
   `project(CycloneDDS ...)` re-runs CMake's compiler init and **resets
   `CMAKE_C_FLAGS`** to the toolchain baseline. ddsc's per-target FLAGS
   dropped the threadx includes and the ddsrt port couldn't find
   `tx_api.h` / `nxd_bsd.h`. Switched to directory-level
   `include_directories` / `add_compile_options` /
   `add_compile_definitions`, which propagate via directory properties
   and survive nested `project()` resets.
2. **`packages/dds/nros-rmw-cyclonedds/src/service.cpp:88`** called
   `std::strtoull` — picolibc's `<cstdlib>` on the rv64/threadx cross
   does **not** alias every C function into `std::` (`getenv` is in,
   `strtoull` is not). Switched to `::strtoull`, which resolves through
   `<stdlib.h>` on every target.

Verified 2026-05-30 by `rm -rf examples/qemu-riscv64-threadx/{c,cpp}/*/build-cyclonedds && just threadx_riscv64 build-fixture-extras`: exit 0, all five Cyclone binaries built
(`riscv64_threadx_{c,cpp,rust}_{talker,listener}_cyclonedds`).
The earlier "regenerate sub-step fails" symptom does not reproduce
on the current main; F11 is closed.

## Next pass

Bundle the F1–F2 mechanical items into one commit (single grep replace
across `examples/`). F3 is its own item. F4–F10 are per-doc rewrites
sharing the same `phase-208-followups.md` reference; can land in one
"phase 208 followup doc pass" commit. F11 is platform work.

The "0 BLOCKERS on any tutorial" acc.5 bar is met *today*; F1–F11 are
the next layer of polish, not blockers.
