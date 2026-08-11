---
id: 493
title: "Two cargo workspace ROOTS share one corrosion target dir, so a mixed
  workspace's umbrella staticlib bundles the nros stack twice and the link dies
  on duplicate `#[no_mangle]` symbols"
status: open
type: bug
area: build
related: [phase-340, phase-344, issue-0492, rfc-0070]
owner: phase-340 / phase-344
---

## Symptom

`just build-test-fixtures lane=native` reaches the workspace fixtures and dies
on `examples/workspaces/mixed` (`ws-group-10`):

```
ld.lld: error: duplicate symbol: nros_rmw_cffi_register
>>> defined at lib.rs:731 (packages/rmw/cffi/src/lib.rs:731)
>>>   nros_rmw_cffi-df6cce59090cd17a.….rcgu.o in archive libnros_ws_runtime.a
>>> defined at lib.rs:731 (src/lib.rs:731)
>>>   nros_rmw_cffi-a49bb61a295363bc.….rcgu.o in archive libnros_ws_runtime.a
```

…and the same for `nros_rmw_cffi_lookup`, `_register_named`,
`_registered_names`, `_set_custom_transport`.

## What is actually duplicated

**One archive, two whole compilations of the nros stack.**
`libnros_ws_runtime.a` (44 MB) contains two `-C metadata` identities of each of
**ten** crates:

```
atomic_waker  log  nros_core  nros_log  nros_node  nros_params
nros_platform_api  nros_platform_cffi  nros_rmw  nros_rmw_cffi
```

For `nros_rmw_cffi`: 9 objects under `df6cce59090cd17a`, 9 under
`a49bb61a295363bc`.

Both rlibs sit in the same `deps/` and were built **41 s apart in one run**
(13:20:57 and 13:21:38) — so this is not stale accumulation, and wiping the tree
does not fix it.

## Cause

The two debuginfo paths name the mechanism:

| identity | debuginfo path | means |
| --- | --- | --- |
| `df6cce5…` | `packages/rmw/cffi/src/lib.rs` | compiled with the **repo root** as workspace root |
| `a49bb61…` | `src/lib.rs` | compiled as a path dep of a **different** workspace |

Two cargo invocations, two workspace ROOTS, **one `--target-dir`**:

1. `packages/api/nros-cpp/CMakeLists.txt:54` calls `corrosion_import_crate()`
   **unconditionally**, so every consumer that `add_subdirectory`s nano_ros
   builds `--manifest-path <repo>/packages/api/nros-cpp/Cargo.toml` — root
   workspace context.
2. A workspace that has Rust nodes ALSO gets the synthesised umbrella
   (`cmake/NanoRosRuntimeCrate.cmake`, phase-241 W11 "Option D"):
   `nros_ws_runtime`, its own workspace, carrying
   `nros-cpp = { path = "<repo>/packages/api/nros-cpp" }` as an out-of-workspace
   path dep.

Corrosion derives its target dir from `CMAKE_BINARY_DIR` and offers **no
override** — phase-344 measured exactly this — so both land in
`<build>/cargo/build`. Cargo computes a different `-C metadata` per workspace
root, `deps/` accumulates both, and the umbrella staticlib bundles a mix: some
member crates were compiled against one `nros_rmw_cffi`, some against the other.
Every `#[no_mangle]` export then appears twice.

`cargo metadata` on the umbrella reports **one** `nros-rmw-cffi` package, which
is why this does not look like a dependency-graph problem. It is not one — the
graph is fine; the *identity* is not.

Only **mixed** workspaces fail: the collision needs the umbrella (Rust nodes
present) AND the unconditional plain import. Pure-C/C++ and pure-Rust
configures have one root each.

## Why the plain archive is built at all here

In this workspace the plain `libnros_cpp.a` is on **no executable link line** —
it is produced only so a header rule can consume it
(`nros_cpp_config_generated.h` / `nros_config_generated.h`). The umbrella is
what the 25 real link lines use. `NanoRosRuntimeCrate.cmake:19` states the
intended split: "pure-C / pure-C++ workspaces keep `nros-cpp-headers` pointed at
the plain `nros_cpp-static`" — i.e. the plain import was meant as the *fallback*
for configures with no Rust node, but it is imported unconditionally.

## The framing that settles it: ONE implementation provider

In C there is exactly one site that produces the implementation — an archive,
built once — and every other site is *headers plus a link to that archive*. The
symbol implementation has a single source of truth.

