# RFC-0097 — Release composition: four version axes, and which artifact carries each

**Status:** Draft (2026-09-10)

Amends RFC-0090 (which anticipated this: *"a future refactor may split the 'CLI
version' from the 'runtime ABI version' […]; the call sites here would not
change"*) and builds on RFC-0095 (the store is the root) + phase-440 W7 (the
per-project pin). Supersedes nothing.

## The defect, in one ratio

Four things carry a version, and **one artifact carries all four**, so any of
them moving forces a release of the whole. Measured on `main`, 2026-09-10:

| axis | changes | over |
| --- | --- | --- |
| `packages/cli` (orchestration, provisioning, build driving) | **710** | 60 days |
| runtime crates the CLI compiles (`cli-source-dirs.txt`) | **248** | 60 days |
| `nros-sdk-index.toml` (the prereq table) | **70** | 60 days |
| **`NROS_CODEGEN_VERSION`** | **2** | **all time** |
| union that would force a CLI re-release | **893** of 5217 commits | 60 days |

> **The axis with compatibility meaning moves twice, ever. The artifact carrying
> it would move ~450 times as often.**

`NROS_CODEGEN_VERSION` is the only one of the four that can make a user's
existing generated code wrong (RFC-0090). The other three cannot: an
orchestration fix, a QEMU bump and a board-table edit are all invisible to
already-emitted code. Yet today they ship in the same artifact and under the
same version, so a user cannot take one without taking all.

`release-nros.yml` does not merely bundle them — it **asserts they are equal**,
checking the release version against `NROS_CODEGEN_VERSION` and against
`[tool.nros] version` in the index. The coupling is mechanised, which is why it
has not drifted; it is also why nothing can move alone.

## What is already decomposed, and better than expected

**`NEWSLabNTU/nano-ros-sdk` is already the prereq artifact repo.** It holds only
`.github` + `scripts` and publishes **per-tool** releases —
`qemu-11.0.0-nros6`, `arm-none-eabi-gcc-13.2-nros4`, `openocd-0.12.0-nros2`,
`xrce-agent-2.4.3-nros1`, twelve-plus of them, each on its own cadence with no
reference to any nano-ros version.

So the prereq axis needs **no work**. `nros-sdk-index.toml` is not a versioned
artifact; it is a **manifest of pointers** into a stream that already exists.
An earlier draft of this RFC proposed giving the index its own version — that
was wrong, and worse than what the tree already does: per-tool is finer than
per-index, and already live.

Two of the last twenty index commits touched a `version =` line at all. The
other eighteen were structural — board tables, `[source.*]` entries, prereq
mappings. So the index churns because it is a **live data file**, which is the
argument for not shipping it inside a binary rather than for versioning it.

## `nano-ros` has no releases, so this is greenfield

`gh release list` on `NEWSLabNTU/nano-ros` is **empty**. `scripts/install.sh`
resolves assets from a release stream that has never been published.

There is therefore no installed base, no migration and no compatibility promise
to break. The decomposition can be chosen once, freely — which is the cheapest
this decision will ever be.

## D1 — The fault line is compatibility, not lifetime

The rustup analogy suggests splitting installer from toolchain, on the grounds
that they have different LIFETIMES. The measurements say nano-ros's fault line
is elsewhere: `packages/cli` churns hardest (710) and carries **no**
compatibility promise, while `NROS_CODEGEN_VERSION` barely moves and carries all
of it.

So the split is **the compatibility-bearing part vs everything else**. A CLI
that churns 710 times in 60 days is fine, as long as a user may take any of
those 710 without re-emitting code.

## D2 — Measured: the ABI-bearing crates are codegen-side

Whether codegen can be extracted is a question about who consumes what. Counted
across `packages/cli/*/src`:

| crate | consumers | verdict |
| --- | --- | --- |
| `nros_core` | `generator/` ×2, `types.rs`, `templates.rs`, `lib.rs` | codegen |
| `nros_serdes` | `generator/` ×3, `templates.rs`, `schema_value.rs`, + 1 | codegen |
| `nros_rmw` | `templates.rs`, `generator.rs`, `config.rs` | codegen |
| `nros_orchestration_ir` | orchestration ×6, codegen ×5, cmd ×5 | **everywhere** |
| `nros_board_common` | cmd ×2, orchestration ×1 | not codegen |

The three ABI-bearing crates are **almost entirely codegen-side**. Extracting
codegen moves them out of the CLI.

**The one apparent crossover is not one.** `orchestration/bridge_gen.rs` is the
only orchestration file naming `nros_serdes`, and what it does is emit
`::nros_serdes::Field` **as a string into generated code**. It is a generator
filed under `orchestration/`, not a dependency — the line holds.

