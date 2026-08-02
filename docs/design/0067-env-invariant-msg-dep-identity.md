<!--
RFC-0067 — living design doc. Status flow: Draft → Stable → Superseded.
Drafted 2026-08-02 from the issue-0378 study (prototype-validated, see §Evidence).
Hand-off doc: the implementation is phase-333; this RFC is the WHY.
-->

# RFC-0067 — Env-invariant Rust message-dependency identity

**Status:** Draft (2026-08-02)
**Motivated by:** issue 0378 (leaf message deps resolve against the PUBLIC
crates.io) + the `--locked` reproducibility tension it exposed.
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
