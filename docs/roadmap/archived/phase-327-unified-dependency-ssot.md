# phase-327 — unified dependency SSoT (implement RFC-0062)

**Implements:** [RFC-0062](../design/0062-unified-dependency-ssot.md)
**Resolves:** [issue 0368](../issues/0368-setup-all-on-a-clean-host-seven-modules-fail-on-undeclared-prereqs.md)
**Status:** COMPLETE and ARCHIVED (2026-08-30). W4's second half — "the ldd
audit of the other dists", open since 2026-08-01 — was run on 2026-08-30 and
found the same class in five more dists plus two binaries that could not start
on a stock 22.04 host (`openocd` on `libftdi.so.1`, `arm-none-eabi-gdb` on
`libncursesw.so.5`). Declared, reported at the point of use, and gated by
`check-dist-runtime-deps`; see issue 0926. The `-nros3` rpath re-cut is NOT
done and is now issue 0928 — it is a nano-ros-sdk change rather than work this
repository can carry, which is why the phase archives with it outstanding
rather than staying open indefinitely against another repo.

**Original status (2026-08-01):** Complete but for the qemu dist re-cut (W4's
-nros3 rpath re-cut needs the sdk repo and is the one open item)
**Informed by:** RFC-0014 (the index this extends), issue 0196 (a gate must
cover the class it enforces — here applied to doctor probes), issue 0363
(sync must not narrow a tracked patch table on failure — W5's guard).

## Why

Issue 0368's clean-host walk: `just setup all` failed 7 of 18 modules,
`just doctor tier=all` then flagged 10 — almost entirely on dependencies
declared nowhere, declared Debian-only, or remedied with hand-written text
that had drifted from what the index already provides. RFC-0062 decides the
fix: `nros-sdk-index.toml` declares EVERY dependency class; setup and doctor
both derive from it; the sudo-requiring closure is composed and printed,
never allowed to abort the sudo-less remainder.

## Ground rules (from the RFC)

- Abstract keys for `[system.*]` (`libslirp`, `gnu-parallel`,
  `kconfig-frontends`), not Debian package names.
- Remedies are COMPUTED from entries. A hand-written remedy string for an
  indexed dependency is a defect after W3.
- `nros setup` never runs the privileged closure without `--sudo`; it prints
  one native command per detected manager.
- rosdep is an optional backend (W6), never a prerequisite.

## Work

1. **W1 — index schema + full inventory move.** — **DONE** (2026-08-01; the
   pinned-toolchain channel SSOT stays tools/rust-toolchain.toml with the
   index entry in declared lockstep). Add `[system.*]`,
   `[rust.*]`, `[python.*]` tables and per-`[tool.*]` `system = [..]` to
   `nros-pkg-index` (parse + validate; unknown fields rejected loudly).
   Seed entries from the measured inventory:
   - `[system.*]`: the `apt-packages` list (socat, libmbedtls-dev, clang,
     qemu-system-arm, qemu-system-misc, gcc-riscv64-unknown-elf,
     picolibc-riscv64-unknown-elf, kconfig-frontends) **plus every 0368
     discovery** (python3-dev, python3-venv, python3-pip, libslirp, aria2,
     gnu-parallel, libz3-dev, libclang-dev), each with apt/dnf/pacman/brew
     mappings and a `check` probe (cmd / sharedlib / pkg-config).
   - `[rust.*]`: the pinned toolchain + components
     (`rust-pinned-toolchains`), the target list (`rust-targets`), the cargo
     tools (`cargo-tools`) **plus `cargo-nextest`** (required by `test-all`,
     declared nowhere today).
   - `[python.*]`: west, colcon, the clang-format wheel, the px4
     requirements set.
   Acceptance: `nros setup --list` shows the new classes; a schema test
   round-trips every entry; `git grep` finds no OS package name in
   `just/*.just` that lacks an index entry (the sweep is the gate).

2. **W2 — resolver + printer; fix the F1 cascade.** — **DONE** (2026-08-01:
   `nros setup --system [--check|--sudo]`; workspace setup reordered
   sudo-less-first; apt-packages is the printer). Package-manager
   detection (`os-release` + `command -v`), `nros setup --system [<scope>]`
   resolving a scope's `[system.*]` closure into one native command:
   printed by default, executed only under `--sudo`. Reorder
   `just workspace setup` so every sudo-less installer runs BEFORE the
   system-package step, and `apt-packages` becomes a thin caller of the
   printer (kept as a name so muscle memory and docs survive).
   Acceptance: on a host missing system packages, `just workspace setup`
   completes every sudo-less step, prints the one-liner, and exits 0 with a
   clearly-marked "N system packages pending" summary; the 0368 cascade
   (zephyr/esp32/px4 failing for want of ninja/targets) is unreproducible.

3. **W3 — doctor derives from the index.** — **DONE** (2026-08-01):
   `nros setup --check` walks ALL classes with computed remedies (rustup/
   cargo/pip/native-command per entry); workspace doctor derives from
   `--system --check`; zephyr/rmw_zenoh/esp_idf prereq sites converted; the
   whole-tree sweep removed every hand-written `sudo apt` remedy from just
   recipes and `check-sysdep-remedies` (in `check-fast`) bans the class —
   it caught three more sites (incl. an undeclared doxygen) on its FIRST
   run, before it was even wired. All THREE measured-wrong remedies
   re-pointed at index tools (cyclonedds idlc, play_launch_parser, and — by
   the parallel session — threadx_riscv64's riscv gcc, which now names
   `nros setup --tool riscv-none-elf-gcc` and accepts the xPack dist as
   satisfying the check). A generic walker runs each
   entry's `check` and prints the computed remedy (`nros setup --tool X`,
   the composed native command, `rustup target add …`). Convert the module
   doctors; delete the three measured wrong remedies (threadx_riscv64's
   riscv gcc → the store dist that already exists; cyclonedds' idlc →
   `nros setup --tool cyclonedds`, whose dist CONTAINS idlc;
   play_launch_parser → the prebuilt dist, not the pyo3 source build).
   Acceptance: `just doctor tier=all` output contains no remedy string that
   names apt for a dependency with an index dist; a new lint fails when a
   module doctor probes a dependency with no index entry (issue-0196 rule).

