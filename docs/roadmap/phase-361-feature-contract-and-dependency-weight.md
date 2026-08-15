# Phase 360 — The `std`/`alloc` contract, and what a firmware build actually compiles

**Status (2026-08-15).** IN PROGRESS. `phase-359` (drop `std` from the core
crates) landed on `main` the same day and contradicted W1/W2.a; **that is
settled — `std` is being dropped, so this phase keeps the `std ⇒ alloc` manifest
edge and defers the rest to phase-359** (see "Overlap with phase-359" below).
W1, W2.a, W3, W3.b and all of W8 landed; the contract holds as a STATE (`alloc`
is the one heap predicate, no capability or platform feature enables it, no
`no_std` crate defaults to `std`, one `#[global_allocator]`). **W4, the gate
that would make it an invariant, is not written** — until it is, every number
here is a measurement. W2.b (two dead declarations) needs a decision; W5–W7
(dependency weight, issue 0583) are not started and are the part of this phase
nothing else owns — and after the phase-359 reconciliation they are the larger
part of what is left.

**Numbering — SIX collisions, and then the cause was fixed.** Drafted offline as
"phase-341"; renumbered 341 → 345 → 359 → 360 → **361**, with the issues moving
under it each time: 0467–0471 → 0492–0496 → 0581–0585, then 0581 → 0587, then
0582–0585 → 0591–0594, then 0586–0590 → **0596–0600**.

Every one had the same cause. `just phase-new` / `just issue-new` reserve an id
by pushing `refs/issue-ids/NNNN` to `origin` — a compare-and-swap no parallel
session can race. **This host could not authenticate**, so every call fell back
to the local maximum, which is a guess, and a guess goes stale the moment
another session pushes. Six pulls, six renumbers.

Fixed 2026-08-15 by authenticating. The reservations are now REAL: phase 361 and
issues 0591–0600 are held as refs on `origin`. The last renumber was the
informative one — 0586 through 0589 were already held by two other hosts
(`newslab-server139`, `newslab-server243`) with entirely different slugs, none of
which had landed a file yet. A local maximum could not have seen them, and no
amount of care would have.

One of those neighbours is worth naming: **`refs/issue-ids/0587` is slugged
`cargo-config-gate-treats-authored-comments-as-sync-output`** — the same defect
this branch filed as issue 0595. Two sessions found it independently within
hours. Whichever lands second should fold into the first rather than carry a
duplicate.

**Touches:** RFC-0005 (RMW layer), RFC-0006 (portable RMW/platform interface),
RFC-0033 (message field capacity — the `mode = "heap"` types this contract
governs), RFC-0034 (the `nros_platform_alloc` funnel W8.c enforces), RFC-0062
(unified dependency SSOT).
**Opens:** issue 0598 (`std` implies `alloc` in half the stack), issue 0582
(`default = ["std"]` splits compile identities), issue 0583 (47 of 57 crates in
a firmware build are proc-macro host tooling), issue 0585 (34 implicit
`alloc`/`std` enables; the allocator-ownership half is closed by W8.c).

**Not phase-334 / phase-340 territory.** Those two change *where* an artifact
lives and *when* it can be reused. This phase changes *what gets compiled at
all* and *what a feature flag means*. They touch disjoint files and compose:
issue 0582 names one of the five `-C metadata` identities issue 0446 counts, and
it is the one that survives any cache-layout fix, because to cargo the two units
are genuinely different feature sets.

## Overlap with phase-359 (drop-`std` campaign) — RESOLVED 2026-08-15

[phase-359](phase-359-drop-std-campaign.md) landed on `main` 2026-08-15, while
this branch was rebasing onto it. It is not adjacent work — it reaches the same
manifests, and on one design point the two phases pull in opposite directions.

**Where they agree.** phase-359 W10 is "flip the default, delete the feature",
and notes `nros` still defaults to `std`. W3 here already did the flip, across
eight crates plus ten in-tree dep-sites. W8.c (four `#[global_allocator]`
definitions collapsed to one owner) is orthogonal to `std` and nothing in
phase-359 touches it. W5–W7 (dependency weight, issue 0583) is unowned by any
phase — phase-355 is dependency debt, but its three issues are #374, #507, #524.

**Where they disagree.** phase-359 W8 states the flavours as `core` /
`core+alloc` / `std`, i.e. an ORDERED chain in which `std` implies `alloc`, and
records `std = ["alloc", ...]` as the existing spelling of it. W1/W2.a here
declared the opposite — that `std` and `alloc` are INDEPENDENT axes — and
deleted the `std ⇒ alloc` edge from six crates.

**What the deletion cost, measured 2026-08-15** (HEAD vs `origin/main`, `cfg`
lines in `packages/core` + `packages/api` that mention `feature = "std"`
anywhere in the predicate, comments stripped — the same scope and stripping
`check-std-census.py` uses):

| | `main` | here | Δ |
| --- | --- | --- | --- |
| `nros-node` | 140 | 206 | +66 |
| all nine crates | 252 | 340 | **+88** |

The edge did not disappear; it moved from ONE line per manifest into 123
`cfg(any(feature = "alloc", feature = "std"))` predicates at the use sites.
That is the exact quantity phase-359's `cfg` metric exists to shrink — "the
SHAPE of the split: the branches a reader has to hold in their head" — and
`nros-node`, which took 66 of them, is that campaign's largest target.

**The census cannot see any of it.** `CFG_RE` anchors `feature = "std"`
immediately after `cfg(` or `cfg(not(`, so a predicate that reaches `std`
through `any(...)` does not count. The gate reports 183 sites for both trees.
The one thing it does catch is a symptom of the same edge: `nros-node: path
346 -> 347`, because `read_rmw_selector_env` must return `std::vec::Vec<u8>`
here where `main` returns `alloc::vec::Vec<u8>` — under `std` alone there is no
`alloc` to name. **Filed as [issue 0597](../issues/archived/0597-std-census-counts-only-anchored-cfg-sites.md)**,
which measured the blind spot at 69 of 252 sites (27 %) in the census's own
scope and mutation-verified it: two planted `std`-conditional items using no
`std::` path leave the gate green.

