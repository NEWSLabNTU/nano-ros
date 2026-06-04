# Phase 217 — ARM FVP local runtime

**Goal.** Run the Phase 117.10–117.14 ARM `FVP_BaseR_AEMv8R` (Cortex-A
SMP, Zephyr 3.7 floor) artifacts on a developer's machine end-to-end:
build → invoke the license-gated Arm Fast Models binary → stream UART
output → exit cleanly. Closes the runtime gap left by Phase 117 (build
smokes shipped, runtime tracked as the never-landed 117.13.) and stays
strictly local — Corellium AVH staging is a separate path (different
firmware format + remote provisioning + auth, out of scope here).

**Status.** OPEN. Track A landed 2026-06-03.

**Priority.** P2 — unblocks every other FVP slice (rust example, smoke
test, book chapter) and is the natural reference for boards that follow
the same hwv2 Zephyr shape (`fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`).

**Depends on.** Phase 117.14 (build smoke), Phase 199 (Zephyr 3.7 floor).

## Overview

The Zephyr FVP build path already works — `just zephyr build-fvp-aemv8r`
and `build-fvp-aemv8r-cyclonedds` produce a linked `zephyr.elf` on the
hwv2 board id (`fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`). What is
missing is the **run** half: invoking the Arm `FVP_BaseR_AEMv8R` binary
with the canonical `boards/arm/fvp_baser_aemv8r/board.cmake` `-C`
arguments + the built ELF, then capturing UART 0–3 in the host's
stdout.

Zephyr ships `cmake/emu/armfvp.cmake` which wraps the binary as a cmake
`run` custom target driven by the `ARMFVP_BIN_PATH` env var
(`find_program(... PATHS ENV ARMFVP_BIN_PATH)`). The whole runtime
slice collapses to:

1. Resolve `ARMFVP_BIN_PATH` (the directory containing the FVP binary)
   from one of `ARMFVP_BIN_PATH` / `ARM_FVP_DIR` / `PATH`.
2. `west build -d <build-dir> -t run` (cmake invokes the FVP).

Arm FVP is **license-gated** (`[gated.arm-fvp]` in
`nros-sdk-index.toml`); nano-ros does not download it. The user
accepts the Arm EULA, installs the FVP locally, and exports
`ARM_FVP_DIR` (or `ARMFVP_BIN_PATH`). Same policy as
`nv-spe-fsp` (NVIDIA Orin SPE) — gated installs stay out-of-band.

## Architecture

```
              user
                │
        ARM_FVP_DIR (or ARMFVP_BIN_PATH / PATH)
                │
                ▼
   scripts/zephyr/resolve-fvp-bin.sh
                │   (prints abs dir; exits 1 with hint)
                ▼
       just zephyr run-fvp-aemv8r{,-cyclonedds}
                │   ARMFVP_BIN_PATH=<dir>
                ▼
   west build -d <build-dir> -t run
                │
                ▼
   zephyr/cmake/emu/armfvp.cmake
                │   find_program + ARMFVP_FLAGS
                ▼
  FVP_BaseR_AEMv8R -a cluster0.cpu*=zephyr.elf \
                    -C cluster0.has_aarch64=1 \
                    -C bp.pl011_uart0.out_file=- ...
```

## Work Items

### 217.A — Local run recipes (LANDED 2026-06-03)

- [x] **217.A.1** `scripts/zephyr/resolve-fvp-bin.sh` — resolve
      `FVP_BaseR_AEMv8R` directory from `ARMFVP_BIN_PATH` (Zephyr
      canonical env, highest priority) → `ARM_FVP_DIR/models/Linux64_GCC-*/`
      (sdk-index `[gated.arm-fvp]`) → `dirname $(command -v
      FVP_BaseR_AEMv8R)` (PATH fallback). Prints absolute dir on
      stdout; exits 1 with EULA pointer on miss. ~80 LoC bash.
- [x] **217.A.2** `just zephyr run-fvp-aemv8r` — verifies
      `build-fvp-aemv8r-talker/zephyr/zephyr.elf` exists (else hints
      `just zephyr build-fvp-aemv8r`), resolves the FVP dir via 217.A.1,
      exports `ARMFVP_BIN_PATH`, runs `west build -d
      build-fvp-aemv8r-talker -t run`. Mirrors the
      `build-fvp-aemv8r` recipe's skip rules (no west / no workspace /
      no SDK / no ELF / no FVP).
