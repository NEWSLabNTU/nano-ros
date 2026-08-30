---
rfc: 0062
title: "One dependency SSoT, system-aware"
status: Stable
since: 2026-08
last-reviewed: 2026-08
implements-tracked-by: [phase-327, phase-398, phase-404]
supersedes: []
---

# RFC-0062 — One dependency SSoT, system-aware

**Status:** Stable (2026-08-30; Draft 2026-08-01, amended 2026-08-29)

Settled: `[prereq.*]` is the one declaration namespace, phase-398 landed it
end to end (`[system.*]` retired through alias -> warn -> gate -> delete, the
rosdep fallback deleted), and issue 0926 closed the last consumer gap — a
dist's runtime libraries are now measured rather than hand-listed, reported on
the path where the tool is USED, and gated by `check-dist-runtime-deps`.

One item is deliberately outside this RFC and tracked separately: re-cutting
dists with `$ORIGIN` rpath so a declared dependency disappears instead
(issue 0928). That is a nano-ros-sdk change, not a decision this RFC owes.
**Amends:** RFC-0014 (`nros setup` toolchain management) — extends its index
from two dependency classes to all of them; changes no existing `[tool.*]` /
`[source.*]` semantics.
**Amended:** 2026-08-30 (amendment 2) — the provider is chosen by what the
tool DOES, `check` gains a version constraint, and every resolution reports
its provider; tracked by phase-404, no code yet.
**Amended:** 2026-08-29 — `[prereq.*]` (one key namespace over four providers),
unknown keys are an error, rosdep is no longer consulted, and `[system.*]`
MERGES into `[prereq.*]` and retires. See the amendment below; it REVERSES
§"System-aware resolution".
**Motivated by:** issue 0368 — a simulated end-user `just setup all` on a clean
Ubuntu 22.04 host failed 7 of 18 modules, nearly all on dependencies that were
declared nowhere (or declared as a Debian-only sudo list ordered in front of
the sudo-less installers it then aborted).

## Amendment (2026-08-29) — `[prereq.*]`: one key namespace, four providers, no rosdep

Motivated by a second instance of the class this RFC was written for. An agent
hit `libslirp.so.0` missing on the store's QEMU — a dependency **this index
already declares** (`[tool.qemu] system = ["libslirp"]`, probed by
`[system.libslirp] check.sharedlib`), with a comment saying it was declared
precisely so setup and doctor could say so "BEFORE the smoke check fails with a
bare loader error". It could not: nothing consulted the declaration on the path
where the tool is USED, so the loader spoke first. (Fixed separately — the store
resolver now probes.)

That is a consumer gap, not a schema gap. But it exposed the schema gap behind
it: **a user cannot declare a prerequisite at all.** `package.xml` `<depend>`
feeds build ORDER only, and a name that is not a workspace package is silently
ignored by construction. Every prereq in this tree is declared by the index, for
the index's own tools; nothing carries a user's.

### What changes

**1. `[prereq.<key>]` replaces `[system.<key>]` as the user-facing namespace,
and spans all four providers.** The providers already exist as separate classes
— `[system.*]` (OS package), `[tool.*].dist` (download), `[tool.*].source` +
`install` (build from source), `[source.*]` (submodule). What did not exist is
one name a consumer can write without knowing which of the four answers it.

```toml
[prereq.libslirp]                  # provider = "system" is the default
why      = "qemu -netdev user"
apt      = ["libslirp0"]
dnf      = ["libslirp"]
pacman   = ["libslirp"]
brew     = ["libslirp"]
check    = { sharedlib = "libslirp.so.0" }

[prereq.qemu]
provider = "sdk"                   # resolves through the existing [tool.qemu]
[prereq.freertos-kernel]
provider = "source"                # resolves through the existing [source.*]
```

`provider` defaults to `system`, so all 25 existing `[system.*]` entries are
valid `[prereq.*]` entries unchanged — this is a rename plus a default, not a
migration. `check` and `why` are kept and are the point: they are what makes a
missing prereq *diagnosable* rather than a loader error.

**2. Resolution is a ladder, and an unknown key is an ERROR.** For each
`<depend>` in a consumer's `package.xml`:

| rung | outcome |
| --- | --- |
| a workspace package | build ORDER, not a prereq — today's behaviour |
| a generated message package | `nros sync` owns it |
| a `[prereq.*]` key | its provider installs it; `check` decides present/absent |
| anything else | **error, naming the key** |

