---
rfc: 0071
title: "RMW backend descriptor: a backend declares itself, core names none"
status: Draft
since: 2026-08
last-reviewed: 2026-08
implements-tracked-by: [phase-347, phase-348]
supersedes: []
superseded-by: null
---

# RFC-0071 — RMW backend descriptor: a backend declares itself, core names none

## Summary

A backend package ships an `nros-rmw.toml` descriptor declaring its build facts
and its capabilities. The toolchain lowers that descriptor the way it already
lowers a board's `nros-board.toml` (RFC-0042 D2). Core packages stop naming
backends: the cfgs they already gate on are flipped by **declared capabilities**
instead of by **detected backend features**, and the closed `resolve_rmw` match
plus its generated CMake `if/elseif` chain are replaced by a read of the
descriptor.

The runtime seam does not change. `nros_rmw_vtable_t` (RFC-0035) already carries
C, C++, Rust and mixed backends and is not the gap.

## Motivation

### "CycloneDDS is the exception" is wrong twice

The exception is real but it is neither singular nor for the reason usually
given. RFC-0031 says cyclonedds "is not pure-cargo linkable" — the *opposite* of
being cargo-linkable — and the tree shows it is not alone:

| backend | implementation | Cargo.toml | CMakeLists.txt |
| --- | --- | --- | --- |
| zenoh | Rust (+ zenoh-pico C via `build.rs`) | yes | no |
| **xrce** | **C** (`publisher.c`, `service.c`) | no | yes |
| **uorb** | **C++** (`publisher.cpp`) | no | yes |
| cyclonedds | mixed (`descriptors.cpp`, `bridge.rs`) | yes | yes |

Three of four are already not pure-cargo. What makes cyclonedds *feel* singular
is not its language but that it is the only one demanding **per-message-type
work** — and that demand is what leaked into core.

### Core names backends today

Two sites, both in `packages/core/nros-node/build.rs`:

```rust
let has_rmw = env::var("CARGO_FEATURE_RMW_ZENOH").is_ok()
    || env::var("CARGO_FEATURE_RMW_XRCE").is_ok()
    || env::var("CARGO_FEATURE_RMW_CFFI").is_ok()
    || env::var("CARGO_FEATURE_RMW_UORB").is_ok();
…
if env::var("CARGO_FEATURE___CYCLONEDDS_LINK").is_ok() {
    println!("cargo:rustc-cfg=rmw_needs_type_descriptors");
}
```

**The capability is already generic; only the trigger is backend-named.**
`rmw_type_registry.rs` gates on `rmw_needs_type_descriptors` and exposes
`MessageForRmw` — a capability name and a generic seam. Core is not asking "is
this Cyclone?" in its logic; it is asking it only in its *detection*. Phase-248
C2 got the seam right and left the trigger behind.

Elsewhere the naming is load-bearing rather than vestigial:

* `packages/api/nros-c/Cargo.toml` declares `rmw-zenoh` / `rmw-xrce` /
  `rmw-cyclonedds`, with `dep:` on concrete backends — and the asymmetry IS the
  special case: `rmw-cyclonedds = ["rmw-cffi"]` carries no `dep:`, because
  Cyclone arrives through CMake. A user-facing library encodes one backend's
  build route in its feature table.
* `packages/cli/cargo-nano-ros/src/rmw_resolver.rs` holds a closed `KNOWN_RMW`
  list, an `UnknownRmw` error, and a `canonical_rmw` alias `match`.
* `cmake/NanoRosRmwDispatch.cmake` is generated from that match into an
  `if/elseif` chain ending in `FATAL_ERROR: unknown rmw`.
* `cmake/NanoRosGenerateInterfaces.cmake` mentions cyclonedds **27 times** —
  more than any other file — for the per-message IDL descriptor step.

### The closed list has already failed

`nros_rmw_dispatch` reports "known: zenoh xrce cyclonedds". **`uorb` is absent**,
though it is an in-tree backend. A closed enum has already stopped covering the
tree it governs, which is the strongest argument that the list should not be a
list.

## Design

### The precedent to copy: boards