- [x] **217.A.3** `just zephyr run-fvp-aemv8r-cyclonedds` — same shape
      over `build-aemv8r-cyclonedds-talker` (Phase 117.14 cpp/cyclonedds
      example).
- [x] **217.A.4** Example README pointer —
      `examples/zephyr/cpp/cyclonedds/talker-aemv8r/README.md`
      cross-references `just zephyr run-fvp-aemv8r-cyclonedds` under
      the Runtime section.

**Files:** `scripts/zephyr/resolve-fvp-bin.sh`, `just/zephyr.just`,
`examples/zephyr/cpp/cyclonedds/talker-aemv8r/README.md`.

### 217.B — `arm-fvp-installer` skeleton (OPEN)

`nros-sdk-index.toml` declares `installer = "arm-fvp-installer"` for
`[gated.arm-fvp]` but no script exists. Add a thin discovery script
(gated tools never download):

- [ ] **217.B.1** `scripts/installers/arm-fvp-installer.sh` — accepts
      `ARM_FVP_DIR` (required), validates `FVP_BaseR_AEMv8R` lives
      under it, symlinks the discovered dir to
      `~/.nros/sdks/arm-fvp/current/` so `nros setup --tool arm-fvp`
      reports a stable path. Refuses to run without `ARM_FVP_DIR`;
      prints the Arm download URL.
- [ ] **217.B.2** `nros doctor` check — `[gated.arm-fvp]` reports
      `arm-fvp-installer` as a known installer; `nros doctor` matches
      and flags missing `ARM_FVP_DIR` as a warning (not a hard fail,
      gated).
- [~] **217.B.3** Book/reference doc — point at
      `docs/reference/environment-variables.md` for `ARM_FVP_DIR` +
      `ARMFVP_BIN_PATH` (existing convention). **Doc-side LANDED
      2026-06-04 via 217.E.1 + 217.E.2 (`387321817`,
      `docs(217.E.1+E.2+B.3): ARM FVP book chapter + SUMMARY nav +
      reference cross-ref`).** Concretely shipped:
      `book/src/reference/environment-variables.md` carries a new
      "ARM FVP (`FVP_BaseR_AEMv8R`)" section pointing at the
      getting-started chapter; `book/src/reference/supported-boards.md`
      adds an Arm FVP row pointing at the same chapter;
      `book/src/getting-started/arm-fvp.md` is the canonical setup
      chapter (217.E.1 `[x]`); `SUMMARY.md` carries the
      "ARM FVP (Cortex-A SMP)" nav entry (217.E.2 `[x]`).

      **Why this remains `[~]` and not `[x]`:** B.3's body also
      promises the cross-references the *CLI* surfaces will need —
      `nros setup --tool arm-fvp` discoverability + `nros doctor`
      hints. Those CLI surfaces don't exist yet because 217.B.1
      (installer script) + 217.B.2 (`nros doctor` check) are both
      `[ ]` — license-walled on ARM FVP, blocked on a contributor
      with an accepted Arm EULA installing the FVP. Once B.1 + B.2
      land, B.3 closes with a trivial wording bump (no new doc
      pages, just pointers to the now-existing installer + doctor
      slots).

**Files:** `scripts/installers/arm-fvp-installer.sh` (new),
`docs/reference/environment-variables.md`.

### 217.C — FVP runtime smoke test (PARTIAL)

- [x] **217.C.1** `packages/testing/nros-tests/tests/phase217_c_fvp_runtime.rs`
      — discover FVP via the resolver; if missing, `nros_tests::skip!`.
      Asserts the cpp/cyclonedds talker prebuilt at
      `build-aemv8r-cyclonedds-talker/zephyr/zephyr.elf` (hint at `just
      zephyr build-fvp-aemv8r-cyclonedds` on miss), then drives the
      existing 217.A recipe (`just zephyr run-fvp-aemv8r-cyclonedds` →
      `west fvp run`). Greps the captured UART for the Zephyr boot
      banner + the talker's `Published:` line proving the publish loop
      ran. Wall-clock-bounded at 120 s with `ManagedProcess::Drop`
      killing the FVP process group on timeout/panic. Landed 2026-06-04.
