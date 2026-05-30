# Phase 208 — follow-ups (issues not fixed in the closing commits)

Phase 208 closed all original A/B/C/D/E items + acc.4 + acc.5 at
`89f69d911`. A handful of issues the agents surfaced are real but didn't
land in that pile — listed here for the next pass instead of leaving them
buried in per-tutorial reports under `docs/roadmap/book-audit/`.

## Mechanical / wide-scope

### F1 — Empty `[workspace]` table missing on ~80 example `Cargo.toml`

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

### F3 — `just doctor tier=default` still probes the in-tree codegen path

**Symptom.** `tier=default` runs `_pinned-toolchain-files` (rustup network
call → SIGTERM after 3 min) and the in-tree `cargo install --path
packages/codegen/...` probe — which now reports `[MISSING] cargo-tools:
nros` even when `~/.nros/bin/nros 0.3.7` is installed and working. Sent
users loop on reinstall (acc.5 troubleshoot-10min report).

208.D.6 closed the rustup-hang half. The probe-target side still points at
the retired in-tree submodule. Switch the probe to PATH / `~/.nros/bin/`
(mirroring the canonical `find_program(nros …)` resolver from
`cmake/NanoRosGenerateInterfaces.cmake`).

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

### F6 — Stale strings in `troubleshooting-first-10-min.md`

acc.5 troubleshoot report:
- C/C++ transport-failure branch (L58–62) cites `nros_init` (real:
  `nros_support_init`), `ConnectionFailed = -3` (real: `NROS_RET_NOT_FOUND
  = -4`), `process exit -100` (real: `1` via `NROS_CHECK_RET`).
- CMake "missing codegen" branch (L30–38) keys on strings the build
  never prints (`nros-codegen not found`); the real literal is
  `nros (codegen tool) not found on PATH or in ~/.nros/bin`.
- `NROS_LOCATOR` (canonical override) is never mentioned — only the
  legacy `ZENOH_LOCATOR` is.
- Missing branches: `direnv allow` hint (the `FREERTOS_PORT not set`
  panic from `zpico-sys/build.rs`); the `cargo build --features
  rmw-cyclonedds` cannot-link-without-CMakeLists path (Phase 175).

Per-symptom rewrites against the current C/C++ source + post-D+E error
strings.

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

### F11 — `threadx_riscv64 build-fixtures` C cyclonedds CMake regenerate fails

Separately tracked; not a doc fix. The zenoh artifacts on the riscv64
threadx target build clean; cyclonedds C is the failure.

## Next pass

Bundle the F1–F2 mechanical items into one commit (single grep replace
across `examples/`). F3 is its own item. F4–F10 are per-doc rewrites
sharing the same `phase-208-followups.md` reference; can land in one
"phase 208 followup doc pass" commit. F11 is platform work.

The "0 BLOCKERS on any tutorial" acc.5 bar is met *today*; F1–F11 are
the next layer of polish, not blockers.
