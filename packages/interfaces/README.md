# `packages/interfaces/` — the core pre-generated message set

Committed ROS 2 message bindings that **core crates need before any codegen
runs**. Everything under a `generated/` directory here is emitted by
`nros generate-rust` from an ament install, and is the one place in the tree
where a `generated/` tree is tracked (CLAUDE.md's named exception to "never
commit `generated/`"; RFC-0023 is the generator, RFC-0067 the crate identity).

| driver package | tracked crates |
| --- | --- |
| `rcl-interfaces/` | `nros-rcl-interfaces`, `nros-builtin-interfaces` |
| `diagnostic-msgs/` | `nros-diagnostic-msgs`, `nros-std-msgs-diag`, `nros-builtin-interfaces-diag` |
| `rosgraph-msgs/` | `nros-rosgraph-msgs`, `nros-builtin-interfaces-clock` |
| `lifecycle-msgs/` | `nros-lifecycle-msgs` |

Each directory is a **driver package**: a `package.xml` naming one ament package
in `<depend>`, plus an optional RFC-0033 `nros-codegen.toml`. Codegen emits that
package's whole transitive closure into `generated/<edition>/`. Regenerate with
`just generate-bindings` (one private recipe per driver package); a ROS 2 ament
host is required, and the result is `cargo fmt`-ed afterwards.

## Why `nros-` prefixes, and why three copies of `builtin_interfaces`

Both follow from one fact: **a generated crate names its dependencies by CRATE
name and reaches them as flat siblings, `path = "../<dep>"`.**

- **The prefix is load-bearing.** A consumer runs `nros sync` on their own
  workspace and gets their own `builtin_interfaces` crate. If nano-ros shipped a
  crate with that same ament name, the two would be a hard cargo error — *"package
  collision in the lockfile: … only one can be written to lockfile
  unambiguously"* — with no workaround, since two `path` packages sharing a
  `name` + `version` cannot both be recorded. So the committed set is renamed
  into the `nros-` namespace and stays there.
- **The three copies are the price of four output trees.** `rcl_interfaces`,
  `diagnostic_msgs` and `rosgraph_msgs` each reference
  `builtin_interfaces/msg/Time`, so each of the three closures contains its own
  copy; the `-diag` / `-clock` suffixes exist only to stop three crates named
  `nros-builtin-interfaces` colliding in one workspace. Their Rust sources are
  **byte-identical** and all three declare
  `TYPE_NAME = "builtin_interfaces/msg/Time"` — one type on the wire, three in
  Rust, with no conversion between them.

This is accepted tech-debt, tracked as **issue 1428**, not a design intent.

## What would make it one

**One output tree instead of four.** Codegen already deduplicates within a single
invocation, so one driver `package.xml` depending on all four core packages emits
each ament package exactly once — six crates instead of eight, every dep still a
flat sibling, `links` unique because there is one copy. Measured; see RFC-0067
**§D5**, which also records why the two objections to collapsing (that it needs a
codegen change, and that it costs generated trees their relocatability) apply to
a *shared crate across trees* and not to this shape.

The remaining cost is migration, not design: the root workspace member list,
~19 consumer dep rows, four regeneration recipes becoming one, the tracked
lockfiles, and the gate baseline below. Planned as **phase-465**. Do not
half-apply it — a shared crate wired for two of three parents is worse than
either end state (the issue-0394 class, which broke a fresh clone twice).

## What is gated

`just check message-crate-identity`
(`scripts/check-message-crate-identity.py`, buildless, on the fast lane) reads
every tracked manifest in every workspace root and every tracked `.rs`:

1. a tracked generated message crate's `version` is the constant `0.0.0` — never
   the release version, because a `path` dep's `version` is still a requirement
   and `^0.5.0` stops resolving the day the workspace bumps (issue 0394);
2. no dep row anywhere pins one of their versions;
3. no wire `TYPE_NAME` is claimed by more than one **shipped** crate, against the
   shrink-only baseline `.config/duplicate-wire-type-baseline.txt`;
4. a generated crate's `links`, when it has one, is `nros_msgs_` + its own
   `[package] name` — the third identity axis, see below (issue 1455).

Rule 3's baseline holds exactly the six claims of the `builtin_interfaces` triple
(`Time` and `Duration` × three crates). A **fourth** copy fails the gate — which
is how the third arrived unnoticed — and a baselined duplicate that stops
duplicating fails as *stale*, so the debt cannot silently go hollow. Collapsing
therefore empties that file in the same commit.

`#[cfg(test)]` claims are excluded from rule 3 on purpose: a hand-written fixture
struct in `mod tests` is never linked into an image, so it cannot collide on the
wire, and counting them would have put two non-problems in the baseline.

## `links` is the third identity axis, and a rename moves it

`links` is global to the dependency graph, exactly like a crate's name and its
version, so it is a third thing two packages can collide on. Codegen derived it
from the **ament** package name and the rename pass did not rewrite it, so
`nros-builtin-interfaces-clock` shipped `links = "nros_msgs_builtin_interfaces"`
and a consumer's own generated `builtin_interfaces` — carrying the same value —
made the graph unresolvable, for every cargo command in that leaf. Reachable via
`nros/sim-time`.

Fixed (**issue 1455**): `apply_package_renames` recomputes the key from the name
the crate actually ships under, through the emitter's own `links_key`, so the
two shipped crates now read `nros_msgs_nros_builtin_interfaces_clock` and
`nros_msgs_nros_rosgraph_msgs`. The rule is RFC-0067 **§D4**, gated as rule 4
above, and decided independently of phase-465. The other six shipped crates
declare no `links` at all (they predate phase-403's bounds `build.rs`, so there
is no channel to carry); a regeneration gives them one, correctly derived.
