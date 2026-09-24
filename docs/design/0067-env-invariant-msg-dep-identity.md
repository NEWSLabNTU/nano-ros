<!--
RFC-0067 — living design doc. Status flow: Draft → Stable → Superseded.
Drafted 2026-08-02 from the issue-0378 study (prototype-validated, see §Evidence).
Hand-off doc: the implementation is phase-333; this RFC is the WHY.
-->

# RFC-0067 — Env-invariant Rust message-dependency identity

**Status:** Draft (2026-08-02; D4/D5 added 2026-09-22 from the issue-1428 study)
**Motivated by:** issue 0378 (leaf message deps resolve against the PUBLIC
crates.io) + the `--locked` reproducibility tension it exposed; extended by
issue 1428 (`builtin_interfaces` generated three times, none canonical) and
issue 1455 (a generated crate's `links` collides with a consumer's own copy).
**Amends / refines:** RFC-0026 §Cargo.lock policy (adds the third leaf class —
in-tree testing/bench leaves that COMMIT their locks), RFC-0048 W9 (the
`nros sync`-managed leaf `[patch.crates-io]`), RFC-0023 (codegen emits the
generated crate). Does not change the ament/`package.xml` SSoT.

## Problem

A generated ROS message crate (`std_msgs`, `builtin_interfaces`, …) is produced
per host by `nros sync` / `nros generate-rust` from the consumer's ament install.
Its **committed identity has two axes that vary with the host's ROS distro**, and
both break something when a leaf commits a `Cargo.lock`:

| axis | value | varies by | breakage |
| --- | --- | --- | --- |
| **version** | the ament package version (`std_msgs` = `4.9.1` jazzy, `5.3.6` rolling, …) | ROS distro | a committed lock pins one distro's version → every other host's `--locked` build fails as drift |
| **source** | a crates.io **registry name** (`std_msgs = { version = "*" }`), rescued by `[patch.crates-io]` | whether the patch is in the loaded config chain | when the patch is NOT loaded, cargo resolves the bare name against PUBLIC crates.io, where third parties own `std_msgs` / `builtin_interfaces` (issue 0378). Fails today only because the published version is YANKED — a yank is not a security control. |

The two axes are coupled and make each other worse:

- The `--locked` cargo shim (`scripts/bin/cargo`, issues 0359/0378) is a
  reproducibility promise for tracked locks. In-tree **testing/bench** leaves
  (`packages/testing/nros-bench/*`, `packages/testing/nros-tests/bins/*`) commit
  their locks — and those locks are observed today pinning `4.9.1`, `4.9.0`,
  `5.3.6`: three different distros. A contributor on any other distro cannot pass
  tier 1.
- The `0.0.0`-constant version already designed for this (RFC-0023 codegen,
  `cargo_nros.toml.jinja`: "deliberately not the ament package version … so an
  ament-derived version isn't baked into a committed lockfile") makes the
  crates.io exposure *worse* if adopted alone: `std_msgs = "0.0.0"` is a REAL
  squatted crate on crates.io, so a `version = "*"` that resolves to `0.0.0`
  against the registry MATCHES the squatter instead of failing on the yank.

So neither axis can be fixed in isolation. RFC-0026 sidesteps this for
`examples/**` by gitignoring their locks; it does not cover the testing/bench
leaves that legitimately want a committed, reproducible lock.

## Decision

**Make a committed message-crate reference env-invariant on BOTH axes, entirely
from the `package.xml` SSoT, so a committed lock is genuinely reproducible and
no message name is ever resolved against crates.io.**

### D1 — Message deps are `path` deps, never registry names

Every reference to a generated message crate — the **leaf's** dependency and the
**inter-message** deps between generated crates — is a `path` dependency:

```toml
# leaf Cargo.toml
std_msgs = { path = "generated/std_msgs", default-features = false }   # was: { version = "*" }
```

Consequences:

- Cargo never consults a registry for a `path` dep → **crates.io is not in the
  message-crate resolution graph, by construction** — independent of cwd, of
  whether any `[patch]` is loaded, and of what a third party publishes. Closes
  issue 0378 structurally: no stub crates to enumerate (only `package.xml` is
  SSoT), no reliance on a name being unclaimed.
- On an unsynced tree the `generated/` target is absent → cargo fails **closed**
  ("failed to load source … path not found"), never falls through to crates.io.
- The `[patch.crates-io]` entries for message crates are **deleted** — a path dep
  needs no patch. (The inter-message deps in generated crates are ALREADY path
  today, e.g. `builtin_interfaces = { path = "../builtin_interfaces" }`; the gap
  is only the leaf's own line.)

Not in scope for D1: `nros-core` / `nros-serdes`, which generated crates still
reach by registry name + patch. Those are nano-ros's OWN crate names (a distinct,
lower-risk exposure); see Open questions.

### D2 — Generated crate version is the constant `0.0.0`; ament version is metadata

The generated crate's `[package].version` is `0.0.0` on every host (already the
`cargo_nros.toml.jinja` behaviour); the real ament version lives in
`[package.metadata.nros] ament_version`, which carries no resolution meaning.