Boards solved this problem already. `nros-board.toml` declares
`[board.capabilities]` and `capability_features`, and
`cmake/NanoRosCapabilities.cmake` lowers them into `NROS_PLATFORM_HAS_*`
defines, "so the cmake-driven fixture builds derive them from board.toml instead
of hand-setting them per overlay (the issue-0038 footgun)".

This RFC is that mechanism applied to the RMW axis. It invents nothing.

### D1 — `nros-rmw.toml`, shipped by the backend

```toml
[rmw]
names = ["cyclonedds", "rmw-cyclonedds", "rmw-cyclonedds-cffi"]  # alias table, was canonical_rmw()
cffi_feature = "rmw-cyclonedds-cffi"

[rmw.build]
driver          = "cmake"          # cargo | cmake | both
rlib_dep        = ""               # bundled in the umbrella, or absent
extra_link_libs = ["nros_rmw_cyclonedds", "ddsc", "stdc++"]
needs_cxx_linker = true

[rmw.capabilities]
type_descriptors = true            # -> cfg(rmw_needs_type_descriptors)

[rmw.codegen]
per_message = "nros_rmw_cyclonedds_generate_from_msg"   # see D4
```

`[rmw.build]` is exactly the four fields `nros_rmw_dispatch` already computes —
relocated from a central match to the package they describe. The zenoh
descriptor is the same file with `driver = "cargo"`, no extra libs, and no
capabilities.

### D2 — core receives capabilities, never backend names

`nros-node` gains two capability features, and `build.rs` stops enumerating
backends:

| today | after |
| --- | --- |
| `CARGO_FEATURE_RMW_{ZENOH,XRCE,CFFI,UORB}` → `has_rmw` | `rmw-present` → `has_rmw` |
| `CARGO_FEATURE___CYCLONEDDS_LINK` → `rmw_needs_type_descriptors` | `needs-type-descriptors` → `rmw_needs_type_descriptors` |

`__cyclonedds-link` is deleted. Nothing else in core moves, because the seam it
feeds (`MessageForRmw`, `register_type_descriptor`) is already generic.

### D3 — the selection facade carries the lowering

