# nano-ros Issues

This directory tracks nano-ros bugs, limitations, and tech-debt — one file
per issue, mirroring the repo's numbered-RFC convention
(`docs/design/NNNN-*.md`) and the roadmap `archived/` pattern. Each file
carries YAML frontmatter plus the issue body (problem, evidence, current
state, fix/direction). Open issues live directly in `docs/issues/`; resolved
ones move to `docs/issues/archived/`.

## Conventions

**Frontmatter schema** (every issue file):

```yaml
---
id: 7                    # the issue number (matches the 4-digit filename id)
title: Unbounded message sequences capped at 64 elements
status: open             # open | resolved | wontfix
type: enhancement        # bug | enhancement | tech-debt
area: codegen            # codegen | rmw | memory | cmake | zephyr | nuttx | freertos | threadx | build | testing
related: []              # e.g. [rfc-0023, phase-218] — cross-links to RFCs / phases
resolved_in:             # (resolved only) commit short-hash or phase, e.g. "Phase 140"
---
```

**Lifecycle**:

1. Open an issue as `docs/issues/NNNN-slug.md` with `status: open`.
2. When resolved, set `status: resolved` + `resolved_in:` and **move** the
   file to `docs/issues/archived/NNNN-slug.md` (trimmed to a terse
   resolution summary). Do the move in the SAME commit as the status flip —
   skipping it is how 39 resolved issues accumulated in `docs/issues/` by
   2026-08-02, with the "Open issues" list below carrying 86 rows for work
   that was already done.
3. **"Open issues" below lists exactly the files in `docs/issues/`** — one row
   per open issue, no more. A resolved issue keeps a row only under "Recently
   resolved", and only for the current cycle; after that `archived/` is the
   sole record. Verify with: `ls docs/issues/0*.md` versus the `**#NNN**` rows.
4. **Numbering** = the next integer after the highest existing id.
   **Slug** = a kebab-case form of the title; the filename id is the
   zero-padded 4-digit issue number.

## Issue vs RFC vs phase doc

- **Issue** (`docs/issues/`) = a bug, limitation, or tech-debt item.
- **RFC** (`docs/design/NNNN-*.md`) = a design decision.
- **Roadmap phase** (`docs/roadmap/`) = an implementation plan.

Issues cross-link to the RFCs and phases that inform or resolve them via the
`related:` frontmatter field.

## Open issues

(#425 resolved 2026-08-05 — a MIXED C+C++ workspace linked BOTH umbrella staticlibs, ~96 duplicate
C-ABI symbols, blocking `build-test-fixtures lane=native`. RESOLVED by applying a rule the tree
already used for TYPED C components everywhere: prefer the umbrella that BUNDLES the other whenever
it exists. FOUR sites carried the `NOT _TYPED` carve-out and all four had to move together — fixing
three left the count at exactly 96, because `NanoRosGenerateInterfaces.cmake` still dragged the C
archive through the GENERATED bindings, which both languages consume. Pure-C workspaces have no
`NanoRosCpp` target and are unchanged (verified). See `archived/0425-*`.)

(#426 resolved 2026-08-05 — `nros sync`'s host metadata probe dumped raw rustc output for Cortex-M
node pkgs and then degraded silently. NOT fixed by skipping every non-host `[build] target`, which
the filing sketched — that would also skip the Cortex-M leaves that probe FINE today. The real
defect was one line: the probe pipes stderr and echoes it UNCONDITIONALLY, so a failure printed a
screenful before the existing degradation path handled it quietly. Now echoed only on success, with
the first rustc diagnostic folded into the one-line "no producer for X" report. See
`archived/0426-*`.)

RESOLVED 2026-08-05 — **#423** the borrowed-view (RFC-0033) RUNTIME e2e proofs were orphaned +
bit-rotted, deleted, then RE-ESTABLISHED as a build-stage fixture + Rust consumer. Fixed three rots:
the RFC-0042 platform.h include; the `nros_config_variant_sz_*` guard (a standalone `nros-c` can't
size the executor, so the recipe links a matching WEAK anchor read from the config header — borrowed
views touch the CDR buffer via `nros_serdes`, not executor opaque storage, so the guard is a false
constraint the weak-merge mechanism is built for); and a `fixed_str()` helper missing from the drifted
`ffi_wrapper.rs` prelude. `scripts/build/borrowed-e2e-fixture.sh` + `tests/borrowed_e2e.rs` +
`just check-borrowed-e2e`; both C + C++ pass. See `archived/0423-*`.
**#413 (REOPENED 2026-08-05)** — `da26485e9` resolved this as a stale binary and un-carved the
cells. The stale half was real: a fresh rebuild does clear `Transport(ConnectionFailed)` at
`Executor::open`. The CLASS is not fixed — the Rust cyclone talker now opens its session and then
fails `NodeRegister("native_rs_talker")`, while the C talker on the same backend, domain and libddsc
publishes normally, and the ZENOH build of the same Rust source works against a router. Ruled out:
phase-337 W8.a (the merged board carries the cyclone register arm verbatim, now on the one boot
funnel), the descriptor registrar (installed by `nros_rmw_cyclonedds_sys::register()` before open),
and feature forwarding. Blocked on the error the emitted `map_err(|_| NodeRegister(..))` discards —
that discard is why this has been mis-diagnosed twice. The un-carved cells are red on main. See
`0413-*`. (2026-08-05)
(#413 resolved 2026-08-05 — the DECLARATIVE Node API never registered Cyclone type descriptors.
Cyclone resolves topic types through a runtime registry; the imperative typed creators call
`register_type::<M>()`, but the declarative path records metadata and hits the type-ERASED
`create_generic_publisher_with_qos(topic, type_name, …)`, which has no `M`. Hence C/C++ (static
descriptor table) and zenoh/XRCE (no registry) were fine while Rust cyclone was not, and hence it
surfaced only when phase-338 W3 made the native Rust examples Node-class. The ACTION half was NOT
#418's CDR header — it was two more funnels the first pass's text-matching missed; ENUMERATING all
eleven `EntityMetadataSpec` sites found them. Also carries a nextest budget: the pubsub e2e is one
test iterating nine cells and needs 93 s against a 60 s default kill. See `archived/0413-*`.)

**#419** — the play_launch pin in `nros-cli-core/build.rs` records the **superproject** SHA when the
submodule is uninitialised, so `nros sync` reports "this `nros` was built from <a nano-ros commit>"
and the issue-0409 guard fires forever. Two compounding faults: an uninitialised submodule is an
empty dir that `.exists()`, so the intended `"unknown"` branch is unreachable and `git -C <empty>
rev-parse HEAD` walks up to the parent repo; and there is no `rerun-if-changed` on the submodule, so
initialising it re-stamps nothing — `just setup-cli` reports success while rebuilding nothing, and
the remedy the message suggests (`setup-launch-resolve`) cannot help because the wrong value is on
the CLI side. See `0419-*`. (2026-08-05)

