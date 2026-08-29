---
id: 368
title: "`just setup all` on a clean Ubuntu 22.04 host: 7 of 18 modules fail, almost all on prereqs the index model was meant to absorb"
status: resolved
type: tech-debt
area: build
related: [rfc-0014, rfc-0062, issue-0336, issue-0196]
---

# `just setup all` on a clean host: 7 of 18 modules fail on undeclared prereqs

## What was done

Simulated the end-user path on a genuinely clean Ubuntu 22.04 host
(newslab-server-241: no ROS 2, no sudo password available to the session, stock
python 3.10 without pip/venv): `just setup all`, then `just doctor tier=all`,
then the doctor's own per-item remedies. Everything below is reproducible from
the transcripts; nothing was fixed by hand before recording it.

Result: **7 modules failed** (`workspace verification qemu zephyr xrce
rmw_zenoh esp_idf`), and the doctor pass then flagged **10**. The orchestrator
itself behaved well — honest per-module failure list, honest non-zero exit,
per-item remedies in doctor. The failures cluster into a few classes.

## Status (2026-08-03) — ALL EIGHT FINDINGS RESOLVED

- **F1 DONE (phase-327 W2, verified).** `just workspace setup` now runs every
  sudo-less installer FIRST and the system step LAST, and `apt-packages`
  delegates to `nros setup --system`, which DEGRADES: it exits 0 and PRINTS the
  missing packages + `sudo apt-get install …` (opt-in `--sudo` to run) instead of
  aborting the module. Verified: `nros setup --system` → exit 0, lists the 2
  unconfirmed apt entries.
- **F2 DONE (phase-327 W3 for the 2 named sites + this cycle for the class).**
  phase-327 pointed the riscv-gcc (threadx_riscv64) and idlc (cyclonedds) doctor
  remedies at `nros setup --tool <name>`. This cycle fixed the SIBLINGS the
  original left on apt/ROS — `arm-none-eabi-gcc` (orin-spe, qemu-baremetal,
  nuttx, freertos), `qemu-system-riscv64` (threadx_riscv64), `idlc` in freertos —
  each now leads with `nros setup --tool <name>`. Audited every remaining apt
  remedy: only `kconfig-frontends-nox` has no index tool, so it correctly stays
  apt.

**Full finding status (audited 2026-08-03):**
- **F1 DONE** — phase-327 W2 (verified).
- **F2 DONE** — phase-327 W3 + class-straggler fixes this cycle.
- **F3 DONE, both halves** — the in-repo declaration landed first
  (`[tool.qemu].system = ["libslirp"]` + `[system.libslirp]`, phase-327 W4), and
  the dist RELINK shipped in `qemu-11.0.0-nros4` (2026-08-29): the Linux dists
  carry their closure in `lib/` with rpath `$ORIGIN/../lib`, so both the
  declaration and the `[system.libslirp]` entry are GONE — a clean host needs no
  apt for qemu at all. Worth recording that the declaration was measured wrong:
  it named `libslirp` alone, while a bare `ubuntu:22.04` is missing SEVEN of the
  graph and dies on `libpixman-1.so.0` before slirp is ever reached. The finding
  came from one host that happened to have the other six. macOS followed in
  `-nros6` (issue 0887, resolved), by a launcher rather than an rpath.
- **F5 DONE** — `[system.]{python3-dev, libz3, libclang-dev, clang}` (phase-327 W4).
- **F6 DONE** — `scripts/setup-verus.sh` pins `release/0.2026.06.28.1847ab3` with
  a glibc guard (phase-327 W6).
- **F7 DONE** — `[system.]{aria2, gnu-parallel, python3-venv, python3-pip}`,
  `[rust.cargo-tool.nextest]`, `[python.]{colcon, clang-format}` (phase-327 W4/W5).