The message-package rung is not decoration. `std_msgs` is a legitimate key in
other ecosystems and a *generated* crate here; without an explicit rung, sync
and the prereq resolver both claim it and the winner is whichever ran last.

The last rung is the behaviour change, and it is the whole point. Today an
unrecognised `<depend>` is dropped in silence — the same silence that let a
declared dependency reach the dynamic loader. `NROS_ALLOW_UNRESOLVED_DEPS=1`
opts out for a tree mid-migration; it is an escape hatch, not a mode.

**3. rosdep is NOT consulted. This REVERSES §"System-aware resolution" above.**

That section makes rosdep an optional resolver for managers the index does not
map, and reports 12 of 24 keys resolvable from the public database. The
reversal is deliberate and the reasoning is not "rosdep is bad":

* **It answers for one provider of four.** rosdep has no concept of an SDK dist,
  a submodule, or a source build, so it can never be the resolver — only a
  partial one, which means every consumer needs the fallback logic anyway.
* **It cannot carry a `check`.** The probe is what turns "missing package" into
  a named remedy, and it is the half that would have prevented this
  amendment's motivating failure. A resolver that supplies packages but not
  probes leaves the diagnosable part to us regardless.
* **A resolver consulted only sometimes is a resolver whose behaviour depends on
  the host.** "rosdep is installed here and not there" makes the same tree
  resolve differently on two machines, which is the drift this RFC exists to
  delete.

**Keep the key NAMES rosdep uses** where one exists (`libslirp`, not `slirp`).
That is free, makes porting an existing rosdep list mechanical, and costs no
runtime dependency. Compatibility with the *database* was the only real prize,
and it is not worth a host-dependent resolver to reach half of it.

Consequence: the `rosdep_resolve` fallback in `cmd/setup.rs` (phase-327 W6)
becomes dead and should be deleted with this work, not left as an unreachable
branch.

### `[system.*]` MERGES into `[prereq.*]`, and retires

Settled by what the index already does, not by preference.

**A flat cross-provider namespace already ships.** `board.packages` is one list
per board, and its names resolve across FOUR classes today:

| name | resolves in |
| --- | --- |
| `qemu`, `arm-none-eabi-gcc`, `espflash`, `riscv-none-elf-gcc` | `[tool.*]` |
| `freertos-kernel`, `lwip`, `nuttx-{libc,kernel,apps}`, `threadx*` | `[source.*]` |
| `genromfs` | `[tool.*]` **and** `[system.*]` |
| `arm-fvp` | `[gated.*]` |

So `[prereq.*]` does not invent a flat namespace. It names the one boards have
been using, and gives it a declaration table.

**One key already needs several providers, and says so in prose.** `genromfs`
exists in two tables deliberately — `[system.genromfs].why` reads "the
`[tool.genromfs]` source recipe is the store alternative". That is *one
prerequisite, ordered providers* expressed as key duplication plus a comment
tying the halves together. Nothing enforces that the two stay in agreement, and
nothing tells a resolver which to prefer.

**`[system.*]` carries no field `[prereq.*]` would lack** — `why`, the four
manager maps, `check`. It is precisely the `provider = "system"` case.

Therefore: **one table.** `[prereq.<key>]` with an ordered `providers` list
replaces the duplication:

```toml
[prereq.genromfs]
why = "NuttX riscv rv-virt etc/ ROMFS image"
providers = ["system", "source"]   # ordered: prefer the OS package, build if absent
apt = ["genromfs"]; pacman = ["genromfs"]
source = "genromfs"                # the existing [tool.genromfs] source recipe
check = { cmd = "genromfs" }
```

#### Retirement, with the mistake this phase already made

`[system.*]` is parsed as an alias lowering to `provider = "system"`, warns, and
is deleted at the next minor version. That is W1.f's pattern — and W1.f is
exactly why the steps below are explicit, because it shipped a correct,
well-tested deprecation lint that **no production path ever called**, so the
warning reached nobody and the removal would have landed on users who were never
told.

1. `[prereq.*]` lands; `[system.*]` parses as an alias. No behaviour change.
2. The deprecation warning is **wired at index load and a test asserts it is
   reached** — not merely that the lint is correct in isolation. A lint proven
   only by direct unit-test calls is proven against the one caller that is not
   the problem.