**The decision — SETTLED 2026-08-15: `std` is being dropped, so the manifest
edge stays and the phase defers to phase-359.**

The options were (A) keep the axes independent, and (B) restore
`std = ["alloc", …]`. (A) buys one property: a `std` consumer never silently
acquires a heap. That property expires the moment phase-359 W10 lands, and its
cost — 123 duplicated predicates — has to be unwound by the same campaign. (B)
was taken.

What that meant in practice, and it is worth stating plainly because the earlier
draft of this section had it backwards: **W2.a never made the axes independent.**
It deleted the edge from six manifests and then re-created it at the use sites,
123 times, as `cfg(any(feature = "alloc", feature = "std"))` — `nros-core`'s own
comment called that "THE heap predicate". The semantics were always `std` ⇒
heap, identical to `main`'s; only the spelling differed, and the spelling was
the expensive part.

Applied:

* `std = ["alloc", …]` restored in twelve crates — the six that had it plus
  `nros-core`, `nros-serdes`, `nros-params`, `nros-rmw`, `nros-rmw-cffi`,
  `nros-bridge`, which never did and had been carrying the implication in `cfg`
  instead. `nros-node`'s `dep:portable-atomic-util` moved back off `std`; it
  rides `alloc` and `std` reaches it by implication, which is also the shape
  phase-359 W2 needs when it collapses the five field pairs.
* All 123 predicates collapsed to `cfg(feature = "alloc")` /
  `cfg(not(feature = "alloc"))`. **These gates are now in their final form:
  deleting `std` requires no `cfg` edit in this set at all.**
* `read_rmw_selector_env` returns `alloc::vec::Vec<u8>` again.

Issue 0598 is still fixed, and by a smaller change than the one it replaced: the
defect was `nros-core`'s heap gate and `nros-serdes`'s disagreeing, and
`nros-core`'s `std` now forwards `alloc` to `nros-serdes`, so the types and
their impls arrive together. Verified with the reproducer at `nros-core`'s
`std`-only feature set — `nros_core::heap::Vec<u32>` serializes.

Verified after the reversal: census back to baseline (181 cfg / 425 path, "no
crate moved"), the wide count back to `main`'s 252/183 exactly;
`check-no-std`, `check-workspace-features`, and the `nros-c` / `nros-cpp` clippy
lanes green; seven host feature combos and seven bare-metal target/feature pairs
(thumbv7em, aarch64-unknown-none, armv7r) check clean.

**Still open, and independent of this:** the census blind spot, now
[issue 0597](../issues/archived/0597-std-census-counts-only-anchored-cfg-sites.md).
`CFG_RE` anchors `feature = "std"` immediately after `cfg(` or `cfg(not(`, so
every `all(...)` / `any(...)` spelling is invisible — 69 of 252 sites in its own
scope, and it reported 183 for both trees here while the real difference was 88.
The gate cannot see the regression it exists to catch. Fixing it is phase-359's
call, not this phase's; 0597 carries the measurement and the direction.

## Goal

Two things a user should be able to answer without reading nano-ros source:

1. **"What do I turn on?"** — one axis, one meaning, the same in `packages/core`,
   `packages/rmw`, and `packages/platform`. Today `std` implies `alloc` in four
   crates and not in five others, and the source layer disagrees with the Cargo
   layer in `nros-core`.
2. **"What am I paying for?"** — a firmware build compiles the crates that end
   up in the image, plus what the proc-macro genuinely needs, and nothing that
   was added for an arm the user did not use.

## The map today

### Core

| crate | `default` | `std` | `alloc` |
| --- | --- | --- | --- |
| `nros` (umbrella) | `["std", "ffi-size-markers"]` | `["alloc", …5 crates]` | forwards to 5 |
| `nros-core` | `["std"]` | `["nros-serdes/std"]` | `["nros-serdes/alloc"]` |
| `nros-serdes` | `["std"]` | `[]` (1 cfg site) | `[]` (2 cfg sites) |
| `nros-params` | `["std"]` | `[]` (13 cfg sites) | `[]` (1 cfg site) |
| `nros-node` | `["std"]` | `["alloc", …]` | `[…, "dep:portable-atomic-util"]` |
| `nros-rmw` | **`[]`** | `["nros-core/std", "log"]` | `["nros-core/alloc"]` |

`nros-rmw` is the only converted one. Its manifest says *"explicitly (matches
nros-core). Previously `default = ["std"]`"* — but `nros-core` still declares
`default = ["std"]`. The convention was decided and applied once.

`nros-node` and `nros` say `std = ["alloc", …]`. `nros-core`, `nros-serdes`,
`nros-params`, `nros-rmw` do not. `nros-core/src/lib.rs:19` gates `extern crate
alloc` — and the `heap::{Vec, String}` re-export RFC-0033 codegen emits — on
`any(alloc, std)`, i.e. the source assumes the implication its own manifest does
not make. Issue 0598 has the reproducer: at `nros-core`'s **default** feature
set, `nros_core::heap::Vec<u32>` exists and has no `Serialize` impl.

### RMW