Consequence: a `path`-dep lock entry records the crate's own `version` — now
`0.0.0` regardless of distro → **the committed lock is byte-identical across
distros** → `--locked` holds everywhere. The committed leaf lock becomes a real
promise, not a distro fingerprint.

### D3 — In-tree testing/bench leaves MAY commit a reproducible lock

RFC-0026 gitignores `examples/**/Cargo.lock` because a committed example lock
could not be reproducible. With D1+D2 the message identity is env-invariant, so a
`packages/testing/{nros-bench,nros-tests/bins}/*` leaf that commits its
`generated/` tree CAN commit a reproducible lock. Leaves that do NOT commit
`generated/` keep a path dep pointing at an absent dir → they fail closed until
`nros sync` and therefore cannot commit a lock (nor should they).

### D4 — `links` is the THIRD identity axis, and a rename must move it

D1 and D2 cover the two axes a `path` dep resolves on: the **name** and the
**version**. A generated message crate has a third, and it is also global to the
dependency graph: `links`.

Codegen emits `links = "nros_msgs_<ament_package>"`
(`rosidl-codegen/src/bounds.rs::links_key`) purely as cargo's metadata channel —
no native library is linked; it is what makes the crate's `build.rs` size bounds
reach a dependent as `DEP_NROS_MSGS_<PKG>_BOUNDS_*`. The emitter states the
assumption that makes it safe, at `rosidl-bindgen/src/generator.rs`: *"Cargo
requires `links` to be unique across a dependency graph; a generated crate is
named after its ament package, which already is."*

**That assumption does not survive the `nros-` prefix**, and the consequence
reaches out-of-tree consumers, not just this repo. Three measurements,
2026-09-22:

1. **The prefix does not reach `links`.** `apply_package_renames`
   (`cargo-nano-ros/src/lib.rs`) rewrites `[package] name`, dependency keys,
   `"../<dep>"` sibling paths and `<pkg>/std`. It does not touch `links`. So
   `packages/interfaces/.../nros-builtin-interfaces-clock` ships
   `name = "nros-builtin-interfaces-clock"` with
   `links = "nros_msgs_builtin_interfaces"` — the *ament* value.
2. **A consumer's own copy therefore collides.** A user's `nros sync` emits an
   unrenamed `builtin_interfaces` crate carrying the same `links`. Put both in
   one graph — which `nros/sim-time` → `nros-node/sim-time` →
   `nros-rosgraph-msgs` → `nros-builtin-interfaces-clock` does for any leaf that
   also generates a closure containing `builtin_interfaces` — and cargo refuses
   at resolve time:

   > package `nros-builtin-interfaces-clock` links to the native library
   > `nros_msgs_builtin_interfaces`, but it conflicts with a previous package
   > which links to `nros_msgs_builtin_interfaces` as well

   Resolve-time, so it takes every cargo command in that leaf. Reachable today
   and not yet reached: `sim-time` is off by default and its two in-tree
   consumers deliberately use the committed bindings rather than generating.
   Two shipped crates carry `links` today (`-clock` and `nros-rosgraph-msgs`);
   every further regeneration of the pre-generated set adds one, because a
   current `nros` emits it. Filed as issue 1455.
3. **Dropping the prefix is not the alternative.** Two `path` packages with the
   same `name` + `version` are a hard error even when renamed at the dep site and
   given distinct `links` values:

   > package collision in the lockfile: packages `builtin_interfaces v0.0.0
   > (…/a)` and `builtin_interfaces v0.0.0 (…/b)` are different, but only one can
   > be written to lockfile unambiguously

   So the `nros-` prefix on the committed core set is load-bearing and permanent:
   it is what stops a shipped pre-generated crate colliding by NAME with a
   consumer's own copy of the same ament package.