- [x] **217.C.2** Per-platform nextest group `zephyr-fvp` with
      `max-threads = 1` (FVP licence may be node-locked + UART telnet
      ports collide on parallel runs). Routed via `binary(phase217_c_fvp_runtime)`
      override; added to `justfile`'s fast-path exclusion. Landed 2026-06-04.
- [ ] **217.C.3** Parity check vs Phase 175.A native cyclonedds Rust
      talker/listener: same `std_msgs/Int32` payload, byte-equal wire
      format (Phase 117 stock-RMW interop contract). **Blocks on:** 217.D
      (the matching Rust example on FVP hasn't shipped yet).

**Files:** `packages/testing/nros-tests/tests/phase217_c_fvp_runtime.rs`
(new), `.config/nextest.toml` group entry + `binary(phase217_c_fvp_runtime)`
override, `justfile` fast-path exclusion.

### 217.D — Rust example on FVP (OPEN)

The existing `examples/zephyr/cpp/cyclonedds/talker-aemv8r/` is the
carve-out preserved under CLAUDE.md "Examples = Standalone Projects."
Mirror it on the Rust side once Phase 212.N Entry pkg shape settles:

- [ ] **217.D.1** `examples/zephyr/rust/cyclonedds/talker-aemv8r/` —
      Entry pkg consuming the Phase 212.N `nros-board-fvp-aemv8r-smp`
      crate (already in tree). Same `std_msgs/Int32` payload as
      Phase 175.A.
- [ ] **217.D.2** Build recipe `just zephyr build-fvp-aemv8r-cyclonedds-rust`
      + run recipe `run-fvp-aemv8r-cyclonedds-rust`.
- [ ] **217.D.3** Smoke alongside 217.C.1.

**Files:** `examples/zephyr/rust/cyclonedds/talker-aemv8r/` (new tree),
`just/zephyr.just`.

### 217.E — Book chapter (LANDED 2026-06-04)

- [x] **217.E.1** `book/src/getting-started/arm-fvp.md` — setup
      (license + `ARM_FVP_DIR` / `ARMFVP_BIN_PATH`), build, run,
      expected UART output, ROS 2 interop check, cross-refs to
      Phase 117/217 + the example README. AVH cloud parity noted
      as out-of-scope.
- [x] **217.E.2** Updates `SUMMARY.md` (mdBook nav) — `ARM FVP
      (Cortex-A SMP)` entry under **Embedded Starters**.

## Acceptance

- [ ] `just zephyr run-fvp-aemv8r` boots the talker on the FVP, prints
      the Zephyr 3.7 boot banner + the talker UART output, exits clean
      on Ctrl-C.
- [ ] `just zephyr run-fvp-aemv8r-cyclonedds` publishes
      `std_msgs/Int32` over Cyclone DDS — verifiable in a sibling
      native_sim listener AND in stock `ros2 topic echo /chatter`.
- [ ] Both recipes skip gracefully with a clear hint when the FVP
      binary is not installed (matches every other `[gated.*]` tool
      policy).
- [ ] Phase 217.C smoke passes locally; gated `skip!` on CI.

## Notes

- The build recipes already work — Phase 217 is purely the
  **invocation + capture** half. The cmake `run` target lives upstream
  in `zephyr/cmake/emu/armfvp.cmake`; nothing needs to be patched in
  Zephyr.
- `boards/arm/fvp_baser_aemv8r/board.cmake` carries the canonical `-C`
  flags (UART pipe-out, GICv3, cache state, NUM_CORES from
  `CONFIG_MP_MAX_NUM_CPUS`). Do not duplicate them in the just recipe
  — `west build -t run` reads them through the build dir.
- Corellium AVH cloud FVP is **NOT** the same surface — it boots a
  `.coreimg`-packaged firmware via the AVH instance-create API
  (`/v1/instances` + `fwpackage`). Local FVP loads a raw ELF via
  `-a cluster0.cpu*=<elf>`. Document the AVH path separately as the
  Phase 190 successor (currently archived) or in 217.E cross-ref.
- Phase 117.13 was the historical placeholder for FVP runtime; this
  doc supersedes that line in archived `phase-117-cyclonedds-rmw.md`.