| crate | `default` | `std` | `alloc` |
| --- | --- | --- | --- |
| `nros-rmw-cffi` | `[]` | `["nros-rmw/std"]` | `["nros-rmw/alloc"]` |
| `nros-rmw-zenoh` | `["platform-aliases", "link-ip"]` | `["alloc", "zpico-sys/std", "nros-rmw/std", "log"]` | `["nros-rmw/alloc"]` |
| `nros-rmw-zenoh-staticlib` | `[]` | `["nros-rmw-zenoh/std"]` | `["nros-rmw-zenoh/alloc"]` |
| `nros-rmw-xrce-cffi` | `[]` | `[]` (1 cfg site) | — |
| `nros-rmw-cyclonedds` | `[]` | `[]` (**0 cfg sites, no forwarding — dead**) | — |
| `nros-rmw-cyclonedds-sys` | `["vendored"]` | `[]` (1 cfg site) | — |
| `zpico-sys` | `["platform-aliases", "link-ip"]` | `[]` (1 cfg site) | — |
| `nros-bridge` | `[]` | `["nros-node/std"]` | `["nros-node/alloc"]` (0 cfg sites — pass-through) |

This layer is already close to right: `default = []` everywhere, and where
`default` is non-empty it selects *linkage* (`link-ip`, `platform-aliases`,
`vendored`), never `std`. The two defects are `nros-rmw-zenoh` implying `alloc`
from `std` while its sibling `nros-rmw-cffi` does not, and the dead
`nros-rmw-cyclonedds/std`.

### Platform

| crate | `default` | `std` | `alloc` |
| --- | --- | --- | --- |
| `nros-platform` | **`["std"]`** | `[]` (1 cfg site) | `[]` (**0 cfg sites, forwards nowhere — dead**) |
| `nros-platform-cffi` | `[]` | — | — |
| `nros-platform-mps2-an385` | `["libc-stubs"]` | — | — |
| `nros-platform-esp32-qemu`, `-stm32f4`, `nros-baremetal-common` | `[]` | — | — |

`nros-platform` is the only crate in its layer that defaults to `std`, and its
`alloc` feature does nothing except get implied by `global-allocator = ["alloc"]`.
A user enabling `nros-platform/alloc` to get an allocator gets a no-op.

## What it costs

`cargo check --workspace --timings`, fresh target dir, 48 cores (the run stops
at `cyclonedds-sys`, issue 0390 — lower bound):

```
units = 497   cpu = 113 s   19 crates compiled under >1 feature set   ~8 s redundant

nros-core   0.60 s [alloc, std]   +  0.31 s [alloc, std, default]
nros-params 0.64 s [alloc, std]   +  0.31 s [alloc, std, default]
nros-node   0.59 s [alloc, log, std] + 0.87 s [alloc, log, std, default, rmw-cffi]
```

For `nros-core` and `nros-params` the two units differ by **the inert string
`default` and nothing else**. `libc`, `crossbeam-utils`, `winnow`, `toml_parser`
and `memchr` split the same way in the same run.

`cargo build -p nros --target thumbv7em-none-eabihf --no-default-features
--timings`, fresh target dir:

```
units = 96   cpu = 23.5 s   wall = 7.8 s
57 unique crates; 47 reachable only through the nros-macros proc-macro; 11 of runtime

attributed by name:  macro subtree 20.9 s (89 %)   everything else 2.6 s

  1.45 s  syn                        1.05 s  serde_core
  1.24 s  toml_edit                  0.91 s  winnow
  1.23 s  ros-launch-manifest-model  0.76 s  ros-launch-manifest-sched
  1.21 s  serde_derive               0.65 s  serde_yaml_ng
  1.15 s  nros-macros                0.56 s  thiserror-impl
```

Every one of the twelve most expensive units is host tooling for the
proc-macro. Note the triple: `thumbv7m-none-eabi` is **not** in
`rust-toolchain.toml`'s target set, so a build against it truncates and its
timings are not comparable — use `thumbv7em-none-eabihf`.

## Work items

**W1 — the contract, written once (LANDED 2026-08-15; REVISED TWICE before it
was — see the phase-359 overlap above).** In
`docs/design/ARCHITECTURE.md` §2 as "The `std` / `alloc` contract", normative:

> `alloc` is the heap axis, and it is the ONLY spelling of it: every heap-gated
> item is `cfg(feature = "alloc")`. `std` implies `alloc`, in one manifest line
> per crate (`std = ["alloc", …]`) and nowhere else — so a hosted consumer never
> has to name `alloc`, and an embedded one never acquires a heap without asking.
> No feature OTHER than `std` may enable `alloc`, and none may enable `std`: a
> crate that requires the heap says so with `compile_error!` naming the feature
> the user must add, and never turns it on for them.
>
> Orthogonally: `malloc` and `panic` are UNIFIED PER PLATFORM. Exactly one
> `#[global_allocator]` and one `#[panic_handler]` per image, selected by the
> `platform-<rtos>` feature — which selects the provider and nothing else.

Every crate feature table points here instead of restating it. *No code.*
Acceptance — the rule exists in exactly one place — is met, and the §2 text also
carries the reason the manifest edge won over independent axes, so a future
reader does not re-derive the reversal. One stale claim was corrected while
there: §2 said "`nros` default features are `["std"]` only", which W3 had already
made false.

The rule is no longer only text: **W4 asserts every clause of it**, and found a
crate the hand-sweep behind this text had missed.

*Two superseded drafts, kept because each was wrong in an instructive way.* The
first said "a crate that declares both must declare `std = ["alloc", …]`" —
correct, but justified only as tidiness. The second inverted it to "`std` and
`alloc` are INDEPENDENT axes; `std` does not imply `alloc`", reasoning that
`std` must not be a way to acquire a heap without asking. That reasoning did not
survive contact with the implementation: the code needs the implication either
way, so deleting it from the manifests only respelled it as `any(alloc, std)` in
123 `cfg`s. **A rule the code has to route around is not a contract.** The text
above is the first draft, with the reason the first draft lacked.

**W2 — make the manifests obey it.** Two halves, because only one of them is
monotonic.

*W2.a — twice superseded, and the third state is the first one (LANDED).* The
history is worth keeping because the middle step looked like progress:

1. `std = ["alloc", …]` on six crates — the implication written once per crate.
2. Reverted as "implicit heap enablement", and the edge removed from all six.
   But the code still needs the implication, so it reappeared at the use sites
   as `cfg(any(feature = "alloc", feature = "std"))`, 123 times.
3. Restored, and extended to the six crates that had been carrying it in `cfg`
   without ever declaring it — `nros-core`, `nros-serdes`, `nros-params`,
   `nros-rmw`, `nros-rmw-cffi`, `nros-bridge`. **Twelve crates now declare
   `std = ["alloc", …]`, and every heap gate in the workspace is
   `cfg(feature = "alloc")`.**

Step 2's stated benefit was that a `std` consumer could not silently acquire a
heap. It never delivered that — `any(alloc, std)` grants exactly the same heap,
one `cfg` at a time — and it cost phase-359 88 extra std-mentioning branches to
unwind. The measurement is in the overlap section above.

The axes do still compile independently, which is what step 2 was really
testing, and that stays true with the edge present (it is a manifest default, not
a constraint):

```
nros-core / nros-serdes / nros-params / nros-rmw / nros-rmw-cffi / nros-bridge
    --features alloc (no std)     ok
nros-core / nros-serdes / nros-rmw / nros-node / nros / nros-log
    --features std   (implies alloc)   ok
thumbv7em / aarch64-unknown-none / armv7r   bare and alloc      ok
```

Issue 0598 is fixed by the edge itself: `nros-core`'s `std` forwards `alloc`,
which forwards `nros-serdes/alloc`, so `heap::{Vec, String}` and the RFC-0033
`Serialize`/`Deserialize` impls arrive together instead of one without the
other. Verified with a reproducer built against `nros-core --features std`
alone: `nros_core::heap::Vec<u32>` serializes.

*W2.b — the dead declarations (LANDED 2026-08-15, and one of the two was not
dead).* Decided as: delete what is inert, and say so where a reader would
otherwise believe the comment.

**Deleted — `nros-platform/alloc` and `nros-platform/threading`.** Both sat under
one comment, *"capability features — enabled by RMW shims to declare
requirements"*: declarative by design, inert in fact — zero `cfg` sites anywhere
in the crate (it has only `src/`), forwarding nowhere, named by nothing in the
workspace. W2.b listed `alloc` alone; `threading` is its sibling under the same
comment and went with it rather than being left to be rediscovered. The
alternative — wiring `alloc` to the platform crates that allocate — is the wrong
direction under W8.e: a crate that needs the heap says so with `compile_error!`
naming the feature the USER must add, rather than declaring an empty feature and
hoping a shim turns it on.

**NOT deleted — `nros-rmw-cyclonedds/std`, and this phase was wrong to call it
"unambiguous: delete".** It gates two entire integration-test files, each run
explicitly with `--features bridge-stub,std`:

```
tests/registry_race.rs:29    #![cfg(all(feature = "std", feature = "bridge-stub"))]
tests/bare_metal_link.rs:24  #![cfg(feature = "std")]
```

The "0 cfg sites" claim came from a grep scoped to `src/`. An inner `#![cfg]` on
a TEST CRATE ROOT is precisely the site that scoping cannot see, and deleting the
feature turned both files into `unexpected_cfg` errors under `-D warnings` —
which is how the mistake was caught, immediately, rather than by review.

**This changes W4(c).** A gate asserting "every declared `std`/`alloc` feature
has a `cfg` site or forwards to a dependency" must search `tests/`, `benches/`
and `examples/` as well as `src/`, or it will re-derive this exact deletion
mechanically and with more authority than a human hand-grep had. That is now
stated in W4 below.

*(The old note "`nros-platform` is the only crate in its layer still on
`default = ["std"]`, so decide W2.b and W3 together" is stale: W3 landed and
`nros-platform` is `default = []`.)*

**W3 — `default = []` on every `no_std`-capable crate (LANDED).** `nros-core`,
`nros-serdes`, `nros-params`, `nros-node`, `nros-platform`, `nros`, `nros-c`,
`nros-cpp`, plus every in-tree dep-site made explicit (10 of them; the rest of
the workspace — 199 of 200 `nros` dep-sites — already spelled
`default-features = false` and named its features). Breaking for out-of-tree
consumers: `nros-core = "0.5"` is now a `no_std` build. Needs a release note.

*The acceptance criterion this phase originally stated was wrong, and the
measurement is recorded in issue 0582.* `default = []` did **not** merge any
compile unit: 497 units and 19 split crates before and after. An empty `default`
is still a feature NAME (only omitting the key removes it), and `--workspace`
builds every member as a root with its own defaults anyway. The two units are
the resolver-v2 host graph and target graph, which are legitimately different.

What W3 actually bought, and why it stays:

- **The host/proc-macro side no longer compiles the core crates with `std`.**
  Before, `nros-core` on the `nros-macros` → `nros-orchestration-ir` →
  `nros-rmw` path resolved `[alloc, std]`; now `[]`. Real work removed — it
  shows up as a cheaper unit, not a merged one.
- **Nothing can acquire `std` without saying so**, which is the user-facing
  property this phase exists for. A consumer picks per package: `std` here,
  `alloc` there, neither in the entry.
- **It surfaced issue 0584** — `nros/ffi-size-markers` was reachable only
  through the `default` set that both C/C++ consumers disable, so a `-p nros-c`
  build (what cmake/corrosion runs) never had the markers. Now requested
  explicitly at all four dep-sites.

Verified after landing:

```
host        nros / nros-c / nros-cpp / nros-node / nros-platform, --features std   ok
host        nros, no features                                                      ok
thumbv7em-none-eabihf   bare and alloc, 8 core+rmw crates                          ok
aarch64-unknown-none    bare                                                       ok
armv7r-none-eabihf      alloc                                                      ok
cargo metadata --workspace                                                         ok
```

