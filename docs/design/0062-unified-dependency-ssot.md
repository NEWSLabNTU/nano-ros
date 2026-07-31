# RFC-0062 — One dependency SSoT, system-aware

**Status:** Draft (2026-08-01)
**Amends:** RFC-0014 (`nros setup` toolchain management) — extends its index
from two dependency classes to all of them; changes no existing `[tool.*]` /
`[source.*]` semantics.
**Motivated by:** issue 0368 — a simulated end-user `just setup all` on a clean
Ubuntu 22.04 host failed 7 of 18 modules, nearly all on dependencies that were
declared nowhere (or declared as a Debian-only sudo list ordered in front of
the sudo-less installers it then aborted).

## Problem

nano-ros's dependencies live in five places with five owners:

| Class | Where declared today | System-aware? | Doctor-checked? |
| --- | --- | --- | --- |
| Pinned prebuilt dists (qemu-nros2, cyclonedds, …) | `nros-sdk-index.toml [tool.*]` | yes (per-host dists) | partially |
| Source/submodule deps (freertos, nuttx, …) | `[source.*]` | yes | mostly |
| OS packages | `just/workspace.just apt-packages` (Debian only) + ad-hoc probes inside module scripts (zephyr wants ninja/aria2c, esp_idf assumes python3-venv, rmw_zenoh prints a sudo apt line) | **no** | drifts |
| Rust layer (toolchains, targets, cargo tools) | `rust-pinned-toolchains` / `rust-targets` / `cargo-tools` recipe bodies | n/a | via recipes |
| Python layer (west, colcon, clang-format wheel, px4 requirements) | scattered pip calls per module | no | no |

The consequences measured in 0368:

- One sudo step (`apt-packages`) ordered first aborted the workspace module's
  own **sudo-less** installers, cascading into three other modules' failures.
- Doctor remedies drift from reality because they are hand-written per module:
  three pointed at apt/sudo where an index prebuilt already existed
  (riscv gcc, idlc-in-cyclonedds, play_launch_parser).
- Whole dependencies were simply undeclared until a clean host hit them
  (`python3-dev`, `libz3-dev`+`libclang-dev`, `libslirp0`, `aria2`,
  `parallel`, `cargo-nextest`).
- A prebuilt dist (`qemu-11.0.0-nros2`) has a RUNTIME system dep (libslirp)
  that no layer could express, so `nros setup` installed a binary that cannot
  execute and only its smoke check caught it.

## Decision

**`nros-sdk-index.toml` becomes the single declaration for every dependency
class.** Setup *and* doctor both derive from it — the remedy a doctor prints
is computed from the entry, never hand-written, which deletes the
remedy-drift class the same way RFC-0061's shared coordinate file deleted
lane-coverage drift.

### The classes

Existing, unchanged: `[tool.*]` (+ `.source`), `[source.*]`, `[gated.*]`.

New:

```toml
# -- OS packages, declared by ABSTRACT key, mapped per package manager. ------
[system.libslirp]
why  = "runtime dep of the qemu-nros dist"
apt    = ["libslirp0"]
dnf    = ["libslirp"]
pacman = ["libslirp"]
brew   = ["libslirp"]
check  = { sharedlib = "libslirp.so.0" }        # ldconfig / dlopen probe

[system.gnu-parallel]
why  = "just format fan-out"
apt = ["parallel"]; dnf = ["parallel"]; pacman = ["parallel"]; brew = ["parallel"]
check = { cmd = "parallel" }

# -- Rust layer. -------------------------------------------------------------
[rust.toolchain.nightly-pinned]
channel = "nightly-2026-04-11"
components = ["rustfmt", "clippy", "rust-src", "miri", "llvm-tools"]

[rust.target.riscv32imc]
triple = "riscv32imc-unknown-none-elf"
toolchain = "nightly-pinned"

[rust.cargo-tool.nextest]
crate = "cargo-nextest"; version = "0.9"; locked = true
check = { cmd = "cargo-nextest" }

# -- Python layer (versioned, into one managed venv or --user). --------------
[python.west]
pip = "west"; version = "1.2"
check = { cmd = "west" }
```