**#420** — the `nros_log` facade is a **silent no-op** on ThreadX and NuttX, and on FreeRTOS built
through the Rust board entry. `sinks::default()` forwards to the `nros_platform_log_write` C ABI;
ThreadX and FreeRTOS dispatch that through a fn-ptr slot whose only registrar is
`freertos_c_entry.c:212` (the C/C++ path — ThreadX has no caller at all), and NuttX has no
implementation, falling through to `nros-c`'s weak no-op stub. The link succeeds either way, so the
failure is runtime silence. Nothing has hit it because every shipped body on those platforms uses
`log::info!`, which those boards DO bridge — but phase-338 W7 plans to move the bodies to
`nros_info!`, which would turn every ThreadX and NuttX e2e marker into a grep timeout rather than an
error. W7.a is blocked on this. See `0420-*`. (2026-08-05)

RESOLVED 2026-08-05 — **#382** rlm v0.1.4 preserves launch order (`IndexMap`); the stale generated
TU that kept the test red needed `nros codegen entry`, which needed 0414's CMake half fixed first.
**#398** direction 2: `[[component]].name` stays an instance id, and `apply_model_execution` now
REFUSES a `group_tiers` declaration that reached no node while a node of the same package is in the
model — telling "renamed" apart from "absent in this variant", which look identical otherwise.
**#392** last item done: `orchestration_e2e`'s metadata refresh exposed a service + action the launch
MANIFEST never declared; the manifest was the stale side. See `archived/`.


**#418** — raw action feedback/result payloads carry an EXTRA CDR encapsulation header
(`[outer][goal_id][INNER][body]`), so they are wire-incompatible with ROS 2 *and* with nano-ros's
own typed path. Raw↔raw is self-consistent — every action Runtime cell pairs a raw server with a
raw client, so the double header cancels and nobody noticed. The inner header is deliberate (the raw
consumer reads the body with `new_with_header`), and removing it producer-only reproduces the
issue-#35 corruption its comment warns about, so producer and consumer must change together. This is
what stops phase-338 W3 migrating `action-{server,client}` / `service-client`. Decision in
[RFC-0069](../design/0069-action-payload-envelope.md). See `0418-*`. (2026-08-05)
(#416 resolved 2026-08-05 — `nros sync`'s source digest pruned build dirs by EXACT name, so it skipped
`target/` and walked straight INTO `target-tls` / `target-zenoh` / `target-xrce` / … , the isolated
build dirs `fixtures.toml` gives feature-variant rows. It then read every artifact it found, racing
cargo's own temporaries, and `just build-test-fixtures lane=native` died at a random fixture naming
an `rmeta`/`.o` path. RESOLVED by recognising a cargo build dir instead of listing names —
`CACHEDIR.TAG` plus a `target-` prefix for the not-yet-built case. Listing the dirs would have been
the hand-maintained-exclude-list shape issue 0287 already replaced. See `archived/0416-*`.)

**#415** — `nros::main!` picks the framework emit shape from a hardcoded **deploy-string** table,
while `nros ws check` reads `[package.metadata.nros.board] framework` off the board crate. Two
spellings of one fact, and the macro's falls through to `OwnedSpin` on an unknown key — a silently
wrong entry shape, not a diagnostic. Invisible until phase-337 W7.a deleted `embassy-stm32f4`, the
only in-tree key that selected `Framework::Embassy`; `Framework::Rtic` is in the same shape and only
still reachable via `rtic-mps2-an385`. Fix = let the macro read the same manifest key (needs the
expansion-time fs round-trip `rtic_board_spec_for` already defers). See `0415-*`. (2026-08-04)

Recently resolved (2026-08-04): **#414** — phase-330 W4 made the SystemModel a build artifact and
deleted every committed `config/*model.yaml`; five tests still read those paths and failed on
`os error 2` instead of on what they assert. RESOLVED by the rule that a test never reads a
committed model: where one is needed it is RESOLVED into a temp / build-output dir the way a build
does (`multihost_partition_bake` ×2, `native_main_macro_misuse`, `entry_typed_plan`), and where only
the DECLARATION matters the test reads `system.toml` (`qos_override_e2e`). One consumer named here
originally had been deleted meanwhile; one more turned up in the CLI sub-workspace. Still open and
larger: `nros::main!` consumes the model and tracks it, never seeing `system.toml` or the launch —
so a plain `cargo check` with no build step still fails "SystemModel not found". See
`archived/0414-*`. (2026-08-04)

Recently resolved (2026-08-04): **#413** — a native Rust cyclone/xrce example pair delivered nothing
while C/C++ pairs did (phase-329 W4). NOT a code bug: the rust cyclone/xrce example binaries were 7
days STALE — never rebuilt because no test had exercised those matrix cells — and the stale binary
panicked `Transport(ConnectionFailed)` at `Executor::open`. A fresh fixture-harness rebuild made all
cells deliver (pubsub 9/9, reqresp all green); the code was verified correct throughout
(type-descriptor registration, the message_info CFFI fallback, the rmw-cyclonedds marker). Carves
dropped from both consumers (`da26485e9`); the cells now run in test-all so they can't rot again.
See `archived/0413-*`.

Recently resolved (2026-08-04): **#412** — eight SystemModel files were tracked again under `examples/workspaces/safety`, so
`check-no-tracked-models` (phase-330 W7.e) turns `just check-fast` RED on main. They were scooped
into `3f25803d1`, an unrelated phase-331 fallout fix whose intended change was a launch file plus a
`system.toml` entry — the `git add -A` class. Diagnosis says the fix is SAFE despite first
appearances: seven regenerate byte-identically (the only diff is a `meta.inputs` sha256, which
*should* move once inputs change), and the eighth (`rust_system_model.yaml`) is an ORPHAN — its
launch file was deleted in `9748f7ae3` when two sessions fixed the same problem at once, leaving a
generated file with no producer. RESOLVED: all eight deleted, `check-no-tracked-models` green — safe only once `nros::main!` stopped
requiring a pre-resolved model (`2b022c32a`). See `archived/0412-*`. (2026-08-04)

Recently resolved (2026-08-04): **#411** — the threadx-linux C workspace entry was resolved as `native_threadx_entry` while the build
produces `threadx_entry` (`binaries/mod.rs:1846` vs `fixtures.toml:539`), so
`entry_e2e::case_01_threadx_linux_c` silently SKIPS under `just test-all` (and fails under bare
nextest). Invisible because a missing fixture and an absent toolchain look identical to the resolver —
the 0350 class. Reads like phase-331 rename fallout that swept the workspace and manifest but not the
one call site hardcoding the binary name. Direction: fix the string, then check for siblings — the
entry argument can be derived from the manifest row the call already names, since two spellings of one
fact is what produced it. RESOLVED exactly that way: the resolver names `threadx_entry`, and
`build_workspace_cmake_entry_in` now looks the entry up in the manifest record it already fetches and
FAILS LOUDLY on a mismatch — covering ~20 call sites rather than the one that drifted (swept: no other
mismatches). See `archived/0411-*`. (2026-08-04)