`cargo check --workspace` still stops at `cyclonedds-sys` and
`nros-rmw-xrce-cffi` — every vendored `-sys` submodule is uninitialised on this
host (`git submodule status` shows 10+ at `-`), which is issue 0390's class and
predates this work. The C/C++ and RMW lanes therefore remain UNVERIFIED here;
they need a provisioned tree and `just ci-matrix`.

**W3.b — the codegen template (LANDED, and the premise was wrong).**

The claim this item started from — "every user-generated message package
silently defaults to `std`" — is FALSE, and the correction matters more than the
change. Measured by running the real generator into a scratch dir:

```
nros generate-rust --force -o <tmp> --rename builtin_interfaces=... --rename rcl_interfaces=...

[features]
default = []
std = ["nros-core/std", "nros-serdes/std"]
```

Users' generated crates have been `default = []` all along. The live manifest
comes from a hardcoded `format!` in `rosidl_bindgen::generator::
generate_cargo_toml` (`generator.rs:540`, written at `:629`), not from a
template.

What is actually true: `packages/cli/rosidl-codegen/packs/scaffold/
cargo_nros.toml.jinja` (mirrored byte-identical at `templates/`) DOES render a
`Cargo.toml` carrying `default = ["std"]` — and **that render is discarded**.
`rosidl-bindgen` calls `generate_nros_message_package` and consumes only
`generated.message_rs`; `GeneratedNrosPackage::cargo_toml` has no consumer
outside `rosidl-codegen`'s own tests. So the block drifted with nothing to
notice.

Landed anyway: the template's `[features]` now matches the live emitter's
`default`, carries `std = ["alloc", …]` per W2, and says in a comment that its
own output is dead on the current path. Rationale — phase-335 is wiring the
language-neutral IR path, and a dormant template that disagrees with the live
emitter is exactly how the next `default` regression arrives. The `#[used]`
lesson from issue 0584 is the same shape: a value that is only correct by
accident stays correct until the accident stops.

No regeneration of the six committed in-tree crates is needed — they already
carry `default = []`, and the `generate-lifecycle-msgs` recipe's own closing
`NOTE` says those manifests get workspace inheritance re-applied by hand after
generation, so they were never a byte-for-byte template product.

**W4 — the gate (LANDED 2026-08-15).** `scripts/check-feature-contract.py`,
wired into `check-fast` beside `check-std-census`, buildless (manifests +
sources). Six clauses over 212 crates in `packages/`:

- **(a/manifest)** a crate declaring both features lists `std = ["alloc", …]`,
  and no OTHER feature body enables `alloc` or `std` — a capability, backend or
  platform feature REQUIRES the heap and says so with `compile_error!`.
- **(a/source)** the heap gate has one spelling, `cfg(feature = "alloc")`;
  `any(feature = "alloc", feature = "std")` is rejected in either order and
  inside `not(...)`. Comments are stripped first, so the declaration comments
  explaining WHY that form was rejected are not themselves violations.
- **(b)** no `no_std`-capable crate has a non-empty `default` containing `std`
  or `alloc`. Hosted crates may.
- **(c)** every declared `std`/`alloc` feature has a `cfg` site or forwards to a
  dependency — searching `tests/`, `benches/` and `examples/` as well as `src/`,
  for the reason W2.b learned the hard way.
- **(d)** no feature in a `default` set is unreachable: if every in-workspace
  dep-site passes `default-features = false` without naming it, it is dead in
  every real build. Issue 0584's exact shape.
- **(e)** exactly one `#[global_allocator]`, in `nros-platform`. Test, bench and
  example targets are exempt — a test binary is its own image and
  `nros-tests/tests/loan_e2e.rs` legitimately installs a counting allocator. Its
  ABSENCE is also a failure: the owner going missing is not an improvement.

*Acceptance was "the script fails on a deliberate reintroduction of each of the
five".* `--self-test` does exactly that, in-tree, over synthetic trees: 13 cases,
each clause asserted to FIRE on the violation and to STAY QUIET on the legitimate
near-miss beside it (a hosted crate defaulting to `std`; a feature used only from
`tests/` via an inner `#![cfg]`; a `default` feature one dep-site does request;
an allocator inside a test target; the forbidden spelling appearing in a
comment). A gate nobody has watched fail is a gate whose polarity nobody knows.

**It found a violation on its first real run.** `nros-rmw-zenoh-staticlib`
declared both features with `std = ["nros-rmw-zenoh/std"]` — no `alloc`. W2.a's
hand-sweep had reported twelve crates and this was a thirteenth. That is the
whole argument for W4 in one line: a hand-sweep's COVERAGE is unverifiable, and
this phase's every other number is a hand-sweep. Fixed in the same commit.

**Not covered, deliberately.** `examples/**` is out of scope — those are USER
code (RFC-0026), and the contract's point is that the user spells `std`/`alloc`
at their own dep-sites. And the census blind spot that (a/source) alludes to is
phase-359's gate, not this one (issue 0597, fixed upstream).

### W5–W7 — dependency weight: re-measured 2026-08-15, and the plan does not survive it

The baseline holds, slightly larger than issue 0583 recorded: a firmware build
of `nros` (`-e normal --target thumbv7em-none-eabihf --no-default-features
--features alloc,rmw-cffi`) is **58 crates**, of which **47** are reachable
through `nros-macros`. Everything below was measured on that command.

**W5 — gate the `model = "…"` arm — CANNOT DELIVER AS WRITTEN.** The item
assumed `ros-launch-manifest-{model,sched}` are reached only by the deprecated
`model = "…"` override, so feature-gating that arm would drop them. They are on
the MAINLINE path:

* `nros-macros/src/main_macro.rs:595` parses a `SystemModel` after
  `nros_orchestration_ir::model_location::ensure_model(...)` — that is the
  `launch = "…"` arm, the shape CLAUDE.md and phase-330 W4 make canonical. Issue
  0414 is why it resolves the model itself rather than only looking for an
  artifact.
* `nros-orchestration-ir` uses them in four modules —
  `derive.rs`, `mapper_input.rs`, `rtos_realizer.rs`, `lib.rs` — for the RFC-0052
  tier schema and the chain-aware rank. Not one arm; the crate's purpose.

So the 7 crates cannot be dropped by gating a deprecated arm, because the arm is
not their only consumer. **Same shape as W2.b**: an item asserted a dependency
was reachable by one path, and the claim was never re-derived after the code
moved under it.

**W6 — `toml` 0.8 → 0.9 — STILL BLOCKED, and the blocker is confirmed rather
than assumed.** `cargo tree -i toml` shows four consumers: `nros-macros`,
`nros-orchestration-ir` (ours, bumpable) and `ros-launch-manifest-{model,sched}`
(git deps at tag `v0.1.6`, not ours). Bumping our two leaves 0.8 alive through
the other two, so the `toml_edit` → `winnow 0.7` chain survives and nothing is
un-split. W6 was blocked on W5; with W5 dead it is blocked on an upstream bump
in the `ros-launch-manifest` repo — a fork remote, which by repo policy the
agent prepares and the maintainer pushes.

**W7 — `nros-macros` optional — LANDED 2026-08-16.**

| | crates |
| --- | --- |
| with `nros-macros` | 58 |
| without | **19** |
| dropped | **39** |

Measured with `cargo tree -e normal -p nros --target thumbv7em-none-eabihf
--no-default-features --features alloc,rmw-cffi`, before and after. (The old
acceptance criterion — "the 11 runtime crates plus `paste`" — came from a
narrower feature set; at `alloc,rmw-cffi` the floor is 19.)

**The shape W7 specified is illegal, and W4 is what says so.** "A default-on
`macros` feature on `nros`" was tried: all 62 in-workspace dep-sites pass
`default-features = false`, so a default-only feature is reachable ONLY by
feature unification in a whole-workspace build and vanishes in the per-package
builds cmake runs — issue 0593's exact shape, rejected by clause (d). W3 is what
made that true: with `default = []` established and every dep-site explicit,
nothing default-on can reach anyone.

So the landed form is **`macros` opt-in, named at the dep-site**, consistent with
the rest of the contract and breaking for anyone who invokes a macro:

* `nros-macros` is `optional = true`; `macros = ["dep:nros-macros"]`.
* The three re-exports (`main`, `node`, `derive::RosMessage`) are gated on it, as
  is the in-crate `dispatch_probe_macro_test` module AND the test asserting the
  ABI symbol that module's macro emits — without the macro there is nothing to
  assert and the `extern` would not resolve.
* **145 in-tree crates** gained `"macros"` at their `nros` dep-site. Measured, not
  estimated: of 197 crates depending on `nros`, 145 invoke
  `nros::main!` / `node!` / `derive::` / `nros_macros::`, and **52 do not** — and
  those 52 now stop compiling the macro subtree entirely, which is the point.

**Cargo.lock did not move.** The change adds a feature edge, not a dependency;
`cargo metadata` resolves the root workspace unchanged.

Verified: `nros` checks with and without `macros`; `examples/native/rust/talker`
(a real `nros::main!` leaf) checks clean; `check-feature-contract` 6/6 including
clause (d), which is the clause that forced this design.

**Out-of-tree consumers who invoke a macro must add `features = ["macros"]`.**
That is a release note, and it is the cost W7 was always going to have — the
alternative was every firmware build paying 39 crates for a macro it may never
invoke.


**Status: W5 closed as not-viable, W6 blocked upstream, W7 needs a scope
decision.** The 39-crate prize is real and the mechanism is proven; what is
undecided is whether to spend a 135-manifest breaking change on it now. Issue
0583 stays open and should carry these numbers.

**W8 — no feature may enable `alloc` or `std` but `alloc`/`std` (LANDED).**
Issue 0585 enumerated 34 sites. **0 remain**, and the `#[global_allocator]`
count is 4 → 1.

- **W8.a (done)** — `global-allocator = []` on `nros-c`, `nros-cpp`,
  `nros-platform`. The `["alloc"]` was gratuitous: all three allocator modules
  use only `core::alloc::GlobalAlloc`, and `extern crate alloc` is gated
  separately.
- **W8.b (done)** — new `panic-spin` feature on `nros-c` (forwarded by
  `nros-cpp`). `#[panic_handler]` moved off `all(global-allocator, not(std),
  not(panic-halt))` onto `all(panic-spin, not(std), not(panic-halt))`, so "I
  need a panic handler" is sayable without "I need a heap". The `platform-*`
  rows select it, keeping malloc and panic unified per platform.
