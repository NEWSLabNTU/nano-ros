# RFC-0099 — Provisioning is planned once, prefers prebuilts, and ships what a build needs

**Status:** Draft (2026-09-11)

Implemented by [phase-447](../roadmap/phase-447-provisioning-revision.md).
Amends the provisioning model of [RFC-0014](0014-nros-setup-toolchain-management.md)
and the dependency SSoT of [RFC-0062](0062-unified-dependency-ssot.md); the
release/version axes it composes with are [RFC-0097](0097-release-composition-and-version-axes.md).

Motivated by six issues filed from provisioning a contained self-hosted runner
from an empty store (1264, 1266, 1267, 1273, 1274, 1275) — the first environment
in a long time to ask the provisioning system every question with no warm caches
and no packages a developer laptop happens to have.

## D1 — The released `nros` cannot build anything, and that is the first thing to fix

`nros build` resolves its board catalog from a nano-ros CHECKOUT, and says so:

```rust
// Resolving it needs the board catalog, which lives in a nano-ros checkout,
// NOT in the user's workspace.
None => eyre::bail!(
    "no nano-ros checkout found, so board ids cannot be resolved. \
     Pass --nano-ros-path, or set NROS_REPO_DIR."
),
```

The release asset stages `bin/nros`, `share/nros/nros-sdk-index.toml` and
`share/nros/install.sh` — no `cmake/`, no board descriptors, no runtime. So the
journey `install.sh` -> `nros setup <board>` -> `nros new` terminates at
`nros build`.

This breaks nobody TODAY, and that is the trap. RFC-0097 records that nano-ros
has zero releases, so everything here is greenfield — and the whole point of
that phase is to make the installed path real. The path dead-ends one step from
the end.

**Why it survived: the only people who could notice are the only people it
cannot affect.** A contributor has `NROS_REPO_DIR` exported by `activate.sh`, so
the second rung of the ladder always hits for them. And the clean-host probe
(issue 0204, `just probe bootstrap`) stops short — `installation.md` carries
`probe=10`/`probe=20` blocks for apt and the bootstrap, and `first-project.md`,
the scaffold -> build -> run page, carries **zero**. Nothing runs a user's first
build on a machine with no checkout.

## D2 — The SDK root ships INSIDE the toolchain, not beside it

RFC-0097 D3 lists four artifacts — launcher, CLI, codegen, prereq tools. None of
them is "the cmake modules, board descriptors and runtime a project compiles
against". That is the missing member.

It goes inside the toolchain artifact, at `share/nano-ros/{cmake,config,packages}`,
because RFC-0097 **D6** already decided that codegen and the runtime are ONE
unit. A separately-versioned `[source.nano-ros]` would split exactly the unit D6
says must not split, and reintroduce the acceptance-range question D6 closed.

Measured — and the first measurement was WRONG, which is worth recording because
it was wrong in the direction that looks finished. `cmake/` + `config/` + every
tracked file under `packages/` is 5.9 MB across 2869 files, and that is not the
payload: it is the three directories this decision first named. What a build
actually reads was established by BUILDING one with no checkout present
(phase-447 A1), and it is **30.5 MB across 2906 files**:

* `packages/cli` cannot be excluded whole. `packages/core/nros-macros` path-deps
  `packages/cli/{nros-pkg-index,nros-entry-lower}`, so cargo cannot LOAD the
  graph without them and every Rust, C and C++ user project fails before
  compiling anything. The carve-back set is DERIVED at staging time from every
  relative `path = "../../cli/<x>"` outside `packages/cli`, not listed by hand.
  The exclusion still earns its keep: 75.9 MB tracked stays out.
* The root `Cargo.lock` ships. The SDK root IS a cargo workspace — corrosion
  builds `nros-c`/`nros-cpp` from it — so without the lock a first build either
  re-resolves into the install prefix or fails under the project-wide `--locked`
  the `scripts/bin/cargo` shim injects.
* Plus `zephyr/`, `scripts/`, the root `CMakeLists.txt`, `nano_rosConfig.cmake`,
  `nros-sdk-index.toml`, and one NuttX `rust-toolchain.toml`.