3. A gate rejects a key declared in two provider tables. That makes the
   `genromfs` shape illegal once its merged entry exists, so the duplication
   cannot silently return.
4. `[system.*]` is deleted at the next minor — and only after the warning has
   actually shipped in a release, not merely been written.

### The `check` vocabulary

**Today only `[system.*]` has probes: 22 of 25 entries. `[tool.*]` has none of
14, `[source.*]` none of 15.** Presence for those two is implicit — the store
path or the checkout directory exists — which is the state the motivating
failure exploited: the QEMU dist was present by that test and unusable.

The design follows from `genromfs` again. Its probe is `check = { cmd =
"genromfs" }`, and that is correct **whichever provider installed it** — apt or
the source recipe. So:

> **`check` answers "is the capability usable?", never "did provider X install
> it?"** It stays one provider-independent vocabulary. Providers contribute
> INSTALLATION knowledge, not DETECTION knowledge.

That splits cleanly into two questions that were being conflated:

* **`check` — is it usable now?** Provider-independent, OR-ed, tri-state
  (`Present` / `Missing` / `Unknown`). Extends to every provider unchanged.
* **provider verification — is what we installed still what we declared?**
  Store path + version, submodule rev, dist sha256. Already exists in
  `[tool.*]`/`[source.*]`; stays there. It is a different question and it
  belongs to whoever did the installing.

`Unknown` is load-bearing and any new kind must be able to return it: a probe
that cannot answer on this host must not vote (issue 0487 — libgcrypt ships
`.pc` on Arch and `libgcrypt-config` on Ubuntu, so either probe alone is a false
negative on one of them, and a false negative here prints a sudo command for a
package that is already installed).

Two kinds the current four cannot express:

```toml
# RUNS — the resolved binary executes. Not `cmd`: that is `command_exists`, a
# PATH lookup, and a store dist is not on PATH. This is the probe the motivating
# failure needed — QEMU's path existed, and the dynamic loader was the first
# thing to disagree.
#   Unknown when the tool targets a foreign platform (cross toolchains,
#   emulator-less hosts): "cannot execute here" is not "absent".
check = { runs = "qemu-system-arm --version" }

# PATH — a file that must exist inside a checkout. For `source`/submodule
# providers, which have no PATH entry and no soname to probe, so today their
# presence test is "the directory exists" — true of an empty uninitialised
# submodule.
#   Relative to the provider's own `dest`, so the probe does not restate a
#   location the provider already declares.
check = { path = "include/FreeRTOS.h" }
```

Both compose with the existing OR: `{ cmd = "genromfs", runs = "genromfs -h" }`
is present if either answers, which is the libgcrypt rule applied to a stronger
probe.

### What this amendment still does not decide

* ~~**Whether `providers` is ordered preference or a fallback chain with a
  policy**~~ — **DECIDED by amendment 2 below (2026-08-30): ordered
  preference, and the ORDER is derived from what the tool does rather than
  authored per entry.** The `--offline` tension is resolved there too.
* **Whether `check` becomes REQUIRED.** Three `[system.*]` entries have none
  today (`ros-rmw-zenoh-cpp`, `python3-venv`, `picolibc-riscv64-unknown-elf`)
  and report `UNPROBED`; requiring one would force an answer for each, which may
  not exist.

## Amendment 2 (2026-08-30) — the provider is chosen by what the tool DOES

Motivated by a question this RFC's own framing invited: if a tool is available
from the system, what is the point of shipping a copy? The first answer — "we
ship it when the binary is patched" — is close, and measurement shows it
explains the wrong thing.

**Only three tools carry a patch**: `qemu` and `cyclonedds` (NEWSLabNTU forks)
and `play_launch` (ours). By a patched/unpatched rule the other eleven become
candidates for deletion. They should not all move, and the reasons differ per
tool, which is the tell that the criterion is wrong.

### The axis is not "patched", it is two questions

**1. Does its output enter a build artifact?** Then PIN it, whatever the system
has. A compiler, Corrosion, the codegen tool, `idlc` — these decide what the
produced binary IS, and a host-varying build input is this tree's most expensive
bug class: museum binaries, the accumulating store shadowing a pin (issue 0500),
the loader picking a library nobody chose (0774). Corrosion is the concrete
case: `< 0.6.0` shares one `cargo/build` across workspace roots, so `mixed`
fails to link with duplicate `#[no_mangle]` symbols — "whatever the host has" is
a landmine, not a convenience.