**`nros_orchestration_ir` does not move, and does not need to.** Its own header:
*"the small set of `system.toml` types that describe scheduling tiers […]
depended on by BOTH"*. It is build-time IR, not something user firmware links
against, so it has no bearing on whether emitted code matches a runtime.

## D3 — Four artifacts, and what each may promise

| artifact | contents | version means | cadence |
| --- | --- | --- | --- |
| **launcher** (`$NROS_STORE/bin/nros`) | read pin → ensure toolchain → `exec` | nothing; it is a proxy | ~never |
| **CLI** (`toolchains/<ver>/bin/nros`) | orchestration, provisioning, build driving, `orchestration_ir`, `board_common` | "these features exist" | fast (710/60d) |
| **codegen** (`toolchains/<ver>/bin/nros-codegen`) | the emitter + `nros_core`, `nros_serdes`, `nros_rmw`, and `NROS_CODEGEN_VERSION` | **"code emitted before this still compiles"** | rare (2 ever) |
| **prereq tools** (`nano-ros-sdk` releases) | per-tool tarballs | that tool's own version | per tool, already live |

The promise column is the point. Only one row makes a claim about the user's
existing artifacts, and it is the row that almost never moves.

## D4 — The launcher must be a SEPARATE binary, which it is not today

phase-440 W7 put dispatch in `packages/cli/nros-cli/src/main.rs`, before clap —
correct for "must keep working when older", and it does dispatch. But it means
the fronted `nros` **is** the full CLI, so the launcher's lifetime is the CLI's
lifetime and "the launcher never changes" is not true of it.

For the launcher to be stable it has to be its own small binary, whose only
inputs are the pin file and the store layout. Everything else belongs behind the
`exec`. W7's `dispatch.rs` is already that logic, isolated; what remains is a
second bin target that contains only it.

This is the one place where rustup's shape is right for the same reason it is
right there: a proxy that outlives what it proxies cannot share a release with
it.

## D5 — Stop shipping the index inside the binary

`shipped_index()` resolves `<prefix>/share/nros/nros-sdk-index.toml` from the
release asset. That is why 70 index commits per 60 days would each be a CLI
release.

The index is a manifest of pointers into `nano-ros-sdk`, so it belongs with the
**toolchain**, fetched, not compiled in — and a newer index may be used with an
older CLI, because a pointer table has no ABI. This is the cheapest of the
changes here and removes the second-largest release trigger.

## Simulated workflows

### The user

```console
$ curl -fsSL https://nano-ros.dev/install.sh | sh
  installed: ~/.nros/bin/nros          # the LAUNCHER only, ~2 MB

$ cd ~/my-robot && cat nros-toolchain.toml
[toolchain]
version = "0.7.1"       # the CLI/toolchain bundle
codegen = 7             # written by nros, not by hand — the compat token

$ nros build
  info: fetching nano-ros 0.7.1 (release asset, 38 MB)
  info: index 2026-09-10 (prereq pointers)
  error: missing prerequisites for this build:
    - Zephyr SDK 0.16.8   (board `mps2-an385`)
        run: nros setup mps2-an385
  nothing was built.

$ nros setup mps2-an385     # pulls per-tool assets from nano-ros-sdk
$ nros build                # artifacts in ~/my-robot/build/
```

Taking a newer CLI, with no risk to emitted code:

```console
$ nros toolchain install 0.7.9 && nros pin 0.7.9
$ nros build
  info: codegen 7 unchanged — generated code is not re-emitted
```

Taking a newer codegen, which IS a compatibility event and says so:

```console
$ nros pin 0.8.0
  warn: codegen 7 -> 8. Generated bindings will be re-emitted; commit the result.
```

### The contributor

Unchanged, and deliberately so — RFC-0095 D0/D5. A clone uses its own build:

```console
$ git clone --recurse-submodules … && cd nano-ros
$ direnv allow && just setup-cli && just ci gate
```

The launcher is not involved: inside a checkout the ownership guard requires
that checkout's own build, and dispatch declines at its own seam.

### Maintaining the four axes

| what changed | what is released | what a user must do |
| --- | --- | --- |
| a QEMU patch | `nano-ros-sdk`: `qemu-11.0.0-nros7` | nothing, or `nros setup` for that board |
| the prereq pointer to it | a new **index** with the toolchain | nothing |
| an orchestration fix | a new **CLI** | take it whenever; no re-emit |
| the message format | a new **codegen**, `NROS_CODEGEN_VERSION` + 1 | re-emit, deliberately, on a pin bump |
| the launcher's three jobs | a new **launcher** (rare) | `nros self update` |