- **W8.c (LANDED) — `nros-platform` is the single owner of the allocator.**
  Four crates could install a `#[global_allocator]`: `nros-platform` (over
  `<ConcretePlatform as PlatformAlloc>`), `nros-c` (a direct `extern "C"
  nros_platform_alloc`), `nros-platform-mps2-an385` (its own `FreeListHeap`
  static) and `zpico-alloc` (a `GlobalAlloc` impl for that heap). The first two
  sat under *identical* gates and were kept apart by a manifest comment —
  `nros-c` deps `nros-platform` non-optionally, so any image enabling both got
  a duplicate lang item.

  The earlier note called this undecidable because "cargo offers no way for
  either crate to detect the other's feature". That framed it as a detection
  problem when it is an ownership problem: with ONE definition site, cargo's own
  feature unification makes the collision unspellable, and no detection is
  needed. `nros-c/global-allocator` forwards to
  `nros-platform/global-allocator`; the other three definitions are deleted.

  `nros-platform` is the right owner because it is the only one that covers
  both link shapes. Every `platform-*` feature resolves `ConcretePlatform` to
  `CffiPlatform` (`resolve.rs`), whose `PlatformAlloc` impl *is*
  `nros_platform_alloc` — the same funnel nros-c called directly — while the
  bare-metal Rust crates (mps2-an385, stm32f4, esp32-qemu) reach their own
  arena through the same trait. One API, one arena, per RFC-0034 D6.

  Three things fell out of it:

  - **`extern crate nros_platform` is load-bearing.** A `#[global_allocator]`
    reaches the image only if the crate DEFINING it is linked, and a dependency
    never named in code is dropped first — the `FORCE_LINK` DCE class again.
    Without it `nros-c --features platform-threadx,alloc` fails with *"no global
    memory allocator found"* while `cargo tree` shows
    `nros-platform feature "global-allocator"` enabled. `alloc-stats` masked the
    failure by giving the crate an unrelated reason to be referenced, so the
    matrix below deliberately tests `alloc` WITHOUT it.
  - **The `alloc-stats` counter moved to `nros-platform`,** beside the allocator
    it instruments. `nros-c`/`nros-cpp` keep the four `#[no_mangle]` C names and
    read the accessors. Both had defined their own `HeapStats` static exporting
    the SAME symbols, so enabling `alloc-stats` on both was a duplicate-symbol
    error waiting to happen. The counter is a pair of `AtomicUsize` written
    inline; pulling `zpico-alloc` (RMW layer) into the platform layer for it
    would invert RFC-0001's layer map, and the dep is gone from both API crates.
  - **Over-aligned requests now FAIL instead of silently succeeding.** Both
    deleted allocators discarded `layout.align()` and returned 8-aligned memory
    for any alignment — UB no build could observe. The platform ABI has no
    alignment parameter, so the surviving allocator answers `align > 8` with
    null and lets `handle_alloc_error` fire, which is what `zpico-alloc`'s impl
    already did. Behaviour change, deliberate: nothing in the nros runtime
    exceeds 8-byte alignment, so a request that does was already broken.

  `nros-cpp/global-allocator` was deleted outright — a dead declaration with
  zero `cfg` sites whose comment claimed it installed an allocator, while the
  single-runtime rule two lines below said nros-c owns it. This is exactly the
  class W4 clause (c) exists to catch.

  Verified:

  ```
  nros-c    platform-{threadx,zephyr,freertos},alloc  (thumbv7em)      ok
  nros-c    platform-threadx (no alloc) | +alloc,alloc-stats           ok
  nros-c    std,rmw-cffi,platform-posix,ros-humble [,alloc-stats]      ok
  nros-cpp  platform-{zephyr,threadx},alloc [,alloc-stats] (thumbv7em) ok
  nros-platform  platform-threadx,global-allocator[,alloc-stats]       ok
  nros-platform-mps2-an385  [cffi-export]  (thumbv7m)                  ok
  boards: mps2-an385, mps2-an385-freertos, threadx-qemu-riscv64        ok
  zpico-alloc  --no-default-features | stats  (10 tests)               ok
  check-no-direct-kernel-alloc.sh                                      clean
  `#[global_allocator]` definitions in the tree              4 -> 1
  ```
- **W8.d (done)** — 13 `platform-*` bodies across `nros-c`, `nros-cpp`,
  `nros-rmw-zenoh-staticlib` (plus the `n_board_agnostic_run_plan` fixture's
  `posix`) no longer list `alloc`/`std`. They still select the malloc/panic
  provider — that half is correct and stays.
- **W8.e (done)** — 11 capabilities now `compile_error!` naming the feature to
  add: `param-services`, `lifecycle-services` (nros-c + nros-node), `bridge`,
  `cffi`, `config`, `metadata-mode`, `signal-fd-wake`, `unix-mock`, and the six
  example `rmw-cyclonedds` rows (each example gained an `alloc = ["nros/alloc"]`
  passthrough, so the build is `--features rmw-cyclonedds,alloc`).

### What W8 uncovered — the `node_wake` predicate split

`executor/node_wake.rs` is `#![cfg(all(feature = "alloc", feature = "rmw-cffi"))]`,
but every consumer in `executor/spin.rs` was gated `all(std, rmw-cffi)`. Two
different predicates for the same thing, agreeing only because `std` implied
`alloc`. Removing the edge produced:

```
error[E0433]: cannot find `node_wake` in `super`
   --> packages/core/nros-node/src/executor/spin.rs:623
```

**The first fix was wrong** and is recorded because the failure mode is
instructive: a `compile_error!` making `nros-node`'s `std` require `alloc`. It
compiled the workspace, and then fired on the exact combinations CI builds —
`nros --features std,rmw-cffi,ros-humble`, `nros-c --features
std,rmw-cffi,platform-posix,ros-humble` — i.e. it moved the cost onto every
hosted user and contradicted the hosted shape in "Target usage" below. Reverted.

The landed fix gates the five field/initializer sites on `alloc` and gives the
hot spin path ONE shape across the axis via an accessor pair:

```rust
#[cfg(all(feature = "std", feature = "rmw-cffi", feature = "alloc"))]
fn node_wake_ref(&self) -> Option<&std::sync::Arc<super::node_wake::NodeWake>> {
    self.node_wake.as_ref()
}
#[cfg(all(feature = "std", feature = "rmw-cffi", not(feature = "alloc")))]
fn node_wake_ref(&self) -> Option<&NeverWake> { None }   // NeverWake is uninhabited
```

so without `alloc` the wake-primitive branch is statically dead rather than
restructured — no behaviour change on the path that has a heap, which is every
shipping configuration. A second site fell out of the same audit:
`read_rmw_selector_env`, gated `all(std, rmw-cffi)`, returned
`alloc::vec::Vec<u8>`; it is `std::vec::Vec<u8>` now.