**#409** (RESOLVED 2026-08-04) — `setup-launch-resolve` exited 0 without building, so `nros sync` ran
a stale resolver that silently dropped every `params`/`params_files` projection (22 models stripped,
no error). Closed from three sides: the recipe now fails (and its opt-out DELETES the stale binary),
`sync` refuses a resolver whose play_launch pin is not its own (crate versions cannot tell them
apart — both read 0.5.0), and a resolve that loses declared params cannot be promoted. See
`archived/0409-*`.

Recently resolved (2026-08-04): **#410** — a git worktree inside the checkout is claimed by the OUTER
workspace. `.claude/worktrees/agent-*` sits under the main checkout, so cargo's walk-up from a package
there escapes the worktree and lands on the root `Cargo.toml`, whose `exclude` paths are relative and
match nothing — "current package believes it's in a workspace when it's not", naming the outer
manifest. Only crates that are root-`exclude`d AND carry no `[workspace]` of their own are hit (board
crates, PACs, verification), which is why it hid: standalone copy-out examples are immune. Broke
`check-leaf-lockfiles` (~20 untouched crates), `cargo fmt --all` and two fixture builds for two
phase-337 agents at once, in trees whose diffs were clean — every message naming a package the agent
never touched. RESOLVED: `exclude = [".claude", …]` in the root manifest. See `archived/0410-*`.

Recently resolved (2026-08-03): **#406** — a fixture build narrowed to an id that matches nothing
exited 0 having built nothing. `fixtures-build.sh native rust --id workspace-rust-native-realtime`
returned rc=0 in 0.03s: the id is real, but names a `[[workspace_fixture]]` and that script lists
only `[[fixture]]` rows. Same for a typo'd id or platform, and the sibling builder printed a line
then exited 0 too — the 0351/0196 shape, feeding the 0393 coverage stamp. RESOLVED: one shared
`scripts/build/fixture-id-guard.sh` keys loudness on the SPELLING of the filter (`--id` targets one
builder, so empty is fatal; `NROS_FIXTURE_ID` is a sweep-wide narrowing, so a stage that misses says
so and passes; an id in NO table is always fatal; an unfiltered empty coordinate stays silent, since
`threadx-linux/mixed` legitimately has 0 rows). Gated by `check-fixture-id-guard` in `check-fast`.
See `archived/0406-*`.

**#405** — the tier2 lane gate demanded `workspace-c-nuttx-riscv-realtime`, but nothing the tier2
builder ran could produce it: `lane-coords` maps the `nuttx-riscv,c,zenoh` coordinate to the `nuttx`
module, whose `build-fixtures` recipe built only the arm side — the riscv workspaces lived in
separate `full-matrix` recipes (shared kernel tree, one board config at a time). Masked until
phase-331 W6's `ws-realtime-c` → `realtime-c` rename orphaned the old artifact. RESOLVED (phase-337
W3.f): `just nuttx build-fixtures` is now `build-fixtures-arm` then `build-fixtures-riscv` — one
stage, serial, with the riscv half gated on the run's own coordinates by the shared
`nros_lane_wants_platform` helper. The issue-0196 half is
`every_fixture_token_is_producible_by_the_module_that_owns_it`, which walks each module's recipe
graph from `build-fixtures` and fails when a fixture token that module OWNS is produced by no recipe
on that path. See `archived/0405-*`.

**#429** — `nano2nano::{test_gid_consistency,test_sequence_number_increment}` parse `MessageInfo`
trace lines out of the native listener under `RUST_LOG=trace`; the listener emits none (verified
directly: 0 matches for gid/MessageInfo/"Waiting for"), so "got 0" is literal. Transport is fine —
the talker publishes against the same router. Grep-drift class (archived 0157/0164): diff the
pattern against what the fixture prints before debugging delivery. See `0429-*`. (2026-08-05)

**#428** — every CycloneDDS runtime test fails at node registration while every zenoh test in the
same file passes: `session open (rmw=cyclonedds)` then `NodeRegister("…")`. Reproduces from a
directly-invoked binary. Ruled out stale fixtures, missing backend (79 cyclone symbols present),
environment, and the phase-336 profile work. Blocked on diagnosis because `decl_err_from_node`
collapses every `NodeError` except `ExecutorFull` — widen that seam first (issue 0095 did the same
once). See `0428-*`. (2026-08-05)

**#427** — a SystemModel reads as FRESH when only the RESOLVER changed, so a resolver fix never
reaches models that already exist. `meta.inputs[].sha256` covers the launch file and `system.toml`;
`meta.resolver` is recorded but not part of the decision, so `nros sync` exits 0 having done nothing.
Surfaced as `cpp_multi_node_entry` "component order doesn't match launch XML" — the rlm v0.1.4
declaration-order fix (issue 0382) reaching new models only. Workaround `rm -rf <ws>/build/nros/models
&& nros sync`, verified. Second defect noted: models stamp `resolver.version: 0.1.0` while the
resolver is v0.1.4. See `0427-*`. (2026-08-05)

**#422** — TRIAGE INDEX for the 19 runtime E2E failures on a clean tree (tier 1 gates all pass;
1231/1257 tests do). Eight are now diagnosed across three bugs — `0427` (stale SystemModel),
`0428` (cyclone node-register), `0429` (nano2nano grep drift) — and eleven remain. The original
"plausibly environmental" framing was WRONG and is corrected in the issue: zenohd/ROS are present
and every failure examined so far is a real defect. See `0422-*`. (2026-08-05)

**#421** — RESOLVED. `zephyr_leaf_buildrs_uses_shared_bake` asserted a floor of 13 zephyr rust
leaves, phase-291's count; phase-331 W3/W4 deleted four `ws-*-rust` micro-workspaces that each
carried a `zephyr_entry` leaf, leaving 10. Fixed upstream in `1f19ea937` (floor 10, with the
deletion recorded). The original filing said 7 and blamed phase-277 — it grepped only
`examples/zephyr/rust/` and missed the `zephyr_entry*` rule. See `archived/0421-*`. (2026-08-05)

**#404** — no schema for DECLARING a measured WCET. `MapperPath.exec_ms` is `Option<f64>` and nothing
outside rlm's own tests ever sets it, so rlm v0.1.4's `ChainFeasibleWithoutWcet` (issue 0259) now
reports missing evidence with nowhere to put it. Open questions: keying (board id / platform family /
named measurement profile — a WCET belongs to a context, not to code), unit (mapper wants ms, the bench
measures cycles, converting needs a clock rate), granularity (mapper wants a whole callback, the bench
measures primitives), and provenance/staleness. Blocked on 0403 — designing the schema before a
producer emits an artifact answers the keying question by guess. Invariant: absent stays representable
and stays the DEFAULT, else zeros get written in by hand. See `0404-*`. (2026-08-03)

**#403** — the WCET bench emits prose nothing parses, and a QEMU run with a dead cycle counter reports
zeros as if measured. `nros-bench/wcet-cycles-qemu` prints `min=/max=/avg= cycles` to a semihosting log
with no parser, no schema and no consumer, in the `debug` group so no lane runs it. On QEMU the DWT
never increments: the bench detects this, prints a NOTE, then measures anyway and exits 0 — warning and
data on the same stream, and only the data is machine-shaped. Same failure as 0259 one layer earlier
(a non-measurement entering as zero, the most optimistic WCET there is). Direction: structured artifact
with conditions recorded, and a dead counter is a HARD failure emitting no numbers at all. See
`0403-*`. (2026-08-03)

