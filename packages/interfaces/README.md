# `packages/interfaces/` — the core pre-generated message set

Committed ROS 2 message bindings that **core crates need before any codegen
runs**. Everything under `generated/` here is emitted by `nros generate-rust`
from an ament install, and is the one place in the tree where a `generated/`
tree is tracked (CLAUDE.md's named exception to "never commit `generated/`";
RFC-0023 is the generator, RFC-0067 the crate identity).

**ONE driver package, ONE output tree, SIX crates** (phase-465, RFC-0067 §D5):

| file | what it is |
| --- | --- |
| `package.xml` | the driver package: four `<depend>` rows naming `rcl_interfaces`, `diagnostic_msgs`, `rosgraph_msgs`, `lifecycle_msgs` |
| `nros-codegen.toml` | RFC-0033 capacities for the whole closure (keys are package-qualified) |
| `generated/humble/` | `nros-builtin-interfaces`, `nros-std-msgs`, `nros-rcl-interfaces`, `nros-diagnostic-msgs`, `nros-rosgraph-msgs`, `nros-lifecycle-msgs` |

Codegen emits the four packages' whole transitive closure — so `std_msgs` and
`builtin_interfaces` arrive on their own — into `generated/<edition>/`, and each
ament package is emitted exactly **once**, because
`resolve_transitive_dependencies` returns a `HashSet` and
`filter_interface_packages` iterates it.

Regenerate with **`just generate-interfaces`**. A ROS 2 ament host is required.
The recipe re-emits the whole tree, drops `geometry_msgs` (see below) and
`cargo fmt`s the six crates, and it is **idempotent**: a second run leaves the
worktree clean.

## Why `nros-` prefixes

**The prefix is load-bearing.** A consumer runs `nros sync` on their own
workspace and gets their own `builtin_interfaces` crate. If nano-ros shipped a
crate with that same ament name, the two would be a hard cargo error — *"package
collision in the lockfile: … only one can be written to lockfile
unambiguously"* — with no workaround, since two `path` packages sharing a `name`
and a `version` cannot both be recorded. So the committed set is renamed into the
`nros-` namespace and stays there, and **rule 5** of the gate below checks it.

`geometry_msgs` is the one crate the closure emits that would ship an unprefixed
name: it arrives via `diagnostic_msgs`' ament deps, no message in the set
references it, and the recipe deletes it. That is one `rm -rf` line standing
between a regeneration and a consumer-facing resolve failure, which is why the
gate reads the tree instead of trusting the recipe.

## Why there used to be three copies of `builtin_interfaces`

Because there used to be **four driver packages and four output trees**.
`rcl_interfaces`, `diagnostic_msgs` and `rosgraph_msgs` each reference
`builtin_interfaces/msg/Time`, so each of the three closures carried its own
copy, and the `-diag` / `-clock` suffixes existed only to stop three crates
named `nros-builtin-interfaces` colliding in one workspace. Their Rust sources
were **byte-identical** and all three declared
`TYPE_NAME = "builtin_interfaces/msg/Time"` — one type on the wire, three in
Rust, with no conversion between them, and `nros-tests` path-depped two at once.

That was tracked as **issue 1428** and is closed by phase-465. Two objections to
collapsing were recorded and are both answered: it needed **no codegen change**
(the emitted rows were already right — it was a driver-package, recipe and layout
change), and it cost **no relocatability** (state the property precisely as *no
generated tree references another generated tree* and one tree preserves it
trivially; the looser reading was never true, since every generated crate already
reaches `nros-core` / `nros-serdes` by relative path).

A **second** edition would be a second tree, and a legitimate duplicate under
§D5. Not in scope here; the tree stays `generated/humble/`.

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
   `[package] name` — the third identity axis, see below (issue 1455);
5. a shipped generated crate's name is in the `nros-` namespace (phase-465 W4).

Rule 3's baseline is **empty**, and that is how the collapse was proved rather
than claimed: it held exactly the six claims of the `builtin_interfaces` triple
(`Time` and `Duration` × three crates), and the ratchet fails a baselined
duplicate that *stops* duplicating, so those six rows had to go in the same
commit. A second copy of any wire type now fails as new — which is how the third
`builtin_interfaces` arrived unnoticed.

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
the crate actually ships under, through the emitter's own `links_key`. The rule
is RFC-0067 **§D4**, gated as rule 4 above, and was decided independently of
phase-465. All six crates declare a `links` now — the regeneration gave the four
that predated phase-403's bounds `build.rs` a channel to carry.