## Order of work

1. **D5** — index out of the binary. Smallest, removes 70/60d of release
   pressure, no new artifact.
2. **D4** — launcher as its own bin target over W7's existing `dispatch.rs`.
   Small, and required before any claim that the launcher is stable.
3. **D2/D3** — extract `nros-codegen`, carrying the three ABI crates. The
   largest change, and the one that gives `NROS_CODEGEN_VERSION` a version of
   its own.
4. Publish the first `nano-ros` release, once there is something whose
   composition is declared rather than asserted-equal.

Steps 1 and 2 are worth doing whether or not step 3 ever happens.

## What this does NOT claim

**Extracting codegen does not make codegen rare to REBUILD.** The 248
runtime-crate commits per 60 days are mostly `nros-core`/`nros-serdes` churn
that does not bump `NROS_CODEGEN_VERSION`. What the split changes is how often
that churn is a **compatibility event** for a user — from "every release" to
"twice, so far". Saying it the other way would over-promise, and the difference
is the whole value.

## D6 — No acceptance range. Codegen and the runtime stay ONE unit

Decided 2026-09-10. `NROS_CODEGEN_VERSION` remains an exact-match `u32`:
generated code must match the runtime that consumes it, exactly, and a range is
not introduced.

That is the cheap and honest choice — a range is a standing compatibility
commitment across every future emitter change, and nothing in the measurements
asks for one. Two codegen changes in the project's life do not justify a
promise that every later change must keep.

## D7 — Which REVERSES D3's binary split: declare the version, do not extract it

The extraction only pays if the CLI can then move without moving codegen. That
requires the CLI↔codegen seam to be stable. **It is not, and it is
accelerating:** `cmd/codegen.rs`, the `--args-file` interface described in its
own help as "the interface the cmake / build.rs consumers speak", changed **13
times in 60 days** — of **23 changes all time**. The JSON schema itself
(`CodegenArgs`) has never changed; every one of those is a new subcommand
(`entry`, `entry-node`, `entry-pack`, `resolve-deps`,
`cyclonedds-descriptors`), each phase-numbered.

So a newer CLI would routinely require a subcommand an older codegen lacks. The
two would ship together in practice, and the split would cost a binary boundary
while buying nothing.

**What the user actually needs is not two binaries. It is to know whether a CLI
upgrade forces them to re-emit generated code.** A declared field gives exactly
that, and is immune to the seam churning because the seam stays internal:

```toml
# share/nros/manifest.toml, inside the release asset
version  = "0.7.9"      # the toolchain: CLI + codegen + runtime
codegen  = 7            # the ONLY field that can invalidate existing output
index    = "2026-09-10"  # pointers into nano-ros-sdk
nano_ros = "abc1234"     # the runtime commit
```

`0.7.1` and `0.7.9` both declaring `codegen = 7` is the whole feature: the user
takes either, and nothing is re-emitted. `0.8.0` declaring `codegen = 8` is a
compatibility event, and says so before doing anything.

This also reframes the original complaint. "We rebuild the CLI over and over" is
a MAINTAINER cost — cutting releases — not a user cost, because a user downloads
rather than builds. The user's cost is re-emitting, and that is what the
declaration removes. Conflating the two is what made a binary split look
necessary.

`release-nros.yml` therefore stops ASSERTING the three versions equal and starts
RECORDING them. That is the whole change on the release side.

## Revised order of work

1. **D5** — the index leaves the binary. Removes 70/60d of release pressure and
   needs no new artifact.
2. **D7** — the release declares its components instead of asserting them
   equal. One file in the asset; unlocks every version claim above.
3. **D4** — the launcher becomes its own bin target over W7's `dispatch.rs`, so
   the thing users install stops sharing a lifetime with the thing that churns.
4. Publish the first `nano-ros` release, now that its composition is declared.

**D2/D3's extraction is NOT scheduled.** The dependency measurement that made it
look feasible still holds — the ABI crates really are codegen-side — but D7
delivers the user-visible benefit without it, and the seam measurement says the
split would not deliver independence today. Revisit only if that seam stabilises.

## Open

* **Where the launcher's release lives.** `nano-ros` releases the toolchain; a
  launcher meant to outlive it may want its own repo, as `nano-ros-sdk` holds
  tools. Undecided, and D4 does not depend on the answer.
* **Whether the seam should be stabilised deliberately.** 13 changes in 60 days
  is a fast-moving internal interface, which is fine while it IS internal. It
  becomes a problem only if D3 is ever revisited, so the question is parked with
  D3 rather than open on its own.
