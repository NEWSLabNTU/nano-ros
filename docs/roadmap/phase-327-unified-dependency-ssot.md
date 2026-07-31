# phase-327 — unified dependency SSoT (implement RFC-0062)

**Implements:** [RFC-0062](../design/0062-unified-dependency-ssot.md)
**Resolves:** [issue 0368](../issues/0368-setup-all-on-a-clean-host-seven-modules-fail-on-undeclared-prereqs.md)
**Status:** In progress (W5 2026-08-01; W1+W2+W4 and the workspace half of W3 2026-08-01)
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

3. **W3 — doctor derives from the index.** — **PARTIAL** (2026-08-01: the
   workspace doctor's system block derives from `--system --check`; the
   cyclonedds idlc + play_launch_parser remedies re-pointed at index tools.
   REMAINING: the generic walker for `[rust.*]`/`[python.*]`, the other
   module doctors, and the no-index-entry lint). A generic walker runs each
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

6. **W6 — verus pin + optional rosdep backend.** — **verus pin PARTIAL** (2026-08-01:
   `scripts/setup-verus.sh` now pins the release (`VERUS_VERSION`, default
   `release/0.2026.07.27.31579f0`, `=latest` to opt out) instead of fetching
   `releases/latest`, and probes `verus --version` before use — on a host whose
   glibc cannot run the binary it prints an informative `[verus] …` note and
   exits 0 (verification is in no CI gate) instead of a raw loader crash. REMAINING:
   the optional rosdep backend.) Pin the verus release to
   one that runs on the oldest supported LTS glibc (or degrade with a
   message — verification is in no CI tier gate). Then the rosdep backend:
   for a detected manager with no index mapping, consult rosdep (public db +
   the in-repo overlay validated in the 0368 session) — never required,
   never networked unless invoked.
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
