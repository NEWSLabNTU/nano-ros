# RFC-0095 — The store is the root: nano-ros as a provisioned artifact

**Status:** Draft (2026-09-09)

Amends RFC-0014 (`nros setup` gains a subject: nano-ros itself). Builds on
RFC-0062 (the unified dependency SSoT) and RFC-0065 D2 (a missing prerequisite
fails at preflight, naming its remedy) — neither is revised, both become
load-bearing for a second consumer. Supersedes nothing.

Home phase: [phase-440](../roadmap/phase-440-nros-store-is-the-root.md).
Prior phases: 435 (provisioning resolves by axis), 422 (provisioning has one
path), 413 (CI workflow user parity).

## The end state

Today a user of nano-ros **places the repository**: they clone it, build a CLI
inside it, and run their work from within it. The tree's layout is the
interface. Tomorrow they should not know where it lives.

```
curl -fsSL <installer> | sh        # puts `nros` on PATH — nothing else
cd ~/my-robot
nros build
```

`nros` resolves the nano-ros version that project pins, and builds against it —
the way `cargo` builds against a `rust-toolchain.toml` without the user ever
siting the Rust source tree.

This RFC does not implement that. It removes the assumptions that make it
unreachable, in an order where each step is worth landing on its own.

## D0 — Two audiences, and the difference is ownership

Not "clones or doesn't". **Who owns the tree, and who names the version.**

| | contributor | user |
| --- | --- | --- |
| nano-ros | their clone — **mutable**, HEAD | `toolchains/<ver>/` — **immutable** |
| version | whatever HEAD is | named in their project |
| which CLI | that clone's own build | shim dispatches by pin |
| may edit nano-ros | yes, that is the point | no |
| build output | clone / fixture dirs | their project's `build/` |

**The consequence is larger than it looks: the entire staleness apparatus is a
contributor-only concern.** The CLI source stamp, `cli-source-dirs.txt`, fixture
re-staling, the load-bearing "rebuild the CLI *then* fixtures" ordering — all of
it exists because a contributor's nano-ros changes underneath them. An immutable
`toolchains/0.6.2/` cannot go stale. A user never meets any of it, and no
user-facing design should grow a second copy of it.

(What a user still meets is `nros sync` after editing their own `.msg` files.
That is their source going stale against itself, which is a different question.)

## The shape of the tree today, and the category that was hiding

An earlier draft of this RFC said "zephyr-workspace, 144 G". That conflated two
different things, and the correction matters:

| | measured | kind |
| --- | --- | --- |
| `zephyr-workspace/{zephyr,modules}` | **4.1 G** | provisioned SOURCE |
| `zephyr-workspace/build-*` | **140 G** | BUILD OUTPUT |
| `third-party/` tracked submodules | 20 of them | repository content |
| `third-party/{make,ninja,ros}` | — | provisioned tools |
| `external/` | 287 M | vendored tool sources + a 23 M stray duplicate of the 68 G tracked PX4 submodule |

So there are **three** categories, not two, and today all three share
directories:

1. **provisioned source** — belongs in the store, version-keyed, shared;
2. **build output** — belongs to the project that produced it, never in the
   store, never mirrored;
3. **tracked content** — belongs in the repository.

That is exactly why `scripts/dev/ros2-box-sync.sh` needs per-root `--include`
rules to rescue tracked paths from its own broad `--exclude 'target/'` and
`'build-*/'`: build output and provisioned source live in the same directory, so
no name-based rule can separate them. The rule set cannot keep up with a root
count that grows, and a missing rule is silent —
`check-box-sync-covers-tracked-source` currently reports **2914 tracked paths**
that would not reach the mirror.

## D1 — A provisioned tree never lives inside any checkout

`nros` refuses to run against a directory owned by a *different* checkout
(phase-431 W1): if the path it operates on sits inside a tree carrying
`packages/core/nros-core/Cargo.toml`, the running binary must be that tree's own
`packages/cli/target/**` build. A provisioned workspace nested inside a second
checkout is therefore unusable by every CLI but that checkout's — a property of
where somebody put a directory, discovered fifteen minutes into a fixture build.

Outside every checkout, the guard takes its **first silent case** and the
question cannot arise.

## D2 — The store is the root, and it is version-keyed

```
$NROS_STORE/                       # ~/.nros — resolved via NROS_HOME/NROS_STORE,
├── bin/nros                       #   never an absolute literal, and never in a gate
├── toolchains/0.6.2/              # nano-ros ITSELF, pinned
├── workspaces/zephyr/3.7.0/       # provisioned SOURCE (4.1 G), shared by every project
├── sdk/zephyr-sdk/0.16.8/         # unchanged from today
└── fetch/                         # today's `external/`
```

Version-keying is what makes the end state possible rather than merely tidier:
two projects pinning two nano-ros versions must not collide, and one Zephyr
workspace must be shared by every project that wants that version rather than
copied per checkout.