(`du` on the working tree reports 20 G; that is build output. `git ls-files` is
the honest measure — the repo's own rule for enumerating the tree.)

**And the SDK root is not sufficient on its own.** `nros-launch-resolve` is a
SECOND artifact: every workspace configure goes through it (the bringup's launch
file becomes a SystemModel), and with `share/nano-ros` staged and no checkout,
`cmake` still dies at `nano_ros_entry` with "`nros-launch-resolve` not found.
Build it with ./scripts/bootstrap.sh" — a remedy naming a checkout the user does
not have. It cannot live IN the SDK root: it is its own cargo workspace with its
own lock and it embeds CPython through pyo3. The design already anticipated it —
`model_location::launch_resolver_bin`'s installed rung is
`$NROS_HOME/bin/nros-launch-resolve` — and had never had anything to find. The
release builds it, stages it beside `nros`, and `install.sh` fronts it
conditionally, because `front_newest` refuses an absent declared path and older
assets must stay installable.

It also carries an ABI floor `nros` alone does not: it links
`libpython3.10.so.1.0` as a hard `DT_NEEDED`. That is D5's problem, and D5 is
where it gets declared and probed.

## D3 — The store rung goes LAST on the resolution ladder

`cmd/build.rs` and the scaffolded `CMakeLists.txt` both resolve a nano-ros root
three ways, all of them a checkout: an explicit `-DNANO_ROS_ROOT` /
`--nano-ros-path`, then `NROS_REPO_DIR`, then a relative walk-up.

A fourth rung is added — **the running toolchain's own SDK root** — and it is
added LAST. Not a preference: the ownership guard (phase-431 W1) requires that
an `nros` running inside a checkout be that checkout's own build. A store rung
placed first would silently redirect every contributor to a store copy of a tree
they are editing. Last changes no existing behaviour and fills only the case
that today has none.

## D4 — Prefer a prebuilt, and make that a RULE rather than an intention

`plan_install` already prefers a dist:

```
Provenance::read(prefix) -> Present
tool.dist_for(host)      -> Prebuilt
tool.source              -> Source
```

So a source build never means "this must be compiled" — it means **the index has
no `dist.<host>` row**. Measured: **15 of 25 tools carry one; 10 do not** (`nros`,
`sccache`, `rustfilt`, `cargo-show-asm`, `play_launch_parser`, `corrosion`,
`esp32-qemu`, `espflash`, `genromfs`, `make`).

Each source build removed is a SERIALISATION POINT removed, not only minutes: a
source build takes the whole machine while the network sits idle, which is the
ceiling on any parallel install.

The rule is a ratchet, in the shape this repo already uses
(`.config/gate-selftest-baseline.txt`, the c-array-pool `UNCLASSIFIED_CEILING`):
every tool declares a dist or a recorded reason, and the list of exceptions may
only SHRINK. `make` and `ninja` stay `[tool.*]` — decided — so the survey covers
them rather than routing them to a second acquisition path.

Not `cargo-binstall`: it resolves at run time, where the index's contract is a
pinned `url` + `sha256` verified before unpack and recorded in `nros-sdk.lock`.
If it returns it belongs maintainer-side, minting dist rows offline.

## D5 — A dist is ABI-bound, so declare a floor and prove it RUNS

`host_key()` is `<os>-<arch>`:

```rust
format!("{}-{arch}", std::env::consts::OS)   // "linux-x86_64"
```

No OS version, no libc. `nano-ros-sdk` builds its dists on bare `ubuntu-22.04`
runners — glibc **2.35**, Python **3.10** — so a dist is offered to every Linux
x86_64 host including ones that cannot run it. `[prereq.libpython310]` exists
because a dist is bound to a Python minor version.

glibc is the FORWARD-compatible half and is largely handled already:
`nano-ros-sdk`'s `scripts/lib/bundle.sh` bundles each dist's ldd closure except
the loader/libc family and the compiler runtime. What is not handled is the
BACKWARD half — a named or interpreter-linked dep that does not exist on a
NEWER host (`libpython3.10` on 24.04, `libssl3` against the t64 renames).

Three moves, in order of how invisible they are:

1. **Build on the oldest supported base** (rustup's answer: a triple with no
   version in it, because the floor is under everyone). Policy in
   `nano-ros-sdk`, no schema. Best UX — nothing to see.
2. **Run `smoke` on the INSTALL path**, not only in `--check`. The machinery
   exists (`failing_smoke`, with `LD_LIBRARY_PATH`/`PYTHONPATH` stripped so it
   measures the dist and not the caller). It prevents nothing and converts a
   silent success into a loud failure where the cause is known. Its own doc
   says "Absent `smoke` means no opinion, not a pass", so it needs a
   smoke-or-reason ratchet or it covers only what already declares one.
3. **Declare a floor and probe before downloading**, falling back to source with
   a named reason.

REJECTED: widening `host_key` to `linux-x86_64-glibc2.35`. Compatibility here is
a RANGE, not an identity — a 2.39 host runs a 2.35 binary — so exact-match keys
are wrong, "highest key <= host" is a new resolution mechanism, and it still
misses the interpreter-linked half.

**Status: move 3 landed (2026-09-11, phase-447 D1).** Every `dist.<host>` row
carries `floor = { glibc, glibcxx, macos }` or `none = "<why>"`, and
`plan_install` compares the host with it IN THE PLAN — so a refused prebuilt's
URL never reaches `execute` — falling back to the source recipe with the reason,
or refusing outright when there is none. Both halves are covered: the floor
numbers are the forward half; the backward half is `system = [..]` read against
D9's per-release names, refusing only when a soname is ABSENT and the index
says the manager does not package it on this release. A probe that cannot
answer abstains. `check-dist-floors` is the floor-or-reason ratchet.

Measured with `scripts/sdk/measure-dist-floor.py` (ELF `verneed`/`NEEDED`/
`INTERP`, Mach-O `minos`, cross-checked against `objdump -T`), and the numbers
corrected three assumptions this section was written on:

* **No dist needs glibc 2.35.** The runner has 2.35; every `nano-ros-sdk` dist
  references at most `GLIBC_2.34`. What a runner HAS is not what a binary USES.
* **The C++ runtime is a real floor.** `riscv-none-elf-gcc` and `xrce-agent`
  need `GLIBCXX_3.4.30` (GCC 12, jammy) from a libstdc++ they do not bundle.
* **A dist can bring its own libc.** The Zephyr SDK host tools are a Yocto
  sysroot on their own `ld.so` + glibc; counting them against the host floor
  would have been wrong, so the measurement excludes self-hosted ELF and says
  how many it excluded.

Move 2 (smoke on the install path) is phase-447 C1.

## D6 — Repeated `nros setup` is made CHEAP rather than restructured away

A bootstrap makes ~24 `nros setup` invocations. The obvious fix — collapse them
into one call with variadic scopes and index-declared profiles — is the wrong
one, because it trades away the property that makes the dev workflow good:
`just <platform> setup` runs the command a USER would run for that platform.

Measured on a fully-provisioned host:

| invocation | time |
| --- | --- |
| `nros setup --tool ninja` (installed) | **0.01 s** |
| `nros setup --system` | 0.04 s |
| `nros setup native` | 0.06 s |
| `nros setup mps2-an385-baremetal` | **153.82 s** |
| the same, `--dry-run` (no git) | 0.01 s |

So the repeat cost is not re-installing. Three provisioning paths, and only one
lacks a presence check:

| path | check | skips |
| --- | --- | --- |
| `[tool.*]` | `Provenance::read(prefix)` | yes |
| `[source.*]` clone | dest populated -> `AlreadyPresent` | yes |
| `[source.*]` **submodule** | **none** | **no** |

The submodule arm runs `git submodule update --init [--recursive]`
unconditionally, and `recursive` defaults true. `git submodule status
--recursive -- <path>` answers it in **0.43 s** for both of that board's
sources, and its `-` / `+` / `U` prefixes are the same signal CLAUDE.md already
documents for catching pin rewinds. `SourceDisposition::AlreadyPresent` exists
and already prints `"already present (skip)"`.

**153.82 s -> ~0.5 s**, and the collective recipe becomes cheap because its parts
are, with no aggregation and no call-site migration.

This is not only a bootstrap tax: every `just <platform> setup` for a board with
submodule sources has been paying it. `native` is 0.06 s only because it has none.

REJECTED: a session cache with a TTL. It preserves every call site and gets the
single apt ask — and makes the same command behave differently depending on WHEN
it ran, which is the drift RFC-0062 deleted the rosdep resolver to avoid.

## D7 — Resolve the session's plan once; execute it pipelined, bounded by the host

What repeated-invocation cheapness does NOT fix is the COLD run, and there the
serialisation is real and threefold: fetch -> verify -> unpack is strictly serial
within a package (1266); the package loop is serial (1267); and the system-package
ask is composed per invocation, so one bootstrap printed three overlapping
`apt install` lines with different subsets and acted on none (1274).

Resolution moves to SESSION scope — one plan, before any fetch — which is the
same pre-pass a worker pool needs, so it is not a detour. Execution is a pipeline
over that plan, **bounded by the host's CPU count**.

Four things stay ORDERED regardless of concurrency, and each is a decision rather
than a detail: the lock file's single writer; `front_newest`, which implements
issue 0500's newest-version-first rule where a stale entry shadowing a fresh one
prints success on BOTH paths; `bin_dirs`, folded onto the emitted CMakePreset's
`PATH` in plan order; and per-package output, buffered and flushed in plan order.

Expect less than the package count: at the measured ~0.8 MB/s the link saturates
long before the cores do, and two concurrent source builds mostly move contention
around. The win is overlapping a BUILD with another package's DOWNLOAD — which is
also why D4 comes first.

`--tool` becomes repeatable. `--source` already is, and that asymmetry is the
whole reason `ninja` and `make` are two processes in `workspace.just`.

**How "one ask per session" is reached with D6's several processes**
(phase-447 E2). Within a process the plan asks once, for the union. Across
processes the ask is DEDUPLICATED rather than merged: the session's driver —
`just setup`, or `runner-provision.sh` around it — opens a ledger named by
`NROS_SETUP_SESSION`, and each ask records its keys there, so a key asked for
once is not asked again while a key nobody has asked for still is. This is not
the rejected cache: it has no TTL and no persistence past the driver, it never
hides a need (only a repeat of one), and with the variable unset every command
answers exactly as it did. `--sudo` never reads it.

A missing key whose provider chain ends in the store is not an OS ask at all:
it is offered as `nros setup --tool …`, and omitted when the plan installs that
tool itself. One bootstrap's first ask carried `make ninja-build` for exactly
that reason — keys the session then provisioned from the store, and whose apt
packages are below the version floor on Ubuntu 22.04 anyway.

## D8 — rosdep: adopt neither the syntax nor the resolver; vendor a PINNED snapshot

RFC-0062's amendment rejected rosdep as a resolver for three reasons — it answers
for one provider of four, it cannot carry a `check`, and a resolver present on
one host and not another makes one tree resolve two ways — while explicitly
KEEPING its key names.

Adopting the file format adds nothing. `[prereq.*]` already IS rosdep's model:
the same key namespace and the same per-manager package lists, plus `check`,
`why`, `role` and `provider`, minus OS-version granularity. And the moment those
four extensions are written into rosdep YAML, no rosdep tool can read it — the
interop prize evaporates on contact with the extension, leaving a second config
language in a TOML tree.

The instinct behind the request is right and the numbers are stronger than they
look. Of 46 `[prereq.*]` keys: **infra 19, workspace 11, buildtool 7, vendor 5,
package 4**. Only **4 of 46** are the per-package kind rosdep is designed for;
the rest are exactly the toolchain and build prerequisites it has no vocabulary
for.

The real gap is the DATABASE. A user writing `<depend>libopencv-dev</depend>`
hits the ladder's last rung — UNKNOWN, an error — where a ROS user gets it from
~10k community keys. So: **vendor a pinned rosdep snapshot as index data**, a
fallback rung for `provider = "system"` only, with keys from it marked unprobed.
Every one of the original objections was about a host-installed resolver; a
pinned snapshot is none of those things, and it keeps the provisioner reading
only manifest/index data.

## D9 — The manager fields gain an OS-version dimension

`apt` / `dnf` / `pacman` / `brew` were flat lists — manager-keyed, with no OS
dimension. The tree already had a casualty:

```toml
[prereq.libpython310]
why = "runtime dep of the play_launch_parser dist(s)"
apt = ["libpython3.10"]
```

Ubuntu 22.04 ships Python 3.10; 24.04 ships 3.12 and has no such package, so
the key could not express the alternative.

**The first spelling of this decision was unwritable.** It said "`apt = [...]`
stays valid; `apt.noble = [...]` overrides", and TOML cannot hold a key as both
an array and a table: `apt = [..]` beside `apt.noble` is a parse error. Found
while implementing it (phase-447 D2, #931). What landed instead:

```toml
apt = ["pkg"]                                   # every release, unchanged
apt = { default = ["pkg"], noble = ["pkg-t64"] } # per-release override
```

An explicit `noble = []` means "not packaged on that release", which is exactly
what D5's backward half reads to refuse a dist before downloading it. Release
keys come from `/etc/os-release` and are validated. Python reads these fields
only through `scripts/lib/index_packages.py`, because `entry.get("apt")` on a
table would iterate release NAMES as if they were packages. `libssl3` gained
`noble = ["libssl3t64"]`.

**And `libpython310` turned out to be two requirements under one key** — the
rebase of D2 onto C2 exposed it, when each had traced it to a different
consumer:

* `libpython310` is the exact-library dependency of the released
  `nros-launch-resolve` binary (a hard `DT_NEEDED` on `libpython3.10.so.1.0`,
  shipped since phase-447 A1), so the minor version in its NAME is correct. It
  is `jammy` only, with `noble = []`.
* `libpython3` is the host's own Python for the `play_launch_parser` SOURCE
  build, which links whatever minor is installed: jammy `libpython3.10`, noble
  `libpython3.12t64`, default `libpython3-dev`.

Narrower than rosdep's OS-then-version nesting, still, because the manager is
what actually installs.

**Status: landed (2026-09-11, phase-447 D2) — with the spelling corrected.**
The sketch above is unwritable exactly when it is needed: TOML cannot hold
`apt = [..]` beside `apt.noble = [..]`, since a key is an array or a table,
never both. So the override table carries its own default:

```toml
apt = ["libssl3"]                                         # every release, unchanged
apt = { default = ["libssl3"], noble = ["libssl3t64"] }   # one release renamed it
```

Release keys are the host's `VERSION_CODENAME`, else `VERSION_ID`
(`host_os_release`); `validate` refuses one that cannot be a release name.
An explicitly EMPTY list — `noble = []` — means "not packaged on that
release", which is a different answer from "not named", and D5's floor probe
reads exactly that. Python readers go through `scripts/lib/index_packages.py`,
because `entry.get("apt")` on the table shape iterates RELEASE NAMES.

**`libpython310` was misnamed, not merely stale.** Its only consumer is the
`play_launch_parser` SOURCE build, and a source build links the host's own
CPython — so the requirement is "this host's libpython3", and a key naming
3.10 asked every non-jammy host for something it could not have. It is now
`[prereq.libpython3]` (`default = libpython3-dev`, `jammy = libpython3.10`,
`noble = libpython3.12t64`, probe `sharedlib = "libpython3."`). A soname-EXACT
requirement belongs to a prebuilt that links one minor; it gets its own key the
day such a dist row exists, and `check-dist-runtime-deps` will demand it.

## D10 — The provisioner reads manifest/index files — TRUE for Zephyr since phase-447 F1

**Status: satisfied for the Zephyr module set (2026-09-11, phase-447 F1, issue
1275).** `[zephyr_module.*]` is the SSoT and the two west manifests are the
derived half — a module is in `west.yml`'s allowlist iff its `lines` carries
`"3.7"`, `west-4.4.yml`'s iff `"4.4"`, asserted both ways by
`check-zephyr-module-allowlist`; `nros setup zephyr --dry-run` prices the set.
The manifests stay COMMITTED rather than generated, because `west init -m <url>`
reads `west.yml` out of a bare clone before any `nros` exists to generate one —
the trade `check-abi-bindings` already makes for committed bindgen output.

Three of the four HALs left the allowlist (~2.29 GB off a fresh `west update`).
`hal_espressif` stayed: measured to have NO consumer, and kept anyway, because
deleting it settles the platform-strategy question below by accident — that is
issue 1282. **`west update` does not prune, so this is a fresh-workspace saving
and reclaims nothing in an existing one.**

The paragraph below is the original statement of the defect, kept because it is
what the decision argued from.

The claim was not true. The Zephyr module set lives in `west.yml`, and
`nros-sdk-index.toml` contains **zero** `hal_` mentions — so `west update` pulls
`hal_nxp` (1.3 G), `hal_stm32` (764 M), `hal_espressif` (275 M) and `hal_nordic`
(224 M) for silicon no board in `fixtures.toml` targets, and `nros setup
--dry-run` cannot price any of it because the cost lives in a file the
provisioner never opens.

Bringing the module set under the index makes board -> module the same mechanism
as board -> package. Do not simply delete the four: a Zephyr `prj.conf` or board
fragment can pull a module with no fixture naming the board, and issue 0876 was
exactly that shape — a conf whose only live effect was somewhere nobody was
looking. Whether Zephyr's own espressif support makes our ESP-IDF path a
duplicate is a platform-strategy question to MEASURE, not to settle while
deleting a manifest line.

## Non-goals

* `nros run` / `nros flash` — RFC-0097 D8, unchanged.
* An acceptance range for `NROS_CODEGEN_VERSION` — RFC-0097 D6, unchanged.
* rosdep's YAML syntax, and rosdep as a runtime resolver — D8.
* A wider `host_key` — D5.
* A session cache with a TTL — D6.