**Decision:** a crate rename is a rename of the whole identity. `links` follows
the crate name (`nros_msgs_` + the renamed crate's ident), so a renamed crate and
an unrenamed copy of the same ament package coexist. Refusing the rename is not
an option (the prefix is required by (3)) and neither is leaving `links` behind
(it breaks (2)). This is decidable **independently of D5** — (3) is what makes it
so, and it is why issue 1428 was wrong to hold it back pending the collapse.

**Landed** (issue 1455, 2026-09-24). The recomputation lives in
`apply_package_renames`, not in the emitter, because that function is the one
place that knows what a generated crate is finally called — it owns the
directory name, the `[package] name`, every sibling dep key and every `use`
path. Handing the emitter the rename map instead would answer "what does this
crate ship as" in two places. The FORMULA is still single: the value is
recomputed through the emitter's own `BoundInventory::links_key`, never
text-substituted, which also covers the renames whose old name is not spelled
the way the key spells it (`links_key` normalises `-`/`.`/`/` to `_`).

Renaming the channel is free because nothing consumes it by name: no `build.rs`
in the tree reads `DEP_NROS_MSGS_*` (the only `DEP_*` readers are
`nros-rmw-cyclonedds-sys` on `DEP_DDSC_*` and `nros-c` on `DEP_NROS_NODE_*`),
and the cmake side reads the bounds from `nros_message_bounds.json` by PATH
(`nros_message_bounds_files` → `NanoRosGenerateInterfaces.cmake`), never through
cargo's env channel. Gated as rule 4 of `check-message-crate-identity`: a
tracked generated crate whose `links` does not equal `nros_msgs_` + its own
`[package] name`. A crate with NO `links` stays legal — a pre-phase-403 vintage
emits no bounds `build.rs`, so claiming a graph-global name it never writes to
would be strictly worse.

### D5 — Canonicality is a property of the output TREE, not of a shared crate

One wire type should be one Rust crate per **(ros-edition, capacity profile)**.
Two copies are legitimate only when they differ — a different edition's field
set, or a different RFC-0033 capacity config, both of which make genuinely
different Rust types over the same wire type. Byte-identical copies are not.

The core pre-generated set violated this: `builtin_interfaces` existed three
times with byte-identical sources, one per output tree
(`packages/interfaces/{rcl-interfaces,diagnostic-msgs,rosgraph-msgs}/generated/humble/`),
distinguished only by `--rename` suffixes (`-diag`, `-clock`) whose whole job was
to stop three copies colliding in one workspace.

**The fix is not a shared canonical crate — it is one output tree.** Codegen
emits the whole transitive closure of a driver `package.xml` into one directory
and wires siblings by `path = "../<dep>"`; `resolve_transitive_dependencies`
returns a `HashSet` and `filter_interface_packages` iterates it, so **one
invocation emits each ament package exactly once**. Measured 2026-09-22 with one
driver package.xml depending on all four core packages
(`rcl_interfaces`, `diagnostic_msgs`, `rosgraph_msgs`, `lifecycle_msgs`):

```
Generating bindings for 7 interface packages...
  ✓ builtin_interfaces (2 messages, …)   ← once
  ✓ std_msgs (30 messages, …)            ← once
  …
nros-rosgraph-msgs:   nros-builtin-interfaces = { path = "../nros-builtin-interfaces" }
nros-diagnostic-msgs: nros-std-msgs           = { path = "../nros-std-msgs" }
```

Six crates where the four-tree layout has eight, every dep a sibling, every crate
`nros-`prefixed, `links` unique because there is one copy. Sources match the
committed ones modulo `cargo fmt` (import ordering and a trailing newline — the
committed set is formatted after generation).

So the two objections recorded against collapsing both dissolve, because both are
objections to a *shared crate across trees*:

- *"a file move cannot do it; the parents' sibling `path` rows are emitted and the
  next regeneration writes them back"* (issue 1428 §"Is collapsing a file move or
  a codegen change? — CODEGEN"). True given four trees. With one tree the emitted
  rows are already correct, so **no codegen change is required at all**.
- *"a canonical crate makes generated trees non-relocatable — a generated manifest
  would name a path outside its own tree"* (issue 1428 Resolution). The property
  worth keeping is precisely stated as **no generated tree references another
  generated tree**, and one tree preserves it trivially. (The looser reading —
  "no reference outside itself" — was never true: every generated crate already
  reaches `nros-core` / `nros-serdes` in the checkout by relative path, which is
  §Evidence's deliberate design.)

Out-of-tree consumers are unaffected either way: a user's `nros sync` already
emits one copy per ament package for the whole workspace closure (the same
`emitted` dedupe, `nros-cli-core/src/cmd/ws.rs`), so a user whose closure
contains `builtin_interfaces` has exactly one copy before and after. What D5
changes is only how many copies **nano-ros ships**.

Capacity profiles merge cleanly under one tree because RFC-0033 keys are
package-qualified (`"diagnostic_msgs/DiagnosticArray.status"`), and one
invocation builds one `CapacityResolver` for the whole closure. A future package
that needs the *same* ament package at a *different* capacity is the legitimate
duplicate D5 allows — and then the suffix names the profile, not the neighbouring
tree.

Implementation: phase-465. Not a prerequisite for D4.

## Consequences / migration shape

- The leaf-manifest edit (registry→path) is mechanical and enumerable; a gate
  (`check-msg-dep-is-path`, replacing the interim `check-msg-dep-redirect`)
  asserts the invariant so no new leaf reintroduces a registry-named message dep.
- Regenerating the stale committed generated crates (currently `4.9.1`) to
  `0.0.0` needs a ROS 2 ament host (this is the only step that does).
- No codegen change is required for D2 (already emits `0.0.0`); D1 needs the leaf
  emission path (whoever writes `std_msgs = "*"` today) to write a path dep, and
  the interim `[patch.crates-io]` machinery for message crates to be retired.

## Evidence (prototype, 2026-08-02, `int32-sink`)

Hand-converted `packages/testing/nros-tests/bins/int32-sink` to D1 (leaf
`std_msgs` → path, message `[patch]` entries removed) and simulated D2 (generated
crates set to `0.0.0`), on this checkout, no ROS env:

- **Builds** native (`cargo build`, 9.2 s cold / 1.9 s warm).
- **Unification holds** — `cargo tree` shows exactly ONE `std_msgs` and ONE
  `builtin_interfaces`, both the `generated/` path copy. (The open risk was that
  `[patch.crates-io]` had been forcing single-copy unification; path deps unify
  on their own because all referrers canonicalise to the same dir.)
- **Lock is env-invariant** — with the generated crates at `0.0.0`, `Cargo.lock`
  pins `std_msgs 0.0.0` with **no registry source** (a path dep carries no
  `source`/checksum line). Every distro's codegen emits `0.0.0` → identical lock.
- **From the repo ROOT**, `cargo metadata --manifest-path <leaf> --offline`
  resolves `std_msgs` to `path+file://…/generated/std_msgs#0.0.0` — **not
  crates.io** — closing the `--manifest-path`-from-elsewhere hole that issue 0378
  left open and declared unclosable by repo-side config.

Reverted cleanly; no code landed. Implementation is **phase-333**.

## Open questions

- ~~`nros-core` / `nros-serdes` are reached by registry name + patch inside
  generated crates.~~ **ANSWERED 2026-08-03 — folded into D1.** The deciding
  evidence came from phase-333's own acceptance run: after the message half
  landed, a CONFIG-patched leaf still failed from the repo root with `no matching
  package named nros-core`, because `.cargo/config.toml` is discovered from the
  cwd. So the nros crates reproduced the original bug exactly, one crate set
  over.

  Generated manifests now emit `nros-core` / `nros-serdes` / `nros-rmw` /
  `nros-rmw-cyclonedds` as PATH deps. The asymmetry with message crates is that
  these live in the CHECKOUT rather than beside the generated crate, so the
  emitted path is **relative when the generated tree is inside the checkout**
  (host-invariant, safe to commit) and **absolute only for a copy-out project
  outside it** (regenerated per host by that user's own `nros sync`, exactly like
  the central `nros-patch.toml` it replaces). Emitting an absolute path into a
  committed tree would have re-introduced the issue-0375/0391 class this RFC
  removes.

  Result: every package in a converted leaf — the whole transitive nros graph
  plus the message crates — resolves `path+file://…` from the repo root, and no
  unused-patch warnings appear.
- Should EVERY in-tree testing/bench leaf commit its `generated/` tree (so it can
  build + commit a lock without a ROS host), or only those that need offline
  reproducibility? Trade-off: committed `generated/` is edition-pinned content in
  the tree vs. a leaf that only builds after `nros sync`.
- Interaction with multi-edition (`ros-humble`/`jazzy`): a committed `generated/`
  is one edition's field set. Cross-edition leaves already pick an edition per
  build; confirm the path-dep lock does not over-assert edition.

## Non-goals

- Publishing anything to crates.io (nano-ros publishes nothing there).
- Changing the ament / `package.xml` SSoT or the C/C++ (`find_package`) path
  (RFC-0048).
- The setup/system-dependency SSoT (RFC-0062) — orthogonal.