And one addition to the existing class: a `[tool.*]` may declare
`system = ["libslirp"]` — the runtime OS deps of its **dist** — so the qemu
failure mode becomes representable, checkable, and part of the printed plan.

### Consumers declare needs; nothing re-declares content

Boards and modules already reference `[tool.*]` sets (`board.packages`).
The same mechanism extends: a module's setup/doctor asks the index for its
`needs = [...]` closure. Module scripts stop carrying their own prereq
probes (the zephyr ninja/aria2c check, the rmw_zenoh apt hint, the esp_idf
venv assumption all move into entries).

### System-aware resolution

`nros setup` detects the package manager (apt/dnf/pacman/brew; `os-release`
plus `command -v` fallback) and resolves `[system.*]` keys through the
in-index mapping. Where a key carries no mapping for the detected manager,
an optional **rosdep backend** fills it: the public rosdistro database
already resolves about half of nano-ros's current keys (measured 12/24), it
runs entirely sudo-less (`pip install --user rosdep`, `ROSDEP_SOURCE_PATH`
pointed at a repo-local sources list — no `rosdep init`), and an in-repo
overlay yaml covers the rest (validated end-to-end; transcript in issue
0368 / the 2026-08-01 session). rosdep is an *optional resolver*, not the
SSoT: keys and their primary mappings live in the index, so a host without
rosdep or network loses nothing on the mapped platforms.

### The sudo boundary

Resolution partitions the plan into:

- **unprivileged** — store dists, source builds, rustup, cargo installs,
  pip (venv/`--user`): `nros setup` executes these itself, ordered FIRST;
- **privileged** — the `[system.*]` closure: composed into **one native
  command per manager** and *printed* (or executed only under an explicit
  `--sudo` opt-in). A missing system package therefore degrades a module's
  setup to "here is the one command to run", never aborts the sudo-less
  remainder — the direct fix for 0368-F1.

### Doctor = the same walk, read-only

`nros doctor` runs each entry's `check` probe and prints the entry-derived
remedy (`nros setup --tool X` / the composed native command / `rustup
target add …`). Hand-written remedy strings in module doctors are replaced
by index lookups; a probe with no index entry becomes a lint (the
issue-0196 rule applied to dependencies: the gate must cover the class).

## What this deliberately does not do

- No new lockfile semantics: `nros-sdk.lock` keeps recording store installs;
  system packages are observed by probe, not pinned (distros own their
  versions).
- No containerization answer: images/devcontainers can be *generated from*
  the index later, but are out of scope here.
- No change to `[gated.*]` (licensed SDKs stay instruction-only).

## Migration (phase-327 — `docs/roadmap/phase-327-unified-dependency-ssot.md`)

1. Add the three new classes to the index; move `apt-packages`' list, the
   module-local probes, and every 0368 discovery into entries. `apt-packages`
   becomes a thin printer over the resolver (fixes F1 ordering as a side
   effect).
2. Generic setup/doctor walkers over the index; delete per-module remedy
   strings as each module's entries land.
3. `[tool.qemu] system = ["libslirp"]` + re-cut of the dist (or rpath-bundle
   libslirp; either way the dep is now declared and checked).
4. Optional rosdep backend + overlay for unmapped managers.

## Evidence

- Issue 0368 — the clean-host walk this RFC answers, including the measured
  cascade and the full list of undeclared deps.
- rosdep feasibility test (2026-08-01): user-level install, no-sudo sources
  via `ROSDEP_SOURCE_PATH`, 12/24 public-db coverage, 9/9 overlay coverage,
  machine-readable `#apt` output — preserved in the session transcript and
  `tmp/rosdep-overlay-test/nano-ros.yaml`.