**2. Must it match something the host already runs?** Then the system copy is
not merely ALLOWED, it is MANDATORY, and shipping our own is a defect. This is
not new: RFC-0075 already deleted our vendored `zenohd` and resolves ROS's
`rmw_zenohd` instead, because a pinned copy drifted from the zenoh that
`rmw_zenoh_cpp` links (issue 0609 measured 0.1.1 -> 0.1.9 moving zenoh 1.2.0 ->
1.8.0 with our pin taking no part). The same binding holds cyclonedds to the
version ROS ships (0507). Amendment 2 generalises RFC-0075 rather than inventing
a policy.

Everything that is NEITHER — a debugger, a flasher, an on-chip debug server — is
the real "prefer the system if it is good enough" population.

### Why "good enough" is currently undecidable

`check` is presence-only: `cmd`, `sharedlib`, `pkg_config`, `header`. **There is
no version constraint anywhere in the index.** So "use the system copy when it
satisfies the pin" cannot be expressed today, and that — not policy — is what
blocks the whole idea.

Measured on Ubuntu 22.04, which is why the constraint must be real rather than
assumed:

| tool | pin | apt candidate |
| --- | --- | --- |
| `sccache`, `corrosion`, `espflash`, `xrce-agent` | — | **not packaged at all** |
| `arm-none-eabi-gcc` | 13.2 | 10.3 (2021) |
| `riscv-none-elf-gcc` | 14.2 | 10.2, and a DIFFERENT triple (`riscv64-unknown-elf`) |
| `openocd` | 0.12.0 | 0.11.0 |
| `genromfs` | 0.5.7 | 0.5.2 |
| `qemu` | 11.0.0 (fork) | 9.0.2 |
| `cyclonedds` | 0.10.5 (fork) | 0.8.2 |

Four of ten are not packaged; most of the rest are years behind. So on this host
"prefer system" would rarely fire — and on Arch, Fedora or nixpkgs the table
looks very different. That asymmetry is the argument FOR the mechanism (it is
what makes a non-Ubuntu host first-class, cf.
`docs/development/ros2-on-non-ubuntu.md`) and AGAINST deleting dists on the
strength of one distro's archive.

### What this amendment decides

1. **`providers` is ORDERED PREFERENCE**, closing amendment 1's open question.
   The order is not authored per entry by taste; it follows from the two
   questions above, and an entry that departs from its category says why.
2. **`check` gains a version constraint** — a floor plus a declared way to read
   the installed version. Without it, preference is unimplementable.
3. **Every resolution REPORTS its provider.** `nros setup --check` and
   `just doctor` must say which provider satisfied each tool, and at what
   version. A host that quietly used its own `openocd` 0.11 otherwise makes
   "works on my machine" unfalsifiable — the same reason issue 0929's `smoke`
   probes assert on OUTPUT rather than exit status.
4. **`--offline` and air-gapped hosts pin the order to the store.** Amendment 1
   left this tangled with RFC-0065 D14; it is not tangled. Preference is a
   DEFAULT, and `--offline` is an explicit override that removes `system` from
   consideration — the same shape as every other escape hatch here.

### What it does NOT decide, deliberately

* **Which dists actually stop shipping.** That is a per-tool call against the
  measured table once the constraint exists, not a category sweep. Nothing is
  deleted by this amendment.
* **Whether a system-provided BUILD input is ever acceptable.** Question 1 says
  no today. If that is ever relaxed it needs its own evidence, because the whole
  reproducibility argument rests on it.
* **How a version is extracted per tool.** `--version` output is not uniform and
  some tools need a regex; whether that lives in the index or in a small table
  of known shapes is an implementation question for phase-404.

### Cost of being wrong in each direction

Shipping a dist we did not need costs bytes and a re-cut nobody reads. Using a
system copy we should have pinned costs a build that differs per host and fails
somewhere else entirely — which is the failure this repository has paid for most
often. When the evidence is ambiguous, pin.

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

> **REVERSED 2026-08-29** — see the amendment at the top: rosdep is no longer
> consulted at all. The paragraph below describing it as an optional resolver
> is kept for the record of why it was tried and what it measured (12/24 keys),
> not as current behaviour.

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

## Migration (phase-327 — `docs/roadmap/archived/phase-327-unified-dependency-ssot.md`)

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
