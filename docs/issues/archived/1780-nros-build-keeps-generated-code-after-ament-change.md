---
id: 1780
title: "`nros build` after an ament change (e.g. `apt install --only-upgrade
  'ros-humble-*'`) builds the OLD generated message code — no regeneration, a
  byte-identical binary, no diagnostic; only an explicit `nros sync` notices"
status: resolved
type: bug
area: cli, codegen, build
severity: high
found: 2026-10-10
related: [rfc-0104, phase-485, 1018, 1781]
---
## Measured

phase-485 M2, `examples/native/rust/talker` (cargo road), CLI built from
`origin/main` at the time. A scratch ament prefix carries a copy of the host's
`std_msgs` with `package.xml` 4.9.2 → 4.9.3 and a field added to `String.msg`
(`int32 m2_extra`); its `resource_index` entries are copied from the host so it
is a complete ament package. The prefix is appended LAST on
`AMENT_PREFIX_PATH` (first is shadowed by issue 1781).

| step | `generated/std_msgs` | field | binary |
| --- | --- | --- | --- |
| A — baseline `nros build` | `b68de0fc…` | no | `21d57619…` |
| B — ament changed, `nros build` | `b68de0fc…` | **no** | `21d57619…` **unchanged** |
| C — `nros sync`, then `nros build` | `2ea56b3e…` | yes | `4f7c81ec…` |
| D — ament restored, `nros sync` + `nros build` | `b68de0fc…` | no | rebuilt |

So `nros build` neither re-runs codegen nor detects that the inputs moved: the
image is built from generated code for an interface the host no longer has.
For a real ROS upgrade that changes a message, the result talks a different
wire layout from its ROS peers, and nothing says so.

## Why

The generated tree is the output of `nros sync`, and nothing records which
ament inputs it was generated FROM, so nothing can tell it is stale. Issue
1018's class (a configure-time emitter with no freshness edge) with the ROS
install as the input that moved.

## Fix (RFC-0104 D5, phase-485 W5)

Record `[generated.<pkg>] ament` — a hash of the interface files and
`package.xml` the package was generated from, plus the codegen version — and
have `nros build` re-run generation for a package whose hash moved (or refuse,
naming `nros sync`, if regeneration cannot be done in-build). Not yet measured
on the cmake and west roads; acceptance is this table's row B regenerating on
all three.

## Resolved (2026-10-11)

**One record, one hash, three roads.** `rosidl_codegen::ament_stamp` is the
only spelling of "what was this generated FROM": a SHA-256 over `package.xml`
and every file under `msg/`, `srv/`, `action/` — keyed by path RELATIVE to the
package, so a byte-identical copy in another prefix is the same input — taken
over the package's `<depend>` CLOSURE (a nested type's layout and size bound are
baked into its dependents' output), plus the emitted codegen version. It is
written beside every generated package as `nros-generated.toml`,
`[generated.<pkg>] ament = "sha256:…", codegen = N` — RFC-0104 D4's section
shape, so W3's project lock can copy it.

- **cargo** — `nros sync` writes the record into each `generated/<pkg>/` it
  emits. `nros build` (`cmd::ws::refresh_generated`, before planning) recomputes
  it for every generated MESSAGE crate (`[package.metadata.nros] ament_version`
  marks one) and regenerates the moved ones through the same per-package codegen
  sync uses, then sync's dependency walk, so a dependency an upgraded
  `package.xml` newly names is generated too. It prints what moved
  (`std_msgs's ament inputs moved since it was generated (recorded 47b747c771a5,
  now a627f57ef72b)`). A crate whose package this host no longer resolves is
  WARNED about, not refused: sync neither regenerates nor deletes it, so naming
  `nros sync` would be naming a fix that does nothing. `--dry-run` reports and
  writes nothing.
- **cmake and west** — `nros codegen --args-file` (and the `package.xml` front
  doors `nros generate-c`/`-cpp`, through the same helpers) writes the same record beside
  the C/C++ output. Both generators (`NanoRosGenerateInterfaces.cmake`,
  `zephyr/cmake/nros_generate_interfaces.cmake`) call ONE helper,
  `nros_codegen_ament_stale()`, which runs `nros codegen --ament-witness` at
  configure: the CLI writes the CURRENT stamp to `<args>.ament.toml` and the
  helper compares the two files' CONTENT — no timestamp is trusted. A mismatch
  re-emits at configure (both lanes already had a configure-time codegen arm).
  The witness is deliberately NOT a generic mtime input or a build-time
  `DEPENDS`: codegen writes only changed files, so it would stay newer than
  every untouched output and re-run codegen on every configure after the first
  move.

### Measured

Scratch ament prefix `tmp/1780-ament` = the host's `std_msgs` (4.9.2 → 4.9.3,
`int32 m2_extra` appended to `String.msg`; `resource_index` entries copied),
PREPENDED on `AMENT_PREFIX_PATH` (1781 made first-wins correct). "In place"
rows edit the overlay's `.msg` and then `touch -d 2020-01-01` it — the shape
`dpkg` leaves, an mtime from inside the `.deb`. Hashes are 12-hex prefixes; the
generated-tree hash excludes the record itself. **No `nros sync` between rows.**

**cargo** — `examples/native/rust/talker`, `nros build` per row:

| step | `generated/std_msgs` | field | binary | build said |
| --- | --- | --- | --- | --- |
| A — host install (after one `nros sync`) | `1f46e103415b` | no | `c60293af3591` | — |
| B — overlay first | `24ef98ed95e9` | **yes** | `3854177e0f19` | moved `47b7…`→`a627…`, regenerated |
| B′ — same env, again | unchanged | yes | `3854177e0f19` | nothing regenerated, nothing compiled |
| C — overlay removed | `1f46e103415b` | no | `c60293af3591` (= A) | moved `a627…`→`47b7…` |
| E — overlay, field removed IN PLACE, 2020 mtime | `2412629725c7` | no | `c60293af3591` | moved `a627…`→`3a4c…` |

(E's tree differs from A only in `ament_version = "4.9.3"`; the binary is A's.)

**cmake** — `examples/native/c/talker`, `cmake -S -B` + `cmake --build` per row
(Unix Makefiles, what the fixture lane configures with):

| step | before (origin/main) | after |
| --- | --- | --- |
| B — overlay first | field generated (the resolved interface PATHS move, so the args file moves) | field, `00766fe2b176`, "regenerating: … moved `47b7…`→`a627…`" |
| E — field removed in place, 2020 mtime | **field still generated, binary `00766fe2b176` unchanged** | no field, `5a73dfed6d87` (= A), moved `a627…`→`3a4c…` |
| C — overlay removed | — | `5a73dfed6d87`, moved `3a4c…`→`47b7…` |

So the cmake road's row B was already green whenever a configure ran — the
interface files are configure inputs by PATH — and its live defect was the
in-place shape, i.e. a real `apt` upgrade. Under Ninja the old lane happened to
catch E as well; Unix Makefiles did not.

The C++ lane (same helper, `--language cpp`): `examples/native/cpp/talker`,
host install `cpp_talker` `0d23eccc4ddc` → overlay first: "regenerating …
`47b7…`→`a627…`", field in 2 generated files, `b339bd3a9e95`.

**cmake via `nros build`** — `examples/templates/pure-c-workspace` staged under
`tmp/` (its talker publishes `Int32`, so the overlay also gained an `Int32`
field): A `native_entry` `2c540ac360dc` → overlay `0628477e6775` (field in 4
generated files) → both fields removed in place, 2020 mtime: `2c540ac360dc` →
overlay removed: `2c540ac360dc`; each with its "regenerating" line, and a
repeat `nros build` regenerated and compiled nothing.

**west** — `examples/zephyr/c/talker`, `native_sim/native/64`, Zephyr 3.7,
`west build -d … -p auto … -- -DZEPHYR_EXTRA_MODULES=<checkout>` (the command
shape `nros build` execs; west re-runs cmake when given cmake args). Fresh
pristine dir under origin/main's cmake, overlay first, then the field ADDED in
place with a 2020 mtime: **0 field hits, `zephyr.exe` `08091fc79224`
unchanged**. Same dir under this change: "regenerating … `3a4c…`→`a627…`",
field present, `96a969abee26`; field removed in place → `08091fc79224`; added
again → `96a969abee26`; overlay removed → `08091fc79224`.

### Not covered

- A bare `cmake --build` / `ninja` that triggers no reconfigure never asks —
  the same limit as every configure-time input. `nros build`, a fixture's
  `cmake -S -B` and `west build -- <args>` all configure. The Zephyr fixture
  runner's `ninja -C` shortcut (signature unchanged) does not.
- West was measured with `west build`, not through `nros build` of a Zephyr
  workspace image; the configure path is the same generator.
- In the C/C++ lanes the dependency closure resolves through the interface
  index the CLI process sees; a workspace msg package reachable only through a
  cmake-side `NROS_INTERFACE_SEARCH_PATH` is not in a dependent's closure (its
  own record still covers its own files).
- Observed, pre-existing and unchanged: once an args file is rewritten, a
  write-if-changed codegen leaves some outputs older than it forever, so both
  lanes' mtime predicate (`nros_codegen_outputs_stale`) re-runs codegen for that
  package on every later configure (a no-op write). It is why the old Ninja
  and west runs sometimes "caught" an in-place change by accident. Filed as
  issue 1803.

Tests: `rosidl-codegen` `ament_stamp::tests` (location-independence, a field,
a `package.xml` bump, a dependency's change moving its dependent — with an
unresolvable-dependency negative control — unrelated files not moving it,
record round-trip, malformed records); `nros-cli-core`
`cmd::ws::generated_freshness_tests` (a fresh record reports nothing — the
negative control; an old-mtime content change, a `package.xml` bump and a
missing record are reported; an unresolvable crate is listed apart; facade
crates are skipped; `--dry-run` writes nothing); `cargo-nano-ros`
`ament_witness_tests` (witness and record byte-identical until the inputs move,
then a named "moved" line, then equal again after regeneration).