Measured against that, the current wiring is already 90 % right and the last
10 % is the bug:

* `nros-cpp-headers` is an INTERFACE library (aliased `NanoRos::NanoRosCpp`)
  carrying only includes, and `target_link_libraries(nros-cpp-headers INTERFACE
  nros_cpp-static)` names ONE implementation provider. Consumers already do
  "scan the header, link the one archive".
* `nros_synth_runtime_umbrella` SWAPS that provider — it rewrites the INTERFACE
  link from `nros_cpp-static` to `nros_ws_runtime-static`.

The swap is not atomic, in two ways, and its own comment admits the first:

> All other INTERFACE wiring (includes, cyclone, stdc++) is preserved;
> **`nros_cpp-static` stays built but unreferenced.**

1. **The displaced provider keeps building.** It is forced by the header mirror,
   which depends on `$<TARGET_FILE:nros_cpp-static>` — deliberately, because
   issue 0268 showed an order-only dep lets the mirror go stale. Unreferenced is
   not free: it shares the corrosion target dir with the referenced provider,
   which is the duplication above.
2. **Headers and symbols come from DIFFERENT providers.** After the swap the
   interface links the umbrella but still mirrors headers produced by the plain
   crate's `build.rs`. Those headers carry the variant storage sizes
   (`*_OPAQUE_U64S`). If the two builds' feature sets ever diverge, the sizes
   header describes code that was not the code linked — the 0088 → 0114 → 0122 →
   0123 → 0245 → 0268 sizes-mirror class, re-entered through a new door.

So "two providers" is the defect, and separate target dirs (option A below)
would preserve it: two archives would still each contain a full copy of the nros
implementation, still be compiled twice, and any binary that ever linked both
would be back here. **A treats the collision, not the duplication.**

### What the fix has to satisfy

*Exactly one provider per configure, owning BOTH the archive and the generated
headers, chosen atomically.*

That is also a hard constraint rather than a preference: a Rust `staticlib` is a
self-contained bundle, so two of them can never appear in one link — which is
why Option D (phase-241 W11) bundles `nros-cpp` into the umbrella in the first
place. The umbrella IS the single-provider design; it is just not yet exclusive.

### Cargo already models this and the tree does not use it

`links = "nros_cpp"` on the providing package declares "this crate provides the
nros_cpp native implementation", and **Cargo enforces that at most one package
in a graph may claim a given `links` name** — the C model's guarantee,
built in. The provider's build script then publishes its artifacts
(`cargo:include=…`, `cargo:root=…`) and every dependent reads them as
`DEP_NROS_CPP_INCLUDE`. Neither `nros-cpp` nor `nros-c` declares `links`, and no
build script emits that metadata.

Two honest limits on what that buys:

* It would **not** have caught this bug. The check is per-graph, and here there
  are two separate cargo invocations each with a perfectly valid single-provider
  graph. Claiming otherwise would be overselling it.
* What it DOES buy is the missing mechanism for fix B: the umbrella's build
  script could read `DEP_NROS_CPP_INCLUDE` and re-export the provider's headers
  to a stable path, so CMake can mirror headers from whichever provider is
  active. Without it there is no supported way to reach the nros-cpp `OUT_DIR`
  inside the umbrella's build.

The invariant that WOULD have caught it is a CMake-level one: **a configure must
build exactly one Rust staticlib exporting the nros symbol set.** That is
checkable and belongs in a gate, in the spirit of `check-fixture-groups`.

## Fixes considered

**A — give the plain import its own target dir.** Isolates the two roots so the
`deps/` collision cannot happen. **Not the fix.** Under the single-provider rule
it is the wrong shape: two archives would still each carry a full copy of the
nros implementation, still be compiled twice, still feed headers from one while
symbols come from the other. It removes the link error and keeps the defect.
Worth stating because it is the obvious first move and it is a trap.

**B — the umbrella becomes the SOLE provider (recommended).** When
`nros_synth_runtime_umbrella` runs, make the swap total rather than partial:

  1. skip the plain `corrosion_import_crate` for that configure — the code
     comment already says the plain archive is the no-Rust-node fallback, so the
     bug is a missing condition, not a missing design;
  2. mirror the generated headers from the UMBRELLA's build, so headers and
     symbols come from one compilation.

Step 2 is the part with no mechanism today, and it is what makes `links` worth
adding: with `links = "nros_cpp"` + `cargo:include` on the provider, the
synthesised umbrella's build script can read `DEP_NROS_CPP_INCLUDE` and
re-export the headers to a stable path for CMake to mirror. Without that there
is no supported way to reach nros-cpp's `OUT_DIR` from inside the umbrella
build.