The translation point already exists and is already generated. `nros sync`
writes `<entry>_nros_selection` (RFC-0031's lowering made concrete), which today
carries the board's `rmw-<x>` feature. It gains the capability features read
from the descriptor:

```toml
[dependencies]
nros = { path = "…", default-features = false, features = ["ros-humble"] }
nros-node = { path = "…", features = ["rmw-present", "needs-type-descriptors"] }
nros-board-linux = { path = "…", features = ["rmw-cyclonedds"] }
```

No user manifest changes; cargo unifies the features onto the packages the entry
already depends on. A third-party backend needs no core edit — its descriptor
supplies the same rows.

### D4 — the hard half: per-message codegen

`[rmw.codegen].per_message` names a CMake function the backend's own
`CMakeLists.txt` defines. `nros_generate_interfaces()` replaces

```cmake
if(NANO_ROS_RMW STREQUAL "cyclonedds"
   AND COMMAND nros_rmw_cyclonedds_generate_from_msg)
```

with a call through the descriptor: if the active backend declares
`per_message`, invoke that command per message type; otherwise do nothing. The
`COMMAND` guard is retained — a backend that declares a hook it did not define
is a configure-time error naming the backend, not a silent skip.

This is the part that is a **hook, not a data field**, and it is the piece with
genuine design risk: it makes the codegen pipeline call backend-supplied code.
The mitigation is that the call site is one, the contract is one function
signature, and a backend that declares nothing pays nothing.

### D5 — resolution is BY NAME over a SEARCH PATH OF WORKSPACES (revised twice)

The first draft used a central registry. The second replaced it with name-based
resolution plus a search path. This third revision fixes what the search path
*is*, and the answer generalises past RMW.

**Why not colcon.** nano-ros does not adopt colcon, for a reason that also
explains the shape of what replaces it: colcon's discovery artifact — the ament
index reached by sourcing `setup.sh` — **exists only after an install step**.
Our build products are per-target static objects for RTOS targets that generally
have no dynamic linking, so there is no install-and-source stage for an index to
appear in. An install-time index is not merely inconvenient here; it has nowhere
to live.

**Therefore discovery is SOURCE-TIME.** We scan source trees for `package.xml`
rather than consulting a built index. That is the one real divergence from
colcon, and it is forced by the target, not chosen.

**The convention: an ordered list of workspace roots.** There is one concept, not
a special case for nano-ros plus a special case for the user:

```
search path = [ <nano-ros root>, <user workspace> ]      # the default
```

Each root is scanned for `package.xml`; a package that announces itself is a
provider. **The nano-ros tree is simply the first entry** — its `packages/rmw/*`
are not builtins reached by a different path, they are providers found the same
way a user's are. Resolution takes the first match, so a workspace package
shadows a nano-ros one of the same name (colcon's overlay-beats-underlay rule).

Only two roots are accepted, and both live in the user's repo:

| accepted | rejected |
| --- | --- |
| the nano-ros source tree (vendored / `add_subdirectory`'d) | an installed index under `~/.nros` |
| the user's workspace | an env var such as `NROS_RMW_PATH` |

Rejecting machine state is what makes a build reproducible from the checkout
alone, and keeps CI from diverging from a developer's box because someone
installed a vendor backend. Dropping the installed case also costs nothing
today: `packages/api/nros-cpp/CMakeLists.txt` contains no `install()`, and the
`NanoRosCppTargets.cmake` its comments reference **is not in the tree** — the
installed-consumer path is historical, so this declines to build a capability
rather than removing one.

**Both roots are already discoverable.** `_nros_find_root()` in
`NanoRosWorkspace.cmake` walks up for `nros-sdk-index.toml`, "the sentinel that
marks every nano-ros checkout root", with an explicit-arg / `-D` / env /
auto-walk chain. The scan reuses it; the user workspace is where `system.toml`
and `src/` are. The list exists so a third root can be ADDED later without a new
mechanism — not so the two defaults have to be configured.

**Consequences:**

* `KNOWN_RMW`, `canonical_rmw`'s `match` and the generated `if/elseif` chain
  disappear rather than move; the alias table lives in each descriptor's `names`.
* "Unknown rmw" becomes "no package provides rmw `x`; searched: `<ws>/src`,
  `<nano-ros>/packages` " — an error about the search, which is actionable. Where
  no nano-ros root is found (a copied-out example), the message must say so
  rather than implying the user deleted something.
* D5 must retire **all three** closed lists (Q3), not only the dispatch.

**This is deliberately not finished now.** Full equality requires nano-ros's own
provider packages to carry `package.xml`, and today none do — the 99 in the tree
are interface packages and test fixtures. The cost, measured:

| axis | dirs needing `package.xml` | with a descriptor today |
| --- | ---: | ---: |
| `packages/rmw` | 8 families | 0 |
| `packages/boards` | 17 | **8** |
| `packages/platform` | 14 | — |

So the descriptor family is roughly half-populated even where it exists. The
migration is therefore incremental and must stay so: **a provider without a
`package.xml` is simply not discoverable by the scan**, and the existing paths
keep working until it has one. Nothing is deleted before its replacement covers
the same set.

### D6 — user-selectable backend capabilities (revised 2026-08-10)

A backend may offer an optional capability the user should be able to turn on —
zenoh-pico's true zero-copy receive being the live case. Today that is
`nros-rmw-zenoh[unstable-zenoh-api]` → `zpico-sys[unstable-zenoh-api]` →
`#define Z_FEATURE_UNSTABLE_API`. The backend owning that feature is correct;
what must not happen is core or the user's manifest learning its name.

**The user selects a capability; the descriptor maps it to the backend's own
feature.**

```toml
# nros-rmw.toml, shipped by the zenoh backend
[rmw.capabilities]
zero-copy-receive = "unstable-zenoh-api"      # capability -> this backend's feature
```

```toml
# system.toml, written by the user
[system]
rmw = "zenoh"
capabilities = ["zero-copy-receive"]
```

`nros sync` resolves the backend, looks the capability up in its descriptor, and
enables the mapped feature in the generated selection facade. Three properties
fall out:

1. **Core never learns the name.** The dead `unstable-zenoh-api` feature on
   `nros-node` is simply deleted (Q2b) — nothing forwards to it and no `cfg`
   reads it.
2. **A capability the backend does not declare is a clear error**, naming what
   the active backend does offer, instead of a feature that silently does
   nothing.
3. **A custom backend offers whatever it likes.** `zero-copy-receive` on a
   third-party RMW maps to that backend's own feature with no core change —
   which is the general form of the question this RFC exists to answer.

Precedent, again from boards: `nros-board.toml` already carries
`capability_features = ["safety-e2e"]`, forwarding a declared capability to its
backend. This is the same mechanism on the RMW axis.

### D7 — finding the package is easy; the build call is a 2×2, and all four cells already exist

"Find the package by name, then call the right build command" splits into two
questions, and only the first is about names.

**Finding it.** Convention plus a search path (D5): `rmw = "foo"` looks for a
package providing rmw `foo` — in-tree under `packages/rmw/*/`, out-of-tree on a
declared path. This is the easy half.

**Calling it.** The build command is not a property of the backend alone. It is a
property of the **(consumer, provider)** pair, because nano-ros has two build
roots — a cargo-rooted build for pure-Rust binaries and a cmake-rooted build for
C/C++ — and a backend may be provided as either. That is a 2×2, and **every cell
is already implemented in-tree**:

| consumer ↓ / provider → | **cargo crate** | **cmake target** |
| --- | --- | --- |
| **cargo build** | path dep in the generated selection facade — *zenoh* | a `-sys` crate whose `build.rs` drives `cmake::Config` — *`cyclonedds-sys/build.rs:55`* |
| **cmake build** | `corrosion_import_crate` — *zenoh from C/C++* | `add_subdirectory` + `target_link_libraries` — *xrce, uorb, cyclonedds* |

So the descriptor does not have to invent an invocation. It declares **what the
backend provides**, and the toolchain picks the adapter for the consumer it is
building:

```toml
# a Rust backend
[rmw.provides.cargo]
crate = "nros-rmw-zenoh"

# a C/C++ backend
[rmw.provides.cmake]
dir    = "."                  # add_subdirectory target
target = "nros_rmw_uorb"      # the linkable target it defines
needs_cxx_linker = true

# a backend that offers both routes (cyclonedds today)
[rmw.provides.cargo]
sys_crate = "cyclonedds-sys"  # bridges cargo -> cmake via the cmake crate
```

**This reframes `driver` from D1.** A backend does not have "a driver"; it has
one or two *provisions*, and the consumer selects. A cargo-rooted binary asking
for a cmake-only backend is then a precise, answerable error — "backend `uorb`
provides only a cmake target; a Rust-rooted build needs a `sys_crate` bridge" —
instead of today's silent routing rule buried in RFC-0031.

**And it explains the cyclonedds exception properly.** Cyclone is not special for
being C++ — xrce is C and uorb is C++. It is special because it is the only
backend that today fills the **cargo-consumer × cmake-provider** cell, which
RFC-0031 handles with a blanket rule ("Cyclone selection always routes through
the CMake/Corrosion build path, even for an otherwise-Rust binary"). Once that
cell is a declared `sys_crate` like any other, the blanket rule is unnecessary.

### D8 — one descriptor family: rmw, platform, board

The same question arises for a user's own **platform** and **board** packages,
in their own languages, and two thirds of the answer already ships:

| axis | descriptor | status |
| --- | --- | --- |
| board | `nros-board.toml` | exists (RFC-0042), has `capability_features` |
| platform | `nros-platform.toml` | exists (RFC-0049), scaffolded by `nros new platform`, has `[capabilities]` with an explicitly **open vocabulary** |
| **rmw** | — | **missing; this RFC** |

RMW being the only axis without a descriptor is exactly why it is the only axis
needing a closed list in CMake. The fix is to finish the family, not to invent a
mechanism.

`nros-platform.toml`'s `[capabilities]` already says "software-stack FACTS (open
vocabulary)", which is precisely the extensibility asked for: a third-party
platform declares whatever facts it has, and policy is checked against them. D6
should adopt the same open-vocabulary rule rather than a fixed capability enum,
so a custom backend can offer a capability nano-ros has never heard of.

**One leak to fix while doing this.** The platform descriptor currently carries
`[knobs.zenoh.tx]` and `[build.zenoh]` — backend-named sections in a
*platform* file. That is the same violation as core naming a backend, one axis
over: a custom RMW cannot receive platform knobs, and a custom platform must
know zenoh's name to supply them. Those sections should key on the resolved
backend (`[knobs.<rmw>.tx]`, `[build.<rmw>]`) so the platform declares
"here are my settings for whichever backend is selected" without naming one.

## What this does NOT change

* **The vtable ABI.** RFC-0035 is not the gap. A backend still populates
  `nros_rmw_vtable_t` and self-registers through `RMW_INIT_ENTRIES`.
* **Requiring a Cargo manifest from every backend** — explicitly rejected.
  `nros-rmw-xrce` (C) and `nros-rmw-uorb` (C++) have no `Cargo.toml` and should
  not gain one; mandating it would force cargo-driven builds and destroy the
  language neutrality the C ABI buys. The descriptor is a plain declarative file,
  in the `nros-sdk-index.toml` / `nros-board.toml` idiom, not any language's
  build manifest.
* **Cyclone's CMake route.** It stays; it stops being *special*. A descriptor
  saying `driver = "cmake"`, `needs_cxx_linker = true` is a backend describing
  itself, not the toolchain knowing its name.

## Verification

The point of the RFC is falsifiable by grep, which is how it should be gated:

* `packages/core/**` and `packages/api/nros{,-c,-cpp}/**` contain no
  `zenoh|xrce|cyclonedds|uorb` outside prose/comments — a `check-rmw-agnostic`
  gate in the spirit of `check-ffi-struct-mirrors`.
* `nros_rmw_dispatch` has no backend name in it.
* Adding a backend touches no file outside its own package plus one descriptor
  discovery path. **The acceptance test is a fifth backend added out-of-tree
  with zero core edits** — until that is demonstrated the RFC is unproven,
  because every closed list in this area looked open until someone tried.

## Open questions — answered 2026-08-10

### Q1 — Discovery: a search path of WORKSPACES, scanned at source time (see D5)

Answered twice and superseded twice. First "copy the board triple" (registry +
descriptor + gate) — rejected, a registry is a list that drifts. Then "name plus
a search path" — right, but vague about what the path contains.

Settled: **an ordered list of workspace ROOTS, defaulting to
`[<nano-ros root>, <user workspace>]`, scanned for `package.xml` at source
time.** Two accepted sources, both in the user's repo; no installed index, no
env var. The nano-ros tree is the first entry rather than a special case.

The rationale that makes this more than a preference: colcon's index is reached
by sourcing `setup.sh` and therefore **exists only after an install step**, and
our per-target static artifacts have no such stage. Source-time scanning is
forced by the target.

Remaining sub-questions, none blocking the RFC:

* **Shadowing.** A workspace provider overlaying a nano-ros one is a legitimate
  workflow (testing a patched backend). Allow it, warn with both paths — silently
  ignoring the user's copy would be worse.
* **Scan cost and invalidation.** Two source trees walked per configure, cached
  into `build/nros/`. The cache key is the fiddly part and is the
  `CONFIGURE_DEPENDS` class this repo already knows how to get wrong.
* **Topological order.** A provider in `src/` may need building before the
  consumer links it, and `nano_ros_workspace(SUBDIRS …)` is an explicit list
  today. Discovery becomes scheduling; this is the piece with real teeth and it
  is unchanged by anything above.
* **`<depend>` is already in `package.xml`** and could supply that order rather
  than a second dependency declaration — worth using, since the file is being
  parsed anyway.

### Q2 — `has_rmw`: three quarters of it is already dead code

**Resolved, and D2 gets smaller.** `nros-node` declares these features:

```
default std alloc scheduler-* rmw-cffi __cyclonedds-link signal-fd-wake
wake-latency-probe rmw-lending posix-serial ros-* log safety-e2e
unstable-zenoh-api stream ffi-sync param-services lifecycle-services
```

There is **no `rmw-zenoh`, `rmw-xrce` or `rmw-uorb` feature**. So in
`build.rs`, three of the four disjuncts can never fire:

```rust
let has_rmw = env::var("CARGO_FEATURE_RMW_ZENOH").is_ok()   // no such feature
    || env::var("CARGO_FEATURE_RMW_XRCE").is_ok()           // no such feature
    || env::var("CARGO_FEATURE_RMW_CFFI").is_ok()           // the only live one
    || env::var("CARGO_FEATURE_RMW_UORB").is_ok();          // no such feature
```

`has_rmw` is already `rmw-cffi` alone — a capability trigger wearing a
four-backend disguise, left over from before the phase-248 cffi convergence.
D2 is therefore a rename plus a deletion of dead disjuncts, not a semantic
change.

The comment above that block also claims `has_rmw` is set "when compiling for
tests (unit tests use MockSession)". **There is no such branch in the code.**
Fix the comment or restore the behaviour, but do not carry the claim forward
unexamined.

### Q2b — `unstable-zenoh-api` in core is DEAD, so this is a deletion

`nros-node` declares `unstable-zenoh-api`, but tree-wide it is referenced by no
other manifest and by no `cfg` — its only mention in core source is a doc
comment in `executor/handles.rs`, and the seam actually described there is
`rmw-lending`, which is already capability-named.

The live chain lives entirely in the backend, where it belongs:
`nros-rmw-zenoh[unstable-zenoh-api]` → `zpico-sys[unstable-zenoh-api]` →
`#define Z_FEATURE_UNSTABLE_API`.

So core's copy is a dead declaration: **delete it**, no mechanism required. The
user-facing question it raises — how a user turns that capability on without
core naming it — is answered by D6.

### Q3 — uorb is NOT special; the three disagreeing lists are the bug

**Resolved: a gap, not a deliberate bypass.** `uorb` is a first-class
`NANO_ROS_RMW` value in two places and fatal in a third:

| site | accepts |
| --- | --- |
| `cmake/NanoRosFeatureSet.cmake` (validator) | zenoh, xrce, cyclonedds, **uorb**, none |
| `packages/api/nros-cpp/CMakeLists.txt` (own `if/elseif`) | zenoh, xrce, cyclonedds, **uorb** |
| `NanoRosRmwDispatch.cmake` (generated from `resolve_rmw`) | zenoh, xrce, cyclonedds — **`FATAL_ERROR` on uorb** |

uorb never reaches the dispatch because the PX4 shell (`integrations/px4/`)
`add_subdirectory`s the backend and links the `nros_rmw_uorb` target directly.
That is how the inconsistency has survived: the path that would fail is the one
uorb does not take.

**uorb is not a special backend and should not be modelled as one.** It behaves
exactly as the others do — it populates the vtable and self-registers. What is
unusual is its *consumer*: PX4 links it through the `integrations/px4/` shell for
direct app interop, rather than through a nano-ros entry. That is a property of
the deployment, not of the backend, and a descriptor describes the backend.

Two consequences. The descriptor set **must** cover uorb, on equal terms. And
D5 must absorb **all three** lists — the first draft replaced only the dispatch,
which would leave two still disagreeing and the RFC's own strongest evidence
unfixed.

### Q4 — sequencing against issue 0493

**Sequence, do not parallelise, and 0493 goes first.** Both touch the
`nros-c` / `nros-cpp` feature tables: 0493's provider work decides which
staticlib owns the nros symbol set, and D2/D3 here rewrite the feature rows that
put backends into that archive. 0493 is also still diagnosing — its central
question ("why does one staticlib bundle two identities?") is unanswered, and
building a descriptor mechanism on top of an unexplained linking model risks
encoding the bug into the declaration format.

The cheap ordering: land 0493's measurement, then Q2/Q2b (pure deletions and a
rename, no new mechanism), then D1/D5, then D4 last as the only genuinely new
machinery.