**Build output is never in the store.** It stays with the project, like colcon.

## D3 — `third-party/` holds tracked submodules and nothing else

`make/`, `ninja/`, `ros/` move to the store; `external/` retires into `fetch/`;
the stray PX4 duplicate is deleted rather than moved. Then "pinned source or
provisioning?" is answered by the path instead of by reading `.gitignore`, and
the repository becomes a clean artifact the CLI can fetch and pin — which D2's
`toolchains/<version>/` requires.

## D4 — Resolution inverts: store first, checkout last

```
$NROS_<X>_WORKSPACE  →  $NROS_STORE/workspaces/<name>/<version>  →  <checkout-relative>
```

Today it is repo-first, store-never, and the fallback is worse than the primary:
`$repo_root/../nano-ros-workspace` is a path relative to a clone a user does not
have.

The checkout-relative arm **stays, last**, so a contributor patching a Zephyr
module points `NROS_ZEPHYR_WORKSPACE` at their own tree and nothing stops them.
It simply stops being the default, and nothing new may be written against it.

**One resolver.** Consumers call the helper. Today the Zephyr chain alone has
three copies (`just/zephyr.just`, `scripts/build/west-fixtures.sh`,
`scripts/check-tier-preconditions.sh`), which is the two-spellings shape this
repo keeps paying for.

## D5 — The ownership guard already anticipates having no checkout

Stated so nobody narrows it later. The guard is deliberately silent when "the cwd
is not in a checkout — a user's own project, which is exactly what a released
binary is FOR". It is already correct for both audiences: a contributor inside a
clone must use that clone's build; a user in their own project may use a
store-dispatched binary. **No change is needed here, and none should be made.**

## D6 — Resolve is automatic; acquire is explicit

The rustup analogy is right about resolution and wrong about acquisition.

| | automatic | why |
| --- | --- | --- |
| read the pin, locate store entries | **yes** | rustup's genuinely good half; a hand-run resolve step is friction with no payoff |
| fetch the **toolchain** | **yes** | one axis, bounded, deterministic — *conditional on D8's release artifact* |
| fetch **platform prereqs** | **no** | many axes; some need root; a build that provisions can fail on network and is not reproducible |

The third row is already decided and implemented — RFC-0065 D2, `preflight.rs`:

> A missing prerequisite must fail HERE, naming the `nros setup` line that fixes
> it — never mid-compile with a cryptic linker error.

This RFC keeps it and extends it to the user audience. It is the rosdep/colcon
split, and it is the right one: a project targeting zephyr + freertos + nuttx
wants three SDKs, and `nros-ros2` cannot be provisioned at all without root,
which "nothing in this repo sudos" — a tool that silently fetches some
prerequisites and refuses others teaches an inconsistent model.

## D7 — Three versions, and updating one moves none of the others

| | rustup | nros |
| --- | --- | --- |
| launcher | `rustup` | `$NROS_STORE/bin/nros` |
| does the work | `toolchains/1.98.1/` | `toolchains/0.6.2/` |
| what the project says | `rust-toolchain.toml` | `nros-toolchain.toml` |

`nros self update` moves the shim and **nothing else**. If a CLI update moved
the pin, every update would be a silent rebuild of everyone's firmware — worse
here than in Rust, because a toolchain change moves the codegen emitter, and
RFC-0090 makes codegen version the compatibility token.

Bumping a pin is an explicit source change and behaves like one:

* the new toolchain is fetched (D6, bounded);
* generated code re-stales, through the mechanism that already exists — the
  codegen tool is in the input signature (#182). Contributors key that on the
  source stamp; users key it on the toolchain version. Same machinery;
* prerequisites may differ, so preflight refuses with the new remedy;
* **old projects keep building** — `workspaces/zephyr/3.7.0` and `4.4` coexist;
* **rollback is free** — `nros pin 0.6.2` re-uses what is still in the store.

The last two are the payoff of version-keying, and the reason D2 is not
cosmetic.

## D8 — The shim has three jobs, and a fourth is a bug

1. read the pin; 2. ensure that toolchain exists; 3. `exec` it.

Everything else lives in the toolchain's own binary. A shim that does more
becomes a shim-versus-toolchain compatibility matrix, which is the problem it
exists to avoid — and it must keep working when it is *older* than what it
launches.

This is also what settles D6's second row: **auto-fetching the toolchain is only
defensible if `toolchains/<ver>/` is a release artifact.** If it is a git
checkout that must `cargo build` a CLI, "automatic" means a ten-minute compile
inside what looks like a build command, and the honest design is an explicit
`nros toolchain install`.

## D9 — Pin on first build

A project with no `nros-toolchain.toml` floats, which for embedded output is a
reproducibility bug waiting to be discovered on someone else's machine. This
repo already holds the stricter line one layer down — *lockfiles change only
when a dev means it* (issues 0359/0378), with `--locked` injected project-wide
so a mismatch fails rather than silently rewriting.

Same rule, one layer up: the first `nros build` **writes** the pin it used and
says so, the way `cargo` writes `Cargo.lock`. After that, `nros self update`
cannot move it.

## D10 — One provisioning engine, three consumers

Contributor, user and CI runner provision through the same path, from the same
SSoT (`nros-sdk-index.toml`), into the same store. This repo already argued it
for the case one would expect to differ most: `runner-provision.sh` — *"a runner
and a contributor must provision the same way or the two drift"* — and the
container image "provisions THROUGH `runner-provision.sh`, not through a second
list of apt packages."

What differs between the audiences is **which nano-ros is authoritative for the
index and the codegen**, not the mechanism.

## D11 — The store accumulates, so it needs a way to shrink

Toolchains and workspaces are additive by design (that is what makes rollback
free), so disk grows without a verb to reclaim it. Reference-counting against
pins scattered across a filesystem is not reliable, so the honest shape is
explicit and inspectable rather than clever:

```
nros store list                    # what is installed, size, last used
nros store gc --older-than 90d     # unused entries, with a dry run by default
nros toolchain uninstall 0.6.2     # refuses while a known pin names it
```

Decided rather than discovered when a disk fills. `--dry-run` is the default
because a store holding a 4 G workspace shared by three projects is exactly the
thing nobody wants deleted by a flag they misread.

## D12 — The build system encodes no environment shape (issue 1248)

The only question it may ask about an environment is **are ROS 2 ament packages
discoverable, so message packages can be found?** Bare metal, container, VM,
distrobox — that is the operator's choice, and encoding one of them is how a
third state gets invented that neither answer covers.

It had. `ros2-box-sync.sh` rsync'd the host tree into a `<checkout>-box`
sibling, with a marker file, its own env knobs, and a PUSH-LANE gate whose whole
subject was whether that copy was faithful. The rules excluded build output by
directory NAME, and this RFC's own finding is why they could not work: **provisioned
source and build output share directories**, so no name-based rule separates
them. Five instances, each fixed by anchoring one pattern while the class
survived — `packages/cli/build-support/` (the box could not compile `nros`),
Zephyr's `drivers/i2c/target/`, then #1229, #0925 and #1243's four causes.

Retired. **Build where you run**: clone inside the box. That was already the rule
(0759 refused sharing a checkout outright); the mirror existed to make it
convenient and reintroduced the hazard one layer over.

What survives is not a box concept: a distrobox **shares `$HOME`**, so `~/.nros`
is the same directory for two toolchains. That is D2's *a store belongs to one
toolchain* meeting an environment where a different machine does not imply a
different `$HOME` — one variable, not a mode. It also makes D2 load-bearing
sooner than W4: until the store is version- and toolchain-keyed, every
environment that shares `$HOME` needs a hand-set `NROS_HOME`.

## What this already broke

* **The tier-2 lane, nightly.** `run-matrix` has produced no runtime verdict in
  nine consecutive runs; two died on D1 — a shared Zephyr workspace on the
  self-hosted runner sitting inside a *second* nano-ros checkout, so every
  `nros` invocation was refused, reported as an error about a binary, deep in a
  cmake configure, naming neither the workspace nor the fix.
* **`check-box-sync-covers-tracked-source`**, 2914 paths, above.
* **A gate that assumed an environment.** It asks whether the distrobox mirror
  would drop tracked source — a question only where a box is in play, yet it ran
  on the push lane for everyone, and the more a host provisions the likelier it
  is red about a mirror that may never be built. Moved to the build tier
  (phase-440 W2). **A build system must not assume it is inside a distrobox, a
  container, or a VM** — and neither should its gates.

## Open

* **Store sharing across users on one machine.** `~/.nros` is per-user; a CI
  host with two accounts provisions everything twice. A shared read-only store
  with a per-user overlay is the usual answer; out of scope here.
* **Where the pin lives for a multi-package workspace.** One
  `nros-toolchain.toml` at the workspace root is the obvious shape, but it must
  survive `nros build` invoked from a subdirectory.
* ~~**What a release artifact contains.**~~ **ANSWERED by the tree, not by this
  RFC.** `scripts/install.sh` (phase-431 W4) already downloads a versioned
  `.tar.zst` into `$NROS_HOME/sdk/nros/<version>` and fronts it via the
  binary's own `sdk-front`. So D8's condition is met — auto-fetching a
  toolchain is a download, not a `cargo build` — and D2's
  `toolchains/<version>/` is a RENAME of storage that exists. What its asset
  should CONTAIN (host binaries only, or the sources codegen needs) is still
  open and decides whether a store can be shared read-only.