Recently resolved (2026-08-04): **#402** — message codegen had no language-neutral IR: parse /
dependency-resolution / RIHS hashing / sizing were entangled with per-language emission in
`rosidl-codegen`. RESOLVED by RFC-0068 (Stable) / phase-335: a four-stage pipeline **parse → resolve
→ lower → render**, byte-identical until the final wave. Resolve (`rosidl-resolve`) hashes once and
carries the type-description closure; Lower (`rosidl-lower`) adds target-parameterized embedded
constraints for the `no_std` C/C++ emitters; Render drives every backend from runtime `minijinja`
data packs — adding a language is dropping a pack, not editing Rust (askama gone). Boundary is an
in-process trait, not a serialized JSON-IR. See `archived/0402-*` + `book/src/internals/codegen-packs.md`.

Recently resolved (2026-08-05): **#398** — `[[component]] name` no longer matches the launch node name, so every per-node projection
keyed on it silently binds NOTHING. phase-331's consolidation gave component names workspace-unique
prefixes (`rust_params_param_talker`) while launch files kept the plain node name (`param_talker`):
ZERO of `features/`'s 20 component names match any of its 8 launch nodes. It stayed invisible because
that bringup declares no `group_tiers` — the phase-330 W4 params projection is the first per-node
projection it uses, and only its diagnostic made the failure visible at all. Worked around for params
in rlm v0.1.2 (unambiguous `pkg` fallback); `group_tiers` still matches by bare name and will bind
nothing the moment a consolidated workspace declares one. Direction: recouple the names, or key every
projection on `pkg`+`class` — but ONE rule, and loud on failure. RESOLVED as direction 2, the naming
decision belonging to phase-331: `name` stays an INSTANCE ID (recoupling would need per-language
namespaces, changing wire-visible node names and every test asserting them), and the missing
invariant — that failing to match is LOUD — is now enforced. `apply_model_execution` refuses a
`group_tiers` declaration that reached no node WHEN a node of the same package is in the model,
which distinguishes "renamed" (the phase-331 hazard) from "absent in this variant" (legitimate — a
bringup is a catalog and each launch uses a subset). The first draft failed on both, which
`realtime-cpp`'s `aux_node` caught on the first build. See `archived/0398-*`. (2026-08-05)

**#397** (RESOLVED 2026-08-03) — a failing `nros` CLI made `check-model-dims` report every dim of
every model as LOST. The loop read `… 2>/dev/null || true`, so a stale-CLI refusal after a rebase
(the common case) produced zero dims per model and a 118-line data-loss report for content that
was intact — advising "restore from git history" when the fix was `just setup-cli`, whose message
the redirect had discarded. Worse, `--write` shares the loop: re-recording through a broken CLI
bakes the empty reading in, which is the loss the gate exists to prevent. Now fatal per model,
with the CLI's stderr, and both compare and re-record refuse. Watched to fire against a stub CLI —
the first version used a shell variable set inside a pipeline subshell and silently never fired.
See `archived/0397-*`.

**#395** (RESOLVED 2026-08-03) — phase-331 W6 dropped the `[tiers.*.freertos]` declarations when it retargeted the `workspace-cpp-freertos-realtime` fixture onto `realtime-cpp`, and pointed the row at a `deploy_bringup` that belongs to a different workspace. Restored from `a92778843^` (high=5, low=2, cpp low core=0 — the dim `tests/freertos_core_pin_applied.rs` asserts), row repointed at `src/demo_bringup`. Two further baseline entries turned out to be W6 RENAME errors rather than losses: the `fast`/`bulk` dims were intact under `realtime-cpp-subnode-portable`, and the `mid.*` three-tier system was deliberately dropped (`aux_pkg` unreferenced — REVERSED by the follow-up below).

Follow-up on the same day: the two-tier restore left `aux_pkg` unreferenced on the reading that
no test consumes a `mid` tier. `realtime_tiers_e2e`'s `freertos_cpp` cell does —
`Proof::SerialTicks(["ctrl", "aux", "telem"])`, the #144 chained-spawn signal — and the row's
"2-tier" comment described the OTHER workspace (the pre-W6 row pointed at the 3-node mps2
bringup). `[tiers.mid]` + the `aux_node` binding are back; because `aux_pkg` builds only on the
mps2 board, the 3-node resolve is a VARIANT model (`freertos_system.launch.xml` ->
`freertos_system_model.yaml`, the pattern the rclcpp/subnode entries already use) rather than a
second bringup, so W6's fold holds. Generated `NativeTierSpec` is three entries with the
two-hop spawn chain. Baseline 118 dims / 9 models.

Recently resolved: **#396** (filed as #395; renumbered — id collision) — features +
local-msg-package couldn't `nros sync`: the phase-333 migration narrowed the patch table while the
workspace-LOCAL interface crates were still registry-named, and the phase-327 narrowing guard
correctly refused the write. RESOLVED (2026-08-03): direction A — the local crates moved to the
RFC-0067 path-dep spelling, and `registry_style_dep_names` learned that path+version is a PATH dep
(cargo never consults the registry for it), so legitimate narrowing passes while the
registry-strand protection stays. See `archived/0396-*`.

Recently resolved (2026-08-02): **#393** — the fixture BUILD ignored the CI lane that the staleness
gate and the test run already agreed on: `build-test-fixtures` took no lane and fanned out over all
nine platform families, so tier 1 built all 337 manifest rows to run the 180 native ones. RESOLVED:
`just build-test-fixtures lane=<all|native|tier1|tier2|tier2-nightly>` narrows both the module
fan-out and the manifest rows from one `lane-coords` computation, and `.fixtures-built` now records
`lane=` + one `coord=` per coordinate so `_require-fixtures` checks COVERAGE, not existence (the
scope half of #0351). Also fixed: the stamp survived a failed build (the clear ran after the
dependencies that build), three copies of the stamp writer, and a dead `NROS_FIXTURE_SHARED_SIG`
export whose failure `export` was masking. See `archived/0393-*`. (2026-08-02)

Recently resolved (2026-08-05): **#392** — Six bringups cannot `nros sync` AT ALL (not "produce a different model" — fail before
writing anything), so each is a phase-330 W4.a blocker: a bringup that cannot sync cannot regenerate
its deleted model. Two legacy `[system]` schemas (`n9_workspace` missing the required `name`;
`multi_pkg_workspace_freertos` spelling `launch`/`components`/`zenoh_locator` — its header says a
BSP `build.rs` also reads it via `NROS_SYSTEM_TOML`, so migrating it blind trades a sync failure for
a FreeRTOS build failure), two components declaring no `class` (possibly NEGATIVE fixtures for that
very check), and one launch resolving an uninstalled package. Also records the six W4.a failures
that were NOT bugs — a wsroot bug in the probe, and `@NANO_ROS_ROOT@` templates that are
materialised before use — so nobody re-investigates them. RESOLVED: A, B and C landed 2026-08-02;
the last item was `orchestration_e2e`'s stale metadata. Refreshing it (+122 lines, `node_talker` ->
`talker`) immediately failed `plan_pipeline_e2e` with two `metadata-entity-unmatched` errors — the
source declares a service and an action the launch MANIFEST never listed, and the stale metadata had
hidden both. The manifest was the side that had fallen behind; it declares them now (the schema has
supported `services:` / `actions:` all along). `n9_workspace` not syncing IN PLACE is not a defect:
`@NANO_ROS_ROOT@` is what makes it a template. See `archived/0392-*`. (2026-08-05)