- **F8 DONE** — `ci-matrix{,-nightly}` now set `NROS_FIXTURE_LANE=tier2{,-nightly}`
  so the inner `_require-fixtures` requires the SAME lane `_lane-gate` proved,
  instead of the `all` stamp (this cycle).

- **F4 DONE** (phase-327 W5, verified 2026-08-03). Both halves landed:
  - The bundle `packages/cli/interfaces/` was completed — `example_interfaces`
    0.9.3, `action_msgs` 1.2.3, `unique_identifier_msgs` 2.2.1 added from ros2
    `humble` — and is wired as the ROS-less codegen search path
    (`ws.rs` "a host WITHOUT ROS 2 still resolves … packages/cli/interfaces/").
  - The narrowing guard is in `render_managed_entries` /`write_patch_block`: a
    managed crate whose generated/lookup path is missing is a HARD STOP
    (`bail!` "Refusing to write an incomplete [patch.crates-io] — a missing entry
    resolves from crates.io … which fails nowhere"), never a silent drop. The
    guard "fired on the remaining two" packages during the W5 bundle completion.

**All eight findings resolved.** F1/F2/F8 across phase-327 + this cycle;
F3/F5/F6/F7/F4 landed in phase-327 W4–W6 (the issue was simply never updated —
several were fixed the same day it was filed). Closing 0368.

## Findings

**F1 — one sudo step gates a chain of sudo-less installers (the big one).**
`just workspace setup` runs `apt-packages` (sudo) FIRST; under `set -e` its
failure aborts the module, so `install-ninja`, `install-make`,
`install-corrosion`, `rust-targets`, `cargo-tools` — all sudo-less — never
ran. That single ordering cascaded into the `zephyr` (no ninja), `esp32`
(no espflash/target), and `px4` (no ninja) failures. Running the skipped
installers by hand afterwards fixed all of those without sudo.
*Revision: run the sudo-less installers first; make `apt-packages` degrade to
a "run this yourself: sudo apt install …" listing instead of failing the
module.*

**F2 — remedies point at apt/sudo where an index prebuilt exists.**
- doctor `threadx_riscv64`: `[MISSING] riscv64-unknown-elf-gcc (run: just
  workspace apt-packages)` — while `[tool.riscv-none-elf-gcc]` has a pinned
  dist and was already sitting in `~/.nros/sdk/`.
- doctor `cyclonedds`: `[MISSING] idlc on PATH (install a ROS 2 /
  CycloneDDS, or set IDLC_EXECUTABLE)` — while `nros setup --tool cyclonedds`
  installs a dist that CONTAINS `bin/idlc` (verified).
- `just workspace install-play-launch-parser` source-builds with pyo3 (fails
  on stock hosts, see F5) — while `[tool.play_launch_parser]` has a prebuilt
  dist that the same run had already installed to the store.
*Revision: remedies name the index tool when one exists; the apt list keeps
only what the index cannot supply.*

**F3 — the prebuilt `qemu` dist cannot run on a clean host.**
`11.0.0-nros2` links `libslirp.so.0` dynamically; stock Ubuntu 22.04 does not
ship it (it arrives with apt's qemu). `just qemu setup` installs the dist and
then fails its own smoke check with the loader error. *Revision: bundle
libslirp in the dist (rpath `$ORIGIN/../lib`), or declare per-tool system
deps in the index and teach doctor to check them.*

**F4 — the bundled interface set is incomplete for the repo's own examples.**
`packages/cli/interfaces/` exists so codegen works "without a ROS 2
environment", but holds only `builtin_interfaces`/`std_msgs`/
`diagnostic_msgs`. The in-tree example workspaces require
`example_interfaces`, `action_msgs`, `unique_identifier_msgs` — so on a
ROS-less host, `nros sync examples/workspaces/rust` fails
(`no matching package named example_interfaces`) and — worse — **rewrites the
tracked `.cargo/config.toml`, silently dropping the patch entries it could
not generate** (the issue-0363 shape, leaf-local flavor). Verified fix shape:
a share-tree with those three packages' `msg/srv/action` files makes the full
workspace sync + build cleanly. *Revision: complete the bundle (the three
packages are small, licenses permitting), and make sync refuse to NARROW a
leaf patch table when generation failed.*

**F5 — pyo3 source builds need `python3-dev`; nobody says so.**
`install-play-launch-parser` (and `just setup-launch-resolve`, via
`ros-launch-manifest-check`'s hard `z3` dep → bindgen → libclang) die on
stock hosts with raw linker/bindgen errors (`-lpython3.10 not found`,
`Unable to find libclang`). Neither `python3-dev`, `libz3-dev` nor
`libclang-*-dev` appear in `apt-packages`, the index, or doctor. *Revision:
add them to the apt listing + a doctor probe; longer term, decide whether the
resolver build really needs the z3-backed checker.*

**F6 — `verus` fetches the unpinned latest release.**
Latest needs `GLIBC_2.39`; Ubuntu 22.04 LTS has 2.35 → hard failure. Kani,
right next to it, pins and passed. *Revision: pin a release known to run on
the oldest supported LTS, or degrade with a message (verification is not in
any CI tier gate).*

**F7 — assorted undeclared prereqs, one line each.**
`zephyr` prereq check demands `aria2c` (not in any list); `esp_idf` needs
`python3-venv` (idf's venv bootstrap fails); `rmw_zenoh` instructs
`sudo apt install python3-colcon-common-extensions` though colcon installs
fine via pip `--user`; `setup-clang-format` needs pip (absent on stock
python); `just format` needs GNU `parallel` (undeclared, uncheck-ed);
`just test-all` needs `cargo-nextest` (undeclared, unchecked).

**F8 — `just ci-matrix` gates fixtures twice, and the two gates disagree.**
`ci-matrix` = `_lane-gate tier2` (content-based freshness over exactly the
tier-2 coordinates — the right check) followed by `test-all`, whose
`_require-fixtures` checks only the `target/nextest/.fixtures-built` stamp
that the monolithic tier-3 `build-test-fixtures` writes. A tier-2 host that
built its fixture set per-family (as the tier ladder intends) passes the
lane gate and then dies on the stamp, with a hint telling it to run the
tier-3 build it deliberately avoided. *Revision: `ci-matrix` should invoke
`test-all` with the stamp check waived (its own gate already proved the
stronger property), or `_lane-gate` should write a tier-scoped stamp that
`_require-fixtures` accepts.*

## The consolidated apt line this host actually needed

```
sudo apt install qemu-system-arm qemu-system-misc socat gcc-riscv64-unknown-elf \
  libmbedtls-dev clang picolibc-riscv64-unknown-elf kconfig-frontends-nox \
  python3-dev python3-venv python3-pip libslirp0 aria2 parallel \
  libz3-dev libclang-14-dev python3-colcon-common-extensions
```

Everything else provisioned sudo-less once F1's ordering was worked around by
hand.

## Design direction

**RFC-0062 (unified dependency SSoT)** is the structural answer: every class
of dependency — pinned dists, source/submodules, OS packages (abstract keys
mapped per package manager), the Rust layer, the Python layer — declared in
`nros-sdk-index.toml`, with setup and doctor both DERIVED from it and the
sudo-requiring closure composed into one printed native command. The
findings below map onto it; the per-finding patches remain worthwhile
stopgaps if the RFC lands later.
**Implementation: phase-327**
(`docs/roadmap/phase-327-unified-dependency-ssot.md`).

## Suggested work order

1. F1 ordering + degrade (smallest change, biggest cascade removed).
2. F2 remedy re-pointing (doctor text + threadx/esp32 checks consult the
   store or name the `--tool`).
3. F4 bundle completion + the sync narrowing guard.
4. F3 qemu dist relink/bundle (needs an sdk-repo release; `-nros3` suffix).
5. F5/F7 apt-list + doctor additions (one sweep).
6. F6 verus pin.