Verified after W8:

```
nros-node   std,rmw-cffi / std,rmw-cffi,alloc / alloc,rmw-cffi / std      ok
nros        std,rmw-cffi,ros-humble | ros-iron | rmw-cffi (bare)          ok
nros-c      std,rmw-cffi,platform-posix,ros-humble                        ok
nros-c      platform-threadx  (thumbv7em, NO alloc)                       ok
nros-c      platform-threadx,alloc (thumbv7em)                            ok
nros-cpp    platform-zephyr,alloc (thumbv7em)                             ok
nros-bridge std,alloc | alloc                                             ok
implicit alloc/std enables across packages/ + examples/                   0
```

## Target usage — what a consumer's project looks like

The point of W1/W3/W8 is that these four shapes are the WHOLE vocabulary, and
that reading a manifest tells you whether the image has a heap.

### Rust — hosted (native / Linux board)

```toml
[dependencies]
nros = { version = "*", default-features = false, features = ["std", "rmw-cffi"] }
```

Unchanged. 93 of the 99 in-tree leaves already look like this.

### Rust — embedded WITH a heap (Zephyr, FreeRTOS, NuttX)

```toml
[dependencies]
# `alloc` is named. Nothing else can turn it on.
nros = { version = "*", default-features = false, features = ["alloc", "rmw-cffi"] }
# The platform selects the malloc/panic PROVIDER. It does not decide whether
# this image may allocate — the line above did that.
nros-platform = { version = "*", default-features = false, features = ["platform-zephyr"] }
```

Unchanged — `examples/zephyr/rust/talker` is already exactly this.

### Rust — embedded with NO heap on the nros surface

```toml
[dependencies]
nros = { version = "*", default-features = false, features = ["rmw-cffi", "ros-humble"] }
nros-platform = { version = "*", default-features = false,
                  features = ["platform-threadx", "global-allocator", "critical-section"] }
```

`examples/qemu-riscv64-threadx/rust/talker`, verbatim and unchanged. Note what
this says: the platform installs a `#[global_allocator]` (the RTOS heap exists
and C code uses it) while `nros` itself is compiled with no `alloc` — those are
different questions, and after W8 they stay different. **This is the shape the
whole contract exists to make readable.**

What DOES change here is the RMW row:

```toml
# before — selecting a backend silently added a heap to the nros surface
rmw-cyclonedds = ["dep:nros-rmw-cyclonedds-sys", "nros/alloc"]
# after — the backend row selects a backend; if it needs the heap, the user adds it
rmw-cyclonedds = ["dep:nros-rmw-cyclonedds-sys"]
```

with `nros-rmw-cyclonedds` carrying

```rust
#[cfg(not(feature = "alloc"))]
compile_error!("the Cyclone DDS backend allocates: add \"alloc\" to your `nros` features");
```

### C/C++ — already correct, and this is the model

The CMake path does not have this defect. `nros_feature_set()`
(`cmake/NanoRosFeatureSet.cmake:99-135`) emits the axis EXPLICITLY, per
platform, next to the platform feature:

```cmake
posix             -> std   platform-posix
nuttx             -> std   platform-nuttx
threadx_linux     -> std   platform-threadx
freertos/esp_idf  -> alloc panic-halt platform-freertos
threadx_riscv64   -> alloc panic-halt platform-threadx
<unknown cross>   -> alloc panic-halt platform-<X>
```

A C/C++ consumer writes only intent, and the axis is derived once, in one
readable table:

```cmake
find_package(nano_ros REQUIRED)
set(NANO_ROS_FEATURES "param_services;lifecycle")   # capabilities, image-level
nano_ros_add_node(talker ...)
```

`panic-halt` beside `alloc` is the malloc/panic unification already working:
one provider, chosen by the platform, named in the build. W8 makes the Rust
manifests agree with this table instead of duplicating half of it implicitly.

**Consequence: W8.d is a no-op for every C/C++ consumer** — CMake already passes
`std`/`alloc` itself, so deleting `"alloc"` from the `platform-*` feature bodies
changes nothing on that path.

### Blast radius — measured, not estimated

```
in-tree leaves depending on nros / nros-c / nros-cpp   : 99
  already name `alloc` or `std` at the dep-site        : 93
  rely on an implicit enable                           :  6
      examples/qemu-riscv64-threadx/rust/{talker,listener,
        service-{client,server},action-{client,server}}
```

All six are the `rmw-cyclonedds = [..., "nros/alloc"]` row above.

**That count is right for the question it asked and WRONG for the migration.**
It counted leaves naming `alloc` *or* `std`. But once `std` no longer implies
`alloc`, naming `std` is not enough for any consumer that touches an
alloc-gated API — and on the hosted side that is nearly all of them. The real
figure:

```
dep-sites naming `std` but NOT `alloc`  : 77
```

`nros-board-linux` is the first one the build hit: it deps
`nros = { features = ["std", "rmw-cffi"] }` and calls
`Executor::from_session_in`, which is `alloc`-gated —
`error[E0599]: no function or associated item named 'from_session_in'`.

This is the open decision recorded under "W8 — the hosted question" below.
Earlier drafts said "~30" (a guess) and then "6" (a correct answer to the wrong
question); neither is the migration cost.

## Sequencing

W1 → W2 → W3 → W4 is one thread (manifests + gate); W5 → W6 → W7 is the other
(dependency weight). They touch different manifests and can run in parallel.
W3 and W7 are both breaking for out-of-tree consumers — land them in the same
release and write one note, not two.

## Measurement

Re-run both commands after each work item and record the numbers in the item.
The two acceptance numbers are the **57-crate firmware count** and the **count
of crates compiled under more than one feature set** in a workspace check.