**C — stop bundling `nros-cpp` in the umbrella and link the plain archive.**
Rejected. Two Rust staticlibs cannot coexist in one link (each is a
self-contained bundle), which is exactly why Option D bundles it; this
reintroduces the `--allow-multiple-definition` problem that design removed.

**Enforcement, either way.** The invariant is *one Rust staticlib exporting the
nros symbol set per configure*. `links` does not enforce it — that check is
per-graph, and here two independent invocations each have a valid single-provider
graph. A CMake-level gate does: assert a configure builds exactly one such
archive, in the spirit of `check-fixture-groups`. Without it, the next consumer
that imports a root-workspace crate re-creates this silently.

## Connection to the phase-340 identity budget

This is very likely the same mechanism behind the disputed identity reading in
this exact tree. `check-artifact-identity-budget` reads
`examples/workspaces/mixed/build-workspace-fixtures`, and on a long-lived tree
here `nros` measures **12** identities = 2 workspace roots × 2 R3 halves × 3
feature sets, against a ceiling of 6 set by another session on a tree where the
duplication had not occurred. Fixing this should collapse the count — and settle
that disagreement — rather than the ceiling needing to move.

## Not verified

Whether this reproduces in the distrobox lane and in CI. It reproduces here on
every `lane=native` attempt, and it is a property of the CMake graph rather than
of the host toolchain, so a host-specific cause is unlikely — but that is
reasoning, not a measurement.

## Attempted fix 2026-08-10 — REVERTED, and what it taught

I implemented "make the umbrella the sole provider": bound the header mirror to
the active provider with a `$<TARGET_EXISTS:nros_ws_runtime-static>` genex
(evaluated at GENERATE time, so it can see a target created after this file is
configured), had the umbrella copy its own generated headers into the mirror's
source dir, dropped the mirror's hard `cargo-build_nros_cpp` edge, and set
`EXCLUDE_FROM_ALL` on the plain build.

**It did not work, and the reason invalidates part of the diagnosis above.**
Rebuilding from scratch, `examples/workspaces/safety` failed identically, and
`nano_ros/packages/api/nros-cpp/libnros_cpp.a` was still produced.

The second identity is not (only) `nros-cpp`. Two facts I had not connected:

* **`nros-c` is legitimately built from the ROOT workspace in a mixed
  workspace.** `NanoRosRuntimeCrate.cmake`'s own issue-#57 note says mixed
  workspaces "ALSO link the C umbrella (`NanoRos::NanoRos` == `nros_c-static`),
  because the C Node pkgs link it". That is a second Rust staticlib, from the
  root workspace, into the same corrosion target dir — so retiring `nros-cpp`
  removes neither the root-workspace identity nor the duplication.
* The newest failure shows the pair as **relative vs ABSOLUTE** paths —
  `packages/rmw/cffi/src/rust_adapter.rs` against
  `/mnt/wd/…/packages/rmw/cffi/src/rust_adapter.rs` — i.e. workspace member vs
  out-of-workspace path dep, which is the same axis but names the producer more
  precisely than the earlier pair did.

So the shape is worse than "one stray import":

> A mixed workspace NEEDS a root-workspace Rust staticlib (`nros_c-static`, for
> its C nodes) AND a leaf-workspace one (the umbrella, for its Rust nodes).
> Two Rust staticlibs is exactly what the single-runtime model forbids, and
> they share one target dir by construction.

That is a design conflict between phase-241 D3-rev (one staticlib per binary)
and phase-241 W11 Option D (a per-workspace umbrella), which only bites when
BOTH a C node and a Rust node exist in one configure. It is not resolvable by
tidying an import; either the umbrella must subsume the C provider too, or the
two must be built from ONE workspace root.

**Still unexplained and worth measuring before the next attempt:** why the
umbrella's own staticlib bundles BOTH identities when `cargo metadata` reports a
single `nros-rmw-cffi` package in its graph. Until that is answered, any fix is
a guess — mine was.

## Reconciliation with phase-340 (2026-08-10, after `2d1e8d76e`)

Another session reached the SAME phenomenon from the other side. phase-340's
"D is built EIGHT times, with eight distinct identities — zero sharing" census
measures it as **disk**; this issue measures it as a **link failure**. One class,
two vantage points, and they should not be rediscovered separately.