(#399 resolved 2026-08-03 — see `archived/0399-*`: `qemu-esp32-baremetal` compiled zpico C for
riscv32imc but nothing named the compiler, so cc-rs guessed `riscv32-unknown-elf-gcc` and a
documented-provisioned host couldn't build it. Class fix: one shared `RISCV_GCC_CANDIDATES` const
across zpico-build's three riscv probes (was three copies of a two-name list, all omitting the
provisioned `riscv-none-elf-gcc`), the shim build path now calls `detect_riscv_compiler` like the
lib path, and the board declares `packages = ["riscv-none-elf-gcc"]`. No per-example `CC_*`.)

**#408** (RESOLVED 2026-08-04) — seven native e2e reds, THREE distinct faults: a queryable-table
overflow at boot killed all five workspace-feature/param lanes (a service server is a queryable; ROS
param services + lifecycle services need 12 slots, the default was 8), an IPv4-only TLS listener vs a
hostname resolving to `::1` first, and a probe sidecar discarding a manifest-only topic declaration
so the bridge lost `/header`. All fixed and verified behaviourally. See `archived/0408-*`.

**#407** (RESOLVED 2026-08-04) — tier 1 selected a threadx-linux test whose fixture `lane=native`
never builds, so `just ci` failed on a fixture it declines to build. The test had lost its platform
token to the phase-221 naming audit, which predates the token-matching lane filter (#357) that later
made the token load-bearing; restored as `logging_smoke_threadx_linux_captures_stderr`. See
`archived/0407-*`.

**#401** (RESOLVED 2026-08-04) — the box's CARGO_TARGET_DIR and the LEAF-RELATIVE fixture path
contract were mutually exclusive: redirecting put fixtures where tests never stat, so a truthful
"built" report met a wholly red test run. Resolved by the same tree split as #400 — no redirect in a
box-owned tree, so cargo writes the leaf paths the contract names. See `archived/0401-*`.

**#400** (RESOLVED 2026-08-04) — host and box shared one checkout whose artifacts are glibc- and
toolchain-specific; five instances in one session (build scripts, CMake caches, the CLI, the
resolver, the SDK store). Fixed at the premise: the box now mirrors into its OWN tree
(`scripts/dev/ros2-box-sync.sh`) and stops redirecting CARGO_TARGET_DIR there. See `archived/0400-*`.

Recently resolved: **#391** — ThreadX-RV64 C fixture lane built wrong-from-clean + a museum binary
passed the staleness gate. RESOLVED (2026-08-02, `8ef697c95`): (1) the cmake configure helpers now
`rm -rf` a build dir whose cached `CMAKE_TOOLCHAIN_FILE` differs from the requested one — CMake pins
the compiler at first configure and won't swap it on re-configure, so a host-cc-poisoned cache used
to persist (RISC-V kernel → x86 assembler, `csrrci %rax`); the lane self-heals now. (2)
`require_prebuilt_binary_fresh_cmake` also walks `zpico-sys/c/**` (the cargo-nested C shim invisible
to `ninja -t deps`) for zenoh fixtures, so a `zpico.c` edit trips STALE instead of running a museum
binary (issue-0196 class). See `archived/0391-*`. (2026-08-02)

(#390 resolved 2026-08-03 — see `archived/0390-*`: `nros setup <board> --rmw <x>` provisions one
board+rmw slice, but the repo build stage needs the UNION of vendored `[source.*]` (`just test`
links every RMW's `-sys`; `build-test-fixtures` metadata-refresh path-deps platform sources like
`nuttx-libc`), and the failures named no `nros` remedy. Fixed three ways: (Dir 2) the metadata-refresh
harness translates a missing-source cargo error into `nros setup --source <name>` (index-driven) and
every `-sys` gate leads with the same (`e7bdacef7`/`1ba7b23ee`); (Dir 1) a top-level `build_sources`
union in the index + `nros setup --build-sources[ --check]` + a `_require-build-sources` preflight on
`just test`/`build-test-fixtures` (`f63bf71bd`); (Dir 3) the book contributor section says so.
Verified locally both ways incl. a simulated-missing run.)

Recently resolved: **#389** — `ZPICO_MAX_SESSIONS` defaulted to 1 with nothing raising it, so the
cross-session test — the only e2e proof for the #348/#376 multi-session work — skipped on every host
in every tier, always. RESOLVED (2026-08-02, `b1db98829`): `just test-zpico-multisession` builds the
shim at 2 in its own target dir and is a dependency of `just test`; the test PASSES, its first
execution ever. Global default stays 1 — a root `[env]` would leak into in-tree examples via cargo's
config merging (verified) and double their session tables. See `0389-*`. (2026-08-02)

Recently resolved: **#388** — `just test-unit` claimed "no external deps" but two tests hardcoded
`build/zenohd/zenohd`, bypassing the harness's own resolver, so a host provisioned by the documented
`nros setup --rmw zenoh` failed with zenohd installed; a third red was a `nros_tests::skip!` that
only `test-all` converted. RESOLVED (2026-08-02, `e6ed5d68a` + `a7dc55609`): tier 1 honours
[SKIPPED], both files use `zenohd_binary_path()`, and `just zenohd setup` symlinks the store copy
instead of source-building zenoh a SECOND time. Verified with store+build, store-only, and neither.
See `0388-*`. (2026-08-02)

(#387 resolved 2026-08-02 — see `archived/0387-*`: the C-arm runtime "regressions" were TWO
real roots plus confounders. (1) tier class `ba57adf24` — `open_over_session` left
`CppContext.in_dispatch` uninitialised → spurious REENTRANT killed borrowed-tier spins (native +
NuttX-arm + EDF C/C++; Rust's own run_tiers untouched). (2) ThreadX class `4b8c63b36` —
`zpico_spin_once`'s ThreadX arm returned `ZPICO_ERR_TIMEOUT` on idle where every other MT backend
returns 0, so nros-c `spin_period` bailed at 16 idle spins (ThreadX-Linux + ThreadxRiscv64 C
pubsub/service/action; only nros-c consults the counter, so Rust/Cpp passed). `cpp_c_param_live_read`
was a flake (`921eb1a0a`: stale sink skip miscounted + entry outlived by the wait). All QEMU-confirmed.
Residual `rtic-action`/`zephyr-rust-params` are pre-tagged load flakes, not the C class.)

Recently resolved: **#378** — leaf message deps resolved against the PUBLIC crates.io whenever the
`[patch.crates-io]` redirect was not in the loaded config chain (cwd-dependent), and committed leaf
locks pinned whichever ROS distro generated them. RESOLVED (2026-08-02, `93aa02016`, RFC-0067 /
phase-333 W1–W3): message deps are PATH deps repo-wide (138 manifests; 323 patch entries retired,
`nros sync` no longer emits them), generated crates are `version = "0.0.0"` with the ament version
as metadata, and `check-msg-dep-is-path` asserts the property rather than the mitigation. From the
repo root every message id now resolves `path+file://…`, never `registry+…`. Still open as RFC-0067
Q1: `nros-core`/`nros-serdes` keep the cwd-dependent patch. See `0378-*`. (2026-08-02)

**#374** (filed as #373; renumbered) — `nros setup native --rmw zenoh` source-builds zenohd
(`[tool.zenohd]` has no `dist.linux-x86_64`; assets never seeded) and pulls a SECOND rust toolchain
(`1.85.0`, zenoh's own pin) for 792 MB of store — while installation.md:123 promised "ships prebuilt
toolchains per platform per RMW" for exactly this board. **Narrowed 2026-08-01:** the wait is now
announced up front by `nros setup` and the book no longer promises unconditional prebuilts; what
remains is out-of-repo — publish `1.7.2-nros2` assets on `nano-ros-sdk` so the dist rows return.
See `0374-*`. (2026-08-01)

**#371** [severity **high**] — native_sim cyclone app abort()s at a near-deterministic 19–21 s
joining the full Autoware graph during the safety-island demo's scenario init (7/7 on 2026-08-01;
the same tree passed 2× on 2026-07-31 when one sim node was down). mrm_handler flaps
operate/cancel (hot service path) right before death; unnamed cyclone pthread; gdb masks it;
strace -k unwinds only to the zephyr print shim. Isolation: island alone / single-peer feed /
availability flap / idle sim / EKF-odometry all survive — only the full graph + scenario churn
kills it. TRIGGER CONFIRMED by A/B: shadowing autoware_manual_lane_change_handler out of the sim flips 7/7 aborts to VERDICT PASS. See `0371-*`. (2026-08-01)

Recently resolved: **#380** — model regeneration destroyed hand-authored execution dims (four
incidents). RESOLVED structurally by phase-330 (2026-08-03): dims live in `system.toml`, the
committed model is GONE (W4.a — build artifact under `<ws>/build/nros/models/`, entries are
input-addressed via `launch =`/`LAUNCH`, `check-no-tracked-models` bans tracked copies). With no
committed artifact, the conflict cannot recur. See `archived/0380-*`.

Recently resolved (2026-08-05): **#382** (filed as #372; renumbered) — The resolver serializes `structure.nodes` alphabetized, so entry construct order no
longer follows launch declaration order (typed cpp multi-node TU builds listener before talker).
Preserve declaration order in the emitted mapping + a resolver-level order test. Fix collides with
#380's regeneration hazard — sequence them. RESOLVED upstream: `ros-launch-manifest` v0.1.4 carries
`nodes: IndexMap` and the resolver populates it, so no consumer-side sort is needed. The predicted
regeneration hazard is what kept the test red AFTER the fix landed — the committed TU predated the
pin move, and regenerating it needed `nros codegen entry`, which died on the model phase-330 W4
deleted (0414's CMake half, fixed in the same change). `cpp_multi_node_entry` 4/4, constructing
talker before listener. See `archived/0382-*`. (2026-08-05)

(#368 resolved 2026-08-03 — see `archived/0368-*`: `just setup all` failed 7/18 modules on a clean
Ubuntu 22.04, nearly all on prereqs RFC-0062's dependency SSoT was meant to absorb. All EIGHT
findings closed. F1 (sudo `apt-packages` first aborting the sudo-less installers behind it) + F3
(qemu libslirp system dep) + F5 (pyo3/z3/libclang) + F6 (verus pin) + F7 (aria2/nextest/colcon…) +
F4 (complete the bundled interface set + a narrowing guard that HARD-STOPs rather than silently drop
a leaf patch entry) all landed in phase-327 W2–W6 — the issue was just never updated, several fixed
the day it was filed. F2 (doctor remedies name the index tool, not apt — fixed the SIBLING sites the
first pass missed) + F8 (`ci-matrix{,-nightly}` set `NROS_FIXTURE_LANE` so its two fixture gates
agree) closed this cycle. Audited exhaustively.)

**#362** — phase-325 W3's uORB→RMW bridge is blocked on TYPES, not plumbing. The plumbing is proven
(one PX4 module links uORB + zenoh, `NodeBuilder().rmw()` gives two sessions, backend selection is
the cargo-feature knob one layer down). But the bridge must TRANSLATE: inward a payload is the PX4
struct keyed by `ORB_ID`, outward a real ROS 2 subscriber needs CDR with a type name AND type hash.
`nros generate-px4-msgs` already emits exactly the right message set — CDR `px4_msgs::msg::*` from
the PX4 `.msg` tree — but as a **Rust crate**, for the XRCE companion path; an in-firmware module is
C++. Hand-rolling the CDR is not the shortcut it looks like: `rmw_zenoh` keys discovery on the type
hash, so a guessed one is either invisible to ROS 2 or, worse, decoded as a different type. Worth
doing beyond the bridge: using `px4_msgs` makes the bridge's ROS-2-facing contract identical to what
`uxrce_dds_client` already publishes, so a subscriber cannot tell which produced a sample — that
indistinguishability is the interop claim. Blocks W3 only; the W2 direct demo needs no CDR at all,
which is the point of it. See `0362-*`. (2026-07-31)

**#260** — the RFC-0052 Native sched dims are e2e-verified only on the FALLBACK arm: every realtime
fixture is uniprocessor, so the SMP core-pin ACCEPT path (`k_thread_cpu_pin` /
`pthread_setaffinity_np` / `vTaskCoreAffinitySet`, `#ifdef CONFIG_SMP`) is compile-verified only.
Needs ONE SMP fixture (a separate zephyr native_sim SMP variant, not the shared image) to flip a
two-mode core-pin e2e to accept. See `0260-*`. (phase-296 W5.11 2026-07-24)

**#259** — derived scheduling is quantitatively INERT: the model carries no per-callback WCET
(`MapperPath.exec_ms` is `None` everywhere), so the budget dim short-circuits, blocking (`B_i`)
can never be numeric, and the feasibility check assumes `B_i = 0` — **unsound whenever callbacks
share a resource**. The originally-filed symptom (`placement`/`non_preempt_scope` hardcoded
`NotRequested`) follows from it: both are MECHANISMS, not requirements, so no contract fact
implies them. Rewritten 2026-07-26 with the design review — contention is already declared via
MutuallyExclusive callback groups (no new vocabulary for the intra-node case), a `holds:` contract
field is rejected (locking is implementation, not interface), and `criticality → core pin` is
rejected outright (unfalsifiable adjective; would look like a partitioning claim nothing backs).
Prereqs: WCET fact, board peripheral registry, `SchedCaps` core count. See `0259-*`.
(phase-296 W5.11 2026-07-24; reframed 2026-07-26)

(#286 resolved 2026-07-26 — see `archived/0286-*`: the blocker was NOT the missing runner but
`[unstable] build-std`, which is not target-scoped and rebuilds std against nuttx's patched libc
even under `--target <host>`. `probe_blocker()` now degrades such components to the documented
sidecar-less path; nuttx build-fixtures rc=2 → rc=0, qemu still probing.)

(#285 resolved 2026-07-27 — see `archived/0285-*`: ships `nros-launch-resolve`, a dedicated
distinctly-named helper built from a pinned play_launch submodule and invoked by ABSOLUTE PATH,
never $PATH. Kills the version skew and both directions of the name collision — we can't be
shadowed by the unrelated ROS 2 `play_launch`, and we never shadow it either.)

(#311 resolved — no feature SSoT across languages; one `nros_feature_set()` now serves nros-c, nros-cpp and the umbrella, and 50 Rust nodes stopped naming a ROS edition. See `archived/0311-*`.)

(#288 resolved 2026-08-03 — see `archived/0288-*`: deploy-bound standalone examples (node +
entry in one crate) are now host-metadata-probed. The blocker was a STACK of five layers — ungated
Rust asm, build-script cross-compiles, `no_std`+unwind, the probe's up-front deploy-bound skip, and
finally the board's platform C ABI going undefined at LINK. All fixed: skip lifted (best-effort +
source-digest negative cache), and the harness deps `nros-platform-cffi[posix-c-port]` so the
`nros_platform_*` symbols are host-defined. Exact executor sizing now applies; issue 0257's boot
failure no longer reachable for these. Renumbered from a duplicate #286.)


(#293 resolved 2026-07-27 — see `archived/0293-*`: `system.toml` had TWO parsers with different
schemas, so rlm silently dropped `launch = "…"` on deploy blocks and counted launch-scoped
blocks against every launch file; `<node machine=>` was ignored as a placement too. demo_bringup
could not be re-resolved at all. Fixed in rlm + play_launch. The SSoT question — one deploy
schema instead of two mirrors — is recorded there as follow-up.)

(#305 resolved 2026-07-28 — see `archived/0305-*`: one probe project per workspace instead of
N. 8.8 GB for 3.5 of 6 components became 2.9 GB for all 6.)

(#304 resolved 2026-07-28 — see `archived/0304-*`: nros-cpp has TWO build paths and the
`NROS_EXTRA_CPP_FEATURES` hook was only on the umbrella one; the probe takes the other. Probe now links, runs, and records:
all six C++ components produce sidecars.)

**#200** — fixture-build timing campaign blocked on a big-disk CI runner (phase-226 validation
residue). See `0200-*`.

(#270 resolved — `nros-rmw-zenoh` pulled `zpico-sys` DEFAULT features, double-defining clock symbols on orin-spe. See `archived/0270-*`.)

**#271** — Orin SPE BTCM footprint regressed ~+195 KB between `d9af52be` and `21a3a4248`; a
minimal `Executor::open`+spin image no longer fits 256 KB. See `0271-*`.

(#272 resolved — `nros sync` now INLINES the nros/nros-core/nros-serdes trio with absolute paths for
out-of-tree consumers (no fragile `include`); in-tree example leaves keep the committed relative
include. Failure class `no matching package named 'nros'` eliminated. See `archived/0272-*`.)

(#273 resolved — `nros-board.toml` `[board.entry]` signature `_start`→`main` (matches `board_mps2.c`
`Reset_Handler`); the scaffold stub fixed too; the off-lane wake-latency bench spun to `#0317`.
phase-313. See `archived/0273-*`.)

(#284 resolved — `NROS_CYCLONEDDS_MAX_TYPES` hidden compile-time knob, twin of the fixed `NROS_EXECUTOR_MAX_CBS`. See `archived/0284-*`.)

Recently resolved: **#278** — no polling subscriber / blocking service futures. RESOLVED
(2026-08-02): added `nros::PollingSubscription<M>` (latest-value polling sub, take_data/
take_new_data/take/peek, drain-to-latest) and `nros::Client<Svc>::call_polling(req, resp,
timeout_ms)` (bounded service call that never spins the executor → callback-safe on
multi-threaded backends, times out on single-threaded ones). Both pure C++ over existing
primitives, gated by compile tests. Replaces the mrm_handler cache-latest + send-and-poll
weakening. See `0278-*`.

(#290 resolved 2026-07-26 — see `archived/0290-*`: nros-cpp blocking helpers had no
reentrancy guard, so calling one from inside a callback aliased `&mut Executor` silently;
C guarded it all along via `nros_executor_t.in_dispatch`. Fixed with a `DispatchGuard`.)

(#278 + archived #276, #277, #279-#281 from the simple-autoware-safety-island port
friction log, filed 2026-07-25 from docs/porting-notes.md 06/12/14 + the
same-day-fixed 10/16/18. #275 resolved by RFC-0057/phase-305 — see
`archived/0275-*`: L.4 retired, upstream namespaces port verbatim. #276 resolved
2026-07-26 — see `archived/0276-*`: `<param from=…>` parses, param-file values
project into the generated entry; seeding gated on `param_services`. #277 resolved
2026-07-26 — see `archived/0277-*`: phase-306's per-package FFI crates remove the
mixed-subset failure mode by construction (verified on a disjoint-subset workspace,
zero duplicate exports); no union closure needed, union-shim pkg obsolete.)

(#248 resolved 2026-08-04 — Embassy board entry was a stub whose crate could never earn a runtime
lane; RESOLVED BY DELETION in phase-337 W7.a, which is what the issue's own analysis pointed at
("finishing as stm32f4 can never earn a CI runtime lane"). The SEAM stays — `EmbassyBoardEntry`, the
`nros::main!()` Embassy emit branch, and `nros ws check`'s Deferred lint — because RFC-0064 keeps
framework seams and lets boards arrive from integrators. Narrower successor: **#415**. See
`archived/0248-*`.)

(#244 resolved — platform ABI surface asymmetry: PlatformSerial/PlatformIvc had no C header mirror. See `archived/0244-*`.)

(#243 resolved — board-trait duplication ended: `board_init` deleted; two canonical board APIs
(`nros_platform::board` Rust + `<nros/board.h>` C), gated by `check-no-board-init`. phase-313.
See `archived/0243-*`.)

(#242 resolved — RMW parity gaps vs rmw.h: publisher GID + message-info out-param. See `archived/0242-*`.)

Recently resolved: **#240 + #241** — phase-301 landed the batched RMW shape alignment on the
RFC-0054 header SSoT (create_session/subscription/service/client terms, hints in options structs,
call_raw deleted, fallible QoS boundary lowering with the ceil/reject semantics) — `archived/0240-*`,
`archived/0241-*`. (2026-07-24)

## Recently resolved

Closed in the current cycle, kept as a short changelog. Anything older lives only in
`archived/` — that directory, not this list, is the record.

Recently resolved: **#386** — the `--locked` cargo shim broke the book's first node on a FRESH
clone: the example leaf `Cargo.lock` is gitignored, so cargo must CREATE it and `--locked` forbade
exactly that. RESOLVED (2026-08-02): the shim skips `--locked` when the manifest's `Cargo.lock` is
git-ignored (a regenerable artifact, not a tracked promise) — resolves the manifest dir
(`--manifest-path` or cwd) and probes `git check-ignore`; tracked-lock workspaces are unaffected.
Fixes both the can't-create and can't-update-stale modes. Sibling of #384. See `0386-*`. (2026-08-02)

Recently resolved: **#385** — `nros setup` couldn't unpack a `.tar.zst` prebuilt dist on a host
without `zstd`: `tar (child): zstd: Cannot exec`, reported as a bare `unpack prebuilt archive`.
RESOLVED (2026-08-02): added `[system.zstd]` to the index (D1 — listed by `--system`/doctor when
missing) and made `sdk_store` probe `zstd` BEFORE downloading a `.zst` dist, failing with the
package-manager install command (D2). Verified both with zstd masked. See `0385-*`. (2026-08-02)

Recently resolved: **#384** — `scripts/bin/cargo` (the `--locked` injector) appended `$FLAGS` at
argv TAIL, so any `cargo <sub> -- <args>` leaked `--locked` to the child (test harness /
clippy-driver): `error: Unrecognized option: 'locked'`. Surfaced by #379's clippy lane + 4
runtime-clippy tests in `rosidl-codegen/tests/compilation_test.rs`. RESOLVED (2026-08-02): insert
`$FLAGS` BEFORE the first `--` (they're cargo's own flags); no-`--` case unchanged. Verified `cargo
test … -- --nocapture` + `cargo clippy … -- -D warnings` run cleanly through the shim. See `0384-*`.
(2026-08-02)

Recently resolved: **#379** — no lane ran clippy on the `packages/cli` sub-workspace, so 107 lints
(grown from ~30) had accumulated. RESOLVED (2026-08-02): added the `check-cli-clippy` lane (in
`check-build`) and cleared all 107 — `--fix` for the mechanical class, hand-fixes for the rest
(incl. `result_large_err` → boxed error, `type_complexity` → alias, `unexpected_cfgs` → registered
the retired cfg), two documented `too_many_arguments` allows. Flagged a follow-up: the
`scripts/bin/cargo` shim appends `--locked` at argv tail, breaking `cargo <sub> -- <args>`. See
`0379-*`. (2026-08-02)

Recently resolved: **#376** (filed as #372; renumbered — id collision with parallel sessions) — zpico
multi-session (phase-328 / #348) WAS **pubsub-only**: the Rust RMW shim's `SERVICE_BUFFERS` +
`REPLY_WAKERS` were process-global arrays with no session dimension, so two sessions' service
servers/clients collided (async-client cross-wake = lost wakeup). RESOLVED: both arrays flattened by
the session's pool index (`ZPICO_MAX_SESSIONS * K`, indexed `session_index * K + slot`) via a new
`zpico_session_index()`; the reply-waker callback gained a leading `session_index` arg. Identical at
`ZPICO_MAX_SESSIONS=1`; verified 15/15 at pool=2. See `0376-*`. (2026-08-01)

Recently resolved: **#370** (filed as #368; renumbered — fifth id collision between parallel sessions) — zephyr fixture family broken after `just setup-cli` on current main: `nros codegen entry`
rejects ws-realtime-c's committed system model with "places no nodes on board `zephyr`" — the
rebuilt CLI (ros-launch-resolve line, RFC-0060) and the committed model disagree about
execution.deploy targets, so `build-test-fixtures` fails and tier-1 ci can't reach fixture
freshness. Same shape as archived #0361. See `0370-*`. (2026-08-01)

Recently resolved: **#381** (filed as #371; renumbered) — plan/launch tests asserted the deleted
pre-296 launch-XML parse path, skip-hidden by `play_launch_parser_available()` gates. RESOLVED
(2026-08-02): launch_synth — removed the 2 resolver-owned tests (synthesis, launch-file precedence),
re-pointed Path-A refusal to the "no committed SystemModel" contract; workspace_dirwalk — stage a
committed `config/system_model.yaml` + assert the discovery signal; orchestration_includes — kept
the `--record` chain test, removed the cycle + depth-cap tests (moved to ros-launch-resolve).
self_bringup was NOT broken (live `synthesise_self_model`). All four files now RUN + pass; the dead
gates are gone. See `0381-*`. (2026-08-02)

Recently resolved: **#367** — RESOLVED: the `nros ws sync` ghost (phase-265 renamed the verb to `nros sync` but a hidden
`cmd/ws.rs Sub::Sync` alias, the misnamed `nros_require_ws_sync` helper, and hundreds of stale prose refs
survived). Deleted the alias (`nros ws sync` now errors), renamed the helper, swept every active code +
user-facing-doc ref to `nros sync` (log prefixes, just recipes, book, `pr-checks.yml` job name, scripts,
examples — historical records left intact), and added a class gate (`RETIRED_SPELLINGS`, `\bws sync\b`) to
`check-retired-submodule-refs.sh` (in `check-fast`) so it can't creep back. See `archived/0367-*`. (2026-08-01)

Recently resolved: **#369** (filed as #368; renumbered) — Mixed C+C++ workspace fixture link-fails: the variant anchor suffixed by cargo FEATURE SPELLING diverged between the two nros-c builds of a mixed workspace though their SIZES agreed. Fixed: the anchor now hashes the header's own size values (`sz_<fnv64>`) and the archive def is WEAK, so agreeing builds merge and a different-sized header still fails to link. Verified on the exact failing command. See `archived/0369-*`. Original finding: `libstd_msgs__nano_ros_c.a` references
`nros_config_variant_alloc_..._rmw_cffi_rmw_zenoh_ros_humble_std`, but `nros-c` (built for that workspace
with `cffi-zenoh-cffi` → `rmw-zenoh`, NOT `rmw-cffi`) never emits it. The C-side msg codegen stamps its
variant from the WORKSPACE feature union (which carries `rmw-cffi` via the C++ half's `rmw-zenoh-cffi`),
while the linked `nros-c` staticlib carries only its own features → undefined reference. Same variant-
consistency class as #360, unreconciled for the mixed workspace where C/C++ halves pick different rmw
spellings. Fix: compute the C-side variant reference from nros-c's OWN features, not the union. Blocks
`native,mixed,*` fixtures. See `0369-*`. (2026-08-01)

Recently resolved: **#348** — zpico supports only ONE zenoh session per process: `g_session` plus 51 file-scope
statics (every registration table) are process globals, and the ~38 `zpico_*` entry points take no
session handle, so multi-session means a breaking ABI change across 51 consuming files. Split out of
#347, which made the second open FAIL loudly rather than silently wiping the first session. Nothing
in-tree needs it — the bridge workspaces pair zenoh with a DIFFERENT backend — so this is a
capability gap, not a live break. **RESOLVED by phase-328 (2026-08-01):** Option 3 (full
handle-passing) — per-session `struct zpico_session` from a compile-time pool (`ZPICO_MAX_SESSIONS`,
default 1), every `zpico_*` takes a handle; single-session footprint +142 B `.bss`. See `0348-*`.
