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

Measured, so the cost is not a guess: `cmake/` + `config/` + every tracked file
under `packages/` is **5.9 MB across 2869 files**. (`du` on the working tree
reports 20 G; that is build output, and `git ls-files` is the honest measure —
the repo's own rule for enumerating the tree.)

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

`apt` / `dnf` / `pacman` / `brew` are flat lists — manager-keyed, with no OS
dimension. The tree already has a casualty:

```toml
[prereq.libpython310]
why = "runtime dep of the play_launch_parser dist(s)"
apt = ["libpython3.10"]
```

Ubuntu 22.04 ships Python 3.10; 24.04 ships 3.12 and has no such package. The
key cannot express the alternative. (It is also stale in a second way:
`play_launch_parser` has no dist at all — it is one of D4's ten.)

`apt = [...]` stays valid and means "every version"; `apt.noble = [...]`
overrides. Narrower than rosdep's OS-then-version nesting, because the manager is
what actually installs.

## D10 — The provisioner reads manifest/index files, and today it does not

The claim is not true yet. The Zephyr module set lives in `west.yml`, and
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