Their three-way split, and how it maps here:

| their axis | here |
| --- | --- |
| two corrosion roots (`nano-ros_…` C++ side vs `nros_ws_runtime_…`) — R2 | the two identities in this issue |
| host vs explicit `--target` — R3 | the ×2 inside each root |
| "two identities per root+arch cell — **unattributed**" | this issue's open question |

**A factual conflict between the two accounts, now settled.** phase-340 says:

> Corrosion keys its dir on `sha1(workspace_manifest_path)`, so they cannot even
> land in the same tree.

That was true of the tree it measured — `cargo/nano-ros_23c15/` and
`cargo/nros_ws_runtime_16b35/` existed side by side, and I saw them too before
rebuilding. It is **not** true of the current tree: after a from-scratch
rebuild there is ONE corrosion dir, `cargo/build`, and it holds BOTH
`libnros_rmw_cffi-*.rlib`.

That difference is the whole issue. The per-workspace keying is what USED to keep
the two roots apart; with it gone they share one `deps/` and the umbrella bundles
both, turning a disk-waste finding into a hard link error. **What removed the
isolation is not identified** — several changes landed the same day
(B3 + wave 2's shared target dirs, phase-344 W2's builder-keyed driver) and I am
not going to name one without measuring it, having guessed wrong more than once
today.

**Vocabulary collision, which will otherwise mis-route the next reader.**
"Umbrella" means two different things in these documents:

* **phase-340 W2.b's umbrella** = one cargo WORKSPACE spanning the example
  leaves (a generated symlink farm). **CLOSED as impossible**: 22/22 leaves
  carry `[workspace]` — that IS the copy-out promise — and cargo hard-errors on
  nested roots.
* **`nros_synth_runtime_umbrella`** = a synthesised staticlib PROVIDER crate in
  the build dir.

The revised design in this issue proposes making the SECOND the single provider.
It does not re-propose the first, and W2.b's refutation does not touch it. Any
reading that bounces this design off "the umbrella is impossible" has conflated
the two.

**Convergent conclusion on the unexplained ×2.** phase-340 says of it:
"the likeliest cause is the two staticlibs (`nros-c`, `nros-cpp`) resolving an
intermediate crate differently. Not yet proven — **do not quote a cause**."
This issue's attempted fix is evidence on exactly that hypothesis, and it is
negative: retiring `nros-cpp` entirely changed nothing, so `nros-cpp` is not the
second producer by itself. That narrows their candidate without settling it.

## Q2 ANSWERED (2026-08-10) — the topology is the CORROSION VERSION, and both measurements were right

The handoff's highest-value question was "which corrosion topology is current:
hashed per-workspace dirs (phase-340, 344 §2.2) or the hashless shared
`cargo/build` measured here — both reproduce on different worktrees". They
reproduce on different worktrees because they are **different corrosion
versions**, reached by the two provisioning paths:

| which corrosion the CONFIGURE resolves | version | target dirs | consequence |
| --- | --- | --- | --- |
| SDK install, when `find_package(Corrosion)` FINDS it | **v0.5.1** (pinned, `nros-sdk-index.toml`) | hashless shared `cargo/build` | two workspace roots write one `deps/` -> duplicate `#[no_mangle]` -> **CANNOT LINK** |
| FetchContent fallback, when it does NOT | **v0.6.1** | hashed `nano-ros_0b88c`, `nros_ws_runtime_14eac` | no collision -> **wasted disk only** |

**The discriminator is `find_package`, NOT whether the SDK is installed** — a
correction to my first version of this table, which said "no SDK corrosion
installed" on the measured tree. That was wrong: v0.5.1 IS installed here
(`~/.nros/sdk/corrosion`, `.installed-version` = v0.5.1). I had listed
`share/corrosion/`, which does not exist — the layout is `share/cmake/` — and
read the empty listing as absence. Recorded because the same wrong probe is
easy to repeat.

What actually happened on this tree: `find_package(Corrosion QUIET)`
(`cmake/nano_ros_workspace_metadata.cmake:118`) did not resolve the SDK install,
so the configure fell through to FetchContent and fetched v0.6.1 —
`_deps/corrosion-subbuild` confirms the tag, and the dirs are hashed. So the two
investigations measured the same repo with different RESOLUTION outcomes, not
different installs.

That sharpens the probe: the question is why `find_package` misses an installed
SDK corrosion (`CMAKE_PREFIX_PATH` / `Corrosion_DIR` not carrying
`~/.nros/sdk/corrosion/share/cmake`), because that resolution — not a
provisioning step someone forgot — is what decides whether a host gets the
linkable topology or the broken one.