4. **W4 — dists declare their runtime system deps.** — **DONE for the
   declaration + probe** (2026-08-01: `[tool.qemu] system = ["libslirp"]`;
   `nros setup --tool` bails naming the missing package + the composed
   command). REMAINING: the -nros3 rpath re-cut (sdk repo) + ldd audit of
   the other dists.
   `[tool.qemu] system = ["libslirp"]` (+ audit the other dists' `ldd`
   closures). `nros setup --tool` checks/prints them; doctor probes them.
   Re-cutting the qemu dist with `$ORIGIN`-rpath'd libslirp (making the dep
   vanish) is a follow-up for the sdk repo — index suffix `-nros3` when it
   lands; the DECLARATION ships now either way.
   Acceptance: on a host without libslirp, `nros setup --tool qemu` says so
   before the smoke check fails, naming the package.

5. **W5 — complete the bundled interface set + the sync narrowing guard** — **DONE** (2026-08-01): both acceptance checks verified live (ROS-less sync green across the rust workspace; a forced generation gap turns sync red with the tracked table byte-identical, `narrowed_generated_entries` unit-tested both directions).
   (0368-F4). Add `example_interfaces`, `action_msgs`,
   `unique_identifier_msgs` share trees to `packages/cli/interfaces/`
   (licenses are Apache-2.0; record upstream refs in the README), so every
   in-tree example workspace syncs on a ROS-less host. And make `nros sync`
   REFUSE to drop entries from a tracked leaf `.cargo/config.toml` patch
   table when the corresponding generation failed — fail loud instead
   (the issue-0363 shape, leaf-local flavor, observed live in 0368).
   Acceptance: `nros sync examples/workspaces/rust` succeeds with no
   `AMENT_PREFIX_PATH` and no ROS install; a forced generation failure
   leaves the tracked patch table byte-identical and the sync red.

6. **W6 — verus pin + optional rosdep backend.** — **DONE** (2026-08-01,
   merged from two sessions): `scripts/setup-verus.sh` pins via
   `VERUS_VERSION` (`=latest` to opt out) with an informative glibc degrade
   instead of a raw loader crash (parallel session), and the DEFAULT pin is
   `release/0.2026.06.28.1847ab3` — bisected as the NEWEST release that
   RUNS on glibc 2.35 (05.17 + 06.28 run; 07.05/07.12/07.18/07.27 demand
   GLIBC_2.39), so 22.04 gets a working verus, not a graceful failure.
   Two pre-existing script bugs fixed: the fresh-install toolchain parse
   never matched verus's "required rust toolchain X not found" format, and
   `set -e` aborted the already-installed branch at `--version` before
   install_toolchain could run; `verus_ready_or_fix` now disambiguates
   loader errors (degrade) from missing-toolchain errors (install).
   Verified end to end on this 22.04 host. rosdep backend: consulted in
   `run_system` ONLY for entries with no mapping for the detected manager —
   never required, never touched on a mapped platform.
   Acceptance: `just verification setup` is green-or-informative on Ubuntu
   22.04; `nros setup --system` on a mapped platform never invokes rosdep.

## Sequencing

W1 → W2 → W3 land together as the core (each is scaffolding for the next);
W4/W5/W6 are independent behind them. W5 can land first if an unprovisioned
host needs unblocking — it has no dependency on the schema work.

## Net effect

One file answers "what does nano-ros need on this machine": setup installs
what it may, prints what it may not, and doctor's word is derived rather
than remembered. The 0368 class — a dependency discovered by a clean host
failing — becomes a missing index ENTRY, which is a one-line diff and a
lint, not an afternoon of forensic setup archaeology.