**Q2 RESOLVED 2026-08-10 — the discriminator is WHICH BUILDER configures the
tree, not the host and not the install.**

`CMAKE_PREFIX_PATH` wiring for the SDK corrosion exists in exactly ONE of three
builders:

```
scripts/build/compile-check-fixtures.sh    3 references to $HOME/.nros/sdk/corrosion
scripts/build/workspace-fixtures-build.sh  0
scripts/build/fixtures-build.sh            0
```

`compile-check-fixtures.sh:200` prepends the SDK prefix so
`find_package(Corrosion)` resolves it; the other two never do, so their
configures fall through to FetchContent. On ONE host with ONE install:

| builder | corrosion | dirs | consequence |
| --- | --- | --- | --- |
| compile-check | **v0.5.1** (SDK) | hashless shared `cargo/build` | duplicate `#[no_mangle]` -> **cannot link** |
| workspace / fixtures | **v0.6.1** (FetchContent) | hashed per-workspace | **wasted disk only** |

That is why this issue and phase-340/344 reached contradictory measurements
while both were right, and why the forced `-D` probe above failed: the flags
went to a builder that discards them.

**This is the session's recurring class again — one caller wires a rule and its
siblings do not** (the sizes-header mirror chain, #282's guard, #328's resolver,
`fixtures-build.sh`'s `--lang` proxy). Two builders configure cmake for the same
repo and disagree about which corrosion to use.

**The fix is one change, not two.** Route all three builders through a single
prefix helper rather than adding a third copy — and test the 0.5.1 -> 0.6.1 pin
bump in the SAME change, because unifying on v0.5.1 will SURFACE this issue's
link failure on workspace trees that pass today. That is the correct outcome (the
defect is real and currently hidden by an accident of wiring), but it means the
unification must not land without the pin decision beside it. Acceptance is a
rebuild of both a compile-check tree and a workspace tree, not a gate.

**PROBE ATTEMPTED 2026-08-10, INCONCLUSIVE — the v0.5.1 half of the table above
is still INFERENCE, not measurement.** Verified directly: this tree resolves
FetchContent **v0.6.1** and gets hashed dirs with zero duplicate symbols. NOT
verified: that v0.5.1 produces the hashless shared `cargo/build`. That half is
inferred from 0493's report, and the attempt to confirm it here failed —
forcing the SDK install with `-DCMAKE_PREFIX_PATH` / `-DCorrosion_DIR` through
`NROS_CMAKE_EXTRA_DEFS` did not reach the configure (`CMakeCache.txt` shows
`Corrosion_DIR:PATH=Corrosion_DIR-NOTFOUND`, and `_deps/corrosion-subbuild`
fetched v0.6.1 again). So the build ran the SAME arm as before and proved
nothing about the other one.

Whoever picks this up: find the supported way to make the configure resolve the
SDK corrosion — `NROS_CMAKE_EXTRA_DEFS` is evidently not it for this script —
reproduce the hashless topology, and only then test whether the 0.5.1 -> 0.6.1
pin bump removes the link failure. Do NOT treat the version mapping as
established until the v0.5.1 arm has been observed on a tree someone controls.

**This makes the class WORSE than either investigation concluded alone**, and
non-deterministic on top: which topology a host gets depends on whether its
configure resolves the SDK corrosion, and BOTH outcomes occur on trees that were
provisioned the same way. phase-340/344's tree only looked healthy because
`find_package` missed an install that was present — an accident of resolution,
not a property of the code, and not something a user would know to check.

**Consequence for Q1.** "Why does one staticlib bundle both identities when its
graph has one package?" — under a HASHLESS shared dir the two workspace roots
write into one `deps/`, so the archive can pick up rlibs from both roots while
`cargo metadata` still reports one package. The graph is fine and the DIRECTORY
is shared. Test that before hunting a provider bug; it also explains why
retiring `nros-cpp` changed nothing, since the second producer is a second
workspace ROOT rather than a second package.

**Consequence for the fix.** "Separate target dirs is a trap rather than a fix"
still holds for v0.6.1, where they are already separate and the cost is disk. On
v0.5.1 the sharing is not a design choice anyone made — it is the older
corrosion's naming. So the cheapest probe before any provider redesign is
whether the SDK pin moving 0.5.1 -> 0.6.1 removes the link failure outright.
That is a one-line index change plus a rebuild, and it is NOT yet done.

## HANDOFF (2026-08-10) — to whoever owns phase-340 / phase-344

This issue is being handed over rather than continued. Everything below is
either measured or explicitly flagged as not.

### Settled, do not re-derive

* `libnros_ws_runtime.a` bundles two `-C metadata` identities of ten crates
  (`ar t` + `nm`); every `#[no_mangle]` export collides at link.
* The two identities differ by cargo WORKSPACE ROOT, visible in the debuginfo
  paths (repo-root-relative vs crate-relative/absolute).
* `cargo metadata` on the umbrella reports **one** `nros-rmw-cffi` package. The
  dependency graph is fine; the identity is not.
* Reproduces on a pristine checkout: 7 duplicate-symbol errors, 2 cffi rlibs in
  one `deps/`.
* `nros-cpp` is **not** the second producer on its own — retiring it entirely
  (mirror rebound to the active provider, `EXCLUDE_FROM_ALL` on the displaced
  build) left the duplication unchanged. That attempt is reverted; the tree is
  clean.

### The three open questions, cheapest first

1. **Why does one staticlib bundle both identities** when its own graph has one
   package? Until this is answered any fix is a guess — mine was. Suggested
   probe: `cargo build -v` on the umbrella and read the `--extern` / `-L
   dependency=` set actually passed to the staticlib's rustc.
2. **Which tree state is current** — hashed per-workspace corrosion dirs
   (phase-340 / phase-344 §2.2) or a hashless shared `cargo/build` (measured
   here)? Both reproduce, on different worktrees. A bisect over the same-day
   changes (B3 + wave 2 shared target dirs, phase-344 W2's builder-keyed driver)
   settles it. **This is the one that decides whether the class is "wasted disk"
   or "cannot link".**
3. **Does the provider design below survive contact** with Zephyr / NuttX /
   board trees, which have their own providers (`nros-nuttx-ffi` et al.)?

### The proposed design, in one paragraph

One implementation provider per configure, owning BOTH the archive and the
generated headers, from ONE workspace root: a synthesised runtime crate
bundling the C ABI, the C++ FFI, the selected backend and every workspace Rust
node, with `NanoRos::NanoRos` / `NanoRos::NanoRosCpp` reduced to pure INTERFACE
targets. Rationale, the C-model framing it came from, and why separate target
dirs is a TRAP rather than a fix, are in "The framing that settles it" above.
Enforcement should be a CMake gate — exactly one Rust staticlib per configure
exporting the nros symbol set. `links = "nros_c"` is worth adding for the header
publication (`cargo:include` → `DEP_NROS_C_INCLUDE`) but would NOT have caught
this: its check is per-graph.

### Environment notes for reproducing on a non-Ubuntu host

Getting `lane=native` this far needed four fixes/provisions, all landed except
where noted: issue 0492 (`ENABLE_LTO=OFF` for the CMake-self-provisioned
CycloneDDS — lld cannot read GCC LTO IR), `nros setup --source rosidl` (the
no-ROS IDL fallback), `nros setup --tool cyclonedds`, and issues 0486/0487/0489
for the ESP32 lane. `lane=native` still does not complete: it now fails at this
issue.

### Pitfall that cost three wrong attributions

`workspace-fixtures-build.sh` runs groups under `make -j`, so group banners and
compiler output interleave — "the last `== workspace group: … ==` before the
error" is NOT the failing group. Attribute by the failing target path.

## UNIFICATION LANDED 2026-08-10 — one resolution, and it now says which

The second half of Q2's fix (route all three builders through one prefix
derivation) is done, and rebuilding BOTH arms passes. What follows is measured
on this host unless flagged.

### The reason `find_package` missed an install that was present

Q2 left this as "why does `find_package` miss an installed SDK corrosion". It
is not the host and not a forgotten provisioning step — **the two provisioning
paths write two different LAYOUTS, and the root CMakeLists searched only one:**

```
just workspace install-corrosion   ->  $NROS_HOME/sdk/corrosion/            (FLAT)
nros setup --tool corrosion        ->  $NROS_HOME/sdk/corrosion/<version>/  (VERSIONED)
```

The issue-0390 block globbed `${store}/corrosion/*`, which is the VERSIONED
layout. Under the FLAT one that glob yields `lib/` and `share/` — two prefixes
`find_package` cannot resolve from. Measured directly (host `.installed-version`
= v0.6.1):

| CMAKE_PREFIX_PATH | find_package(Corrosion) |
| --- | --- |
| `$HOME/.nros/sdk/corrosion` | **FOUND** (`lib/cmake/Corrosion`) |
| `$HOME/.nros/sdk/corrosion/lib` | NOT FOUND |
| `$HOME/.nros/sdk/corrosion/share` | NOT FOUND |
| `$HOME/.nros/sdk` | FOUND |

`compile-check-fixtures.sh` passed the FLAT parent explicitly, which is why that
one builder resolved the SDK while the same host's other builders did not. So
the "non-deterministic per host" reading in Q2 is sharper than that: it was
deterministic per BUILDER, and the discriminator inside cmake was an unsupported
directory layout.

### What landed

* `cmake/NanoRosCorrosion.cmake` — `nros_resolve_corrosion()`. Store lookup
  (both layouts; each candidate kept only if a `CorrosionConfig.cmake` actually
  sits under it), then `find_package`, then FetchContent at the tag read from
  `nros-sdk-index.toml` rather than a third copy of the pin. The root
  `CMakeLists.txt` and `nano_ros_workspace_metadata.cmake` — the two cmake sites
  that used to answer this separately — both call it. A user's own `cmake -S`
  reaches it too, which no shell helper can.
* It REPORTS, which is the point of the whole exercise:
  `-- nano-ros: Corrosion 0.6.1 via SDK store [hashed per-workspace cargo dirs] — …`
  and `< 0.6.0` prints `[hashless shared cargo/build — issue 0493 link risk]`.
  Both investigations spent days disagreeing about a fact either could have read
  off a configure line, because no such line existed.
* `scripts/build/cmake-prefix.sh` — the shell sibling, exported at file scope,
  sourced by `cmake-incremental.sh` (covering workspace-fixtures-build.sh,
  fixtures-build.sh, phase226-cxx-eff.sh, just/native.just), `fixture-matrix.sh`
  (it carries its own `cmake -S`) and `compile-check-fixtures.sh`, whose inline
  copy is deleted.
* `check-cmake-corrosion-prefix` (check-fast) — a cmake CONFIGURE must sit in a
  file/recipe that sources the helper, or carry
  `# nros-cmake-prefix-exempt: <reason>`. Nine exemptions, each a no-Rust tree, a
  third-party build, corrosion's own build, a copy-out template, or the gate's
  own synthetic project.

### Acceptance — both arms rebuilt

| arm | command | corrosion | cargo dirs | duplicate symbols |
| --- | --- | --- | --- | --- |
| compile-check | `NROS_FIXTURE_ID=cmake_add_subdir compile-check-fixtures.sh` | 0.6.1 SDK | `cargo/<ws>_8058c` | 0 |
| workspace | `workspace-fixtures-build.sh linux mixed --id workspace-mixed-native` | 0.6.1 SDK | `cargo/<ws>_8058c` + `cargo/nros_ws_runtime_e7af4` | 0 |

Both exit 0. The workspace arm is `examples/workspaces/mixed` — the exact tree
this issue dies on — and it now links. Its build dir has **no `_deps/` at all**:
the configure stopped fetching Corrosion over the network. Two hashed dirs, one
per cargo workspace ROOT, is the isolation that makes the duplicate
`#[no_mangle]` impossible, and it is now reached through the SUPPORTED path
rather than through a `find_package` miss.

### Two bugs the rebuild caught that gates did not

Both were mine, both in `nano_ros_workspace_metadata.cmake`, and neither would
have been visible without configuring a real workspace:

1. `CMAKE_CURRENT_LIST_DIR` inside a FUNCTION body names the CALLER's list dir
   — the include resolved to `examples/workspaces/mixed/NanoRosCorrosion.cmake`
   and the configure died. `CMAKE_CURRENT_FUNCTION_LIST_DIR` exists for exactly
   this. Sibling of the CLAUDE.md function-scope pitfall.
2. `Corrosion_VERSION` is a normal variable, so a workspace leaf (a SIBLING
   scope reaching nano-ros via `add_subdirectory`) reported
   `Corrosion unknown … [topology unknown]`. The resolution is now remembered in
   CACHE INTERNAL vars where any scope can read it.

"Acceptance is a rebuild of both arms, not a gate" was right.

### What this does NOT settle

* **The v0.5.1 arm WAS observed — 2026-08-11, correcting this bullet.** Before
  the bump, this host had ONLY `~/.nros/sdk/corrosion/0.5.1-nros1` installed,
  and `examples/workspaces/mixed/build-workspace-fixtures` showed one HASHLESS
  `cargo/build` holding TWO `libnros_rmw_cffi-*.rlib`, with 7 duplicate-symbol
  errors on a from-scratch build. After `nros setup --tool corrosion` (0.6.1) and
  removing the 16 stale workspace build dirs, the same tree showed two HASHED
  dirs (`nano-ros_23c15`, `nros_ws_runtime_16b35`) with ONE identity each and
  RC=0, zero duplicate symbols. That is a controlled before/after across the
  version change on one tree, so the "v0.5.1 → hashless → cannot link" half is
  measured rather than inferred. See "VERIFIED END-TO-END" below.

* **The original bullet, kept because its caution was right at the time:** The pin is now v0.6.1 on both the
  SDK and the FetchContent side, so unifying could not surface the hashless
  topology here — which is the intended outcome, but it means the
  "v0.5.1 -> hashless -> cannot link" half of the table above remains inference,
  exactly as the earlier PROBE ATTEMPTED note said. What IS measured is that the
  supported path and the fallback now resolve the SAME version, so the two
  builders can no longer disagree.
* **Q1 is untouched.** "Why does one staticlib bundle both identities when its
  graph has one package?" — under v0.6.1 the two roots have separate `deps/`, so
  the question does not arise here; it would return on any tree pinned below
  0.6.0. The single-provider design in "The framing that settles it" is
  independent of this change and still unbuilt.
* Only two fixture rows were rebuilt, not `lane=native`. Tier 2 (`just
  ci-matrix`, earned by a `cmake/` diff) was not run — it needs
  `build-test-fixtures lane=tier2` and the disk here is at 96 %.

## Why the cheap fix does not exist (phase-340 item 5, measured 2026-08-10)

phase-340 hoped to defuse this class without the provider redesign, by making
the two roots' cargo `path` field agree ("repo-root-relative vs absolute"). It
does not exist. `path` is not a spelling a caller chooses: cargo spells a unit's
source relative to the workspace root when the package is INSIDE it and absolute
otherwise, so the field records the RELATION this issue already names — root A
builds the shared crates as workspace MEMBERS, the umbrella reaches them as
out-of-workspace PATH DEPS. Measured: an absolute and a relative `path =` dep
line produce the SAME `-C metadata`; only member-vs-path-dep moves it. And cargo
forbids both ways out — a build-dir workspace cannot adopt an in-repo crate
("member of the wrong workspace", and with the root excluding it, "not
hierarchically below the workspace root"), while the umbrella cannot join the
repo-root workspace (an out-of-tree `CMAKE_BINARY_DIR` is not below it, and its
lock names the user's node packages).

**So the two roots cannot share an identity while both exist as they are**, and
this issue's single-provider design — or moving `nros-c`/`nros-cpp` out of the
repo-root workspace — is the whole remaining space. Evidence, including the
three-arm reproduction, is in phase-340 "R2 re-measured — `path` is a RELATION,
not a spelling". Nothing here changes this issue's diagnosis; it removes an
option someone would otherwise try first.

## VERIFIED END-TO-END 2026-08-10 — the bump fixes it on the tree that failed

Provisioned the bumped pin and cleared the stale trees, per the fix this issue
landed:

```
nros setup --tool corrosion          # -> ~/.nros/sdk/corrosion/0.6.1-nros1
rm -rf examples/workspaces/*/build-workspace-fixtures*   # 16 dirs
bash scripts/build/workspace-fixtures-build.sh linux
```

Result: **RC=0, and `grep -c "duplicate symbol"` = 0** — the build that had
failed on every attempt since this issue was opened now passes.

The mechanism is confirmed directly, not inferred from the exit code. The mixed
workspace's corrosion tree is now:

```
cargo/nano-ros_23c15/          1 libnros_rmw_cffi-*.rlib
cargo/nros_ws_runtime_16b35/   1 libnros_rmw_cffi-*.rlib
```

Two hashed per-workspace dirs — the same names phase-340's census reported —
with **one** identity in each, where before there was one hashless `cargo/build`
holding **two**. That is exactly the v0.5.1 → v0.6.1 difference this issue
identified, observed on the failing tree rather than reasoned about.

Note for anyone reproducing: provisioning alone is not enough on a tree that has
already configured. The stale build dirs carry the old topology in their
`CMakeCache.txt`, so they must be removed — which is also why the two
investigations disagreed for as long as they did.
