# Phase 534 — Follow rlm's standard-names renames (v0.3.6 to v0.3.8, then model schema 4)

**Status (2026-11-04). OPEN — W1, W2 and W4 done (rlm `v0.3.8`, play_launch 0.25.0); W3 (model schema 4) not started. Raised by the play_launch side so nano-ros knows which names are moving, and where nano-ros is on the critical path of the next step.** nano-ros pins rlm `v0.3.8` (was `v0.3.5`) and play_launch 0.25.0, the release carrying it.

The design of record is rlm's
[`docs/design/standard-names.md`](https://github.com/NEWSLabNTU/ros-launch-manifest/blob/v0.3.8/docs/design/standard-names.md)
(rlm design issue 57, play_launch phase 98). The rule behind it: where a
safety or real-time standard has a word for one of our concepts, the
contract uses that word. Section 6 of that document is the release
schedule every rename follows, and this phase is nano-ros's part of it.

Depends on: nothing. **Blocks:** rlm's R+1 release (the one that refuses
the old contract keys) ships only after nano-ros has merged the bump to
the release that introduced them (§6: "R+1 ships only after a release
carrying R has been tagged and published to PyPI and nano-ros has merged
the matching pull request"). W1 is therefore the item on that path.

---

## What moved

### Already refused (rlm v0.3.6, phase 98 N4)

The names that rlm v0.3.3 renamed (merged here as #3046) are now parse
errors naming the new key: `severity_levels:` (now `integrity_levels:`),
a hazard's `severity:` (`integrity:`), a mode's `window:` (`min_dwell:`)
and a profile's `severity_scale:` (`integrity_scale:`). `migrate` still
rewrites them. An old rule id in a contract's `acknowledged:` is
`error[ack-renamed]` naming the new id. rlm also gained a `check-terms`
gate that keeps the retired words out of its user-facing text.

The MODEL is unaffected: `SystemModel::from_yaml_str` still rewrites the
schema-2 names on read, permanently.

### Renamed at release R, old spelling still read (rlm v0.3.7 and v0.3.8)

Contract keys (v0.3.7). The old spelling still parses, with one
`deprecated-spelling` warning naming the new key, and `play_launch
migrate` rewrites it:

| Old | New | Standard term |
|---|---|---|
| `hazards.<h>.guards` | `hazards.<h>.monitors` | monitored signals (AUTOSAR WdgM, ISO 26262-6) |
| `sub.<e>.ingress: { rate_hz, burst }` | `sub.<e>.arrival_curve: { rate_hz, burst }` | arrival curve (network calculus) |
| `on_demand: true`, `trigger: spontaneous` | `aperiodic: true`, `trigger: aperiodic` | aperiodic task |
| platform file `mapper:` | `priority_assignment:` | priority assignment policy |

Rule ids (v0.3.8). An old id in `acknowledged:` still matches, with a
warning:

| Old | New |
|---|---|
| `fault-reaction-budget` | `fault-reaction-time` |
| `degradation-level-budget` (and the older `ladder-rung-budget`) | `degradation-level-time` |
| `budget-overflow` | `latency-bound-overflow` |

"Budget" now means only a reservation's CPU time (`budget`,
`budget_period`); a time LIMIT is a deadline or latency bound.

**The model's field names did not move.** `HazardContract.guards`
(`GuardContract`), `SubContract.ingress` (`IngressContract`),
`PubContract.on_demand` and the `EffectiveTrigger::Spontaneous` variant
are unchanged in rlm v0.3.8, because nano-ros reads them. Renaming them
is W3.

### Not decided

M7, `tier` (`TierDef`, `ResolvedTier`) to "scheduling context" or "task",
is the costliest rename and is undecided. If it is taken it gets its own
phase on both sides.

## What nano-ros uses today

Surveyed on `main` at `4fc4af1fdb`:

- **Authored contracts:** none in the tree use an old key. Every
  `ingress:` in a Rust test string (`entity_inventory.rs`,
  `sizing_descriptor.rs`, `router_rule.rs`, `interop_e2e.rs`) is MODEL
  YAML (`contracts.sub_endpoints.<e>.ingress`), whose name has not changed.
- **Model field reads (W3's scope):**
  `codegen/entry/lower.rs` (`.guards`),
  `nros-orchestration-ir/src/contract_monitors.rs` and `ftti_budget.rs`
  (`h.guards`), `entity_inventory.rs` and `router_rule.rs`
  (`c.ingress`, `IngressContract`).
- **Rule ids:** `nros-orchestration-ir/tests/fixtures/derived_settle/{derived,written}.yaml`
  carry `[fault-reaction-budget]` in `meta.diagnostics`. That is
  free text copied from a resolver run; a regenerated fixture prints
  `[fault-reaction-time]`.
- **Design docs:** RFC-0074 shows `ingress: { rate_hz: 200, burst: 4 }` in
  a contract, RFC-0079 shows `mapper: rate_monotonic`, RFC-0050 lists
  `budget-overflow`.
- **nano-ros's own names:** the monitor spec's `guards`
  (`executor/monitor.rs`), the C ABI's `guards`/`n_guards`
  (`nros-cpp/src/lib.rs`), `rmw_subscription_options_t::ingress_rate_mhz`
  and the sizing descriptor's ingress fields are nano-ros vocabulary. rlm
  does not ask for them to move. Whether they follow is nano-ros's call;
  W3 should decide it once rather than leave a mix.

## Work items

### W1 — Bump rlm to v0.3.8

- `just`-level bump of the three `ros-launch-manifest-*` pins in
  `packages/cli/nros-cli-core/Cargo.toml` from `v0.3.5` to `v0.3.8`, with
  the board lockfiles and ledger following, as #3046 did for v0.3.3.
- Expected compile impact: none. v0.3.6 to v0.3.8 add a
  `#[serde(skip)]` field to the contract `Manifest`, a `refused` field to
  `RenamedRule`/`RenamedKey` in `types::rule_names`, and
  `priority_assignment` beside `mapper` in the sched crate's raw platform
  file plus a `spelled_mapper` field on `PlatformFile`; nano-ros
  constructs none of these.
- Regenerate the two `derived_settle` fixtures if a test compares their
  diagnostics; otherwise leave them as the record of the run that made them.

Acceptance:

- nano-ros `main` pins `v0.3.8` and its CI is green.
- Tell the play_launch side when it merges: rlm's R+1 waits on it.

**Done (2026-11-04).** `just bump-manifest v0.3.8` (the four manifests,
the twelve lockfiles following), and
`packages/cli/third-party/play_launch` to its 0.25.0 release commit
`912c7574`. No code change: nano-ros compiles unchanged against v0.3.8.
The `derived_settle` fixtures keep `[fault-reaction-budget]`: no test
compares their `meta.diagnostics`, so they stay the record of the run that
made them.

### W2 — The design docs say the new words

- RFC-0074: the contract example writes `arrival_curve:`; add a status
  line that the key was `ingress:` before rlm v0.3.7. The RFC's own name
  for the concept ("ingress budget") is nano-ros's, so keep or change it
  deliberately.
- RFC-0079: `priority_assignment: rate_monotonic`, with the same status
  line.
- RFC-0050: `latency-bound-overflow`.
- History (archived phases, closed issues) keeps the words of its time.

Acceptance: no active design doc or book page shows a contract or platform
file spelled with a key rlm now warns about.

**Done (2026-11-04).** RFC-0074 writes `arrival_curve:` in every contract
example and has a status line (the key was `ingress:`; the RFC's "ingress
budget", the model field and `ingress_rate_mhz` are unchanged, W3 decides
them). RFC-0079 writes `priority_assignment:` with a status line ("mapper"
in prose names the algorithm). RFC-0050 says `latency-bound-overflow`.
`git grep` over `docs/` and `book/` outside the archives finds no other
contract or platform file spelled with an old key.

### W3 — Model schema 4: rename the mirrored fields together with rlm

rlm renames the model fields in one release, with a nano-ros PR bumping to
it (standard-names §5, step 5). Proposed, to be agreed here before rlm
tags it:

| Model today | Schema 4 |
|---|---|
| `HazardContract.guards: Vec<GuardContract>` | `monitors: Vec<MonitorContract>` |
| `SubContract.ingress: Option<IngressContract>` | `arrival_curve: Option<ArrivalCurve>` |
| `PubContract.on_demand: bool` | `aperiodic: bool` |
| `EffectiveTrigger::Spontaneous` (sched) | `EffectiveTrigger::Aperiodic` |
| `PlatformFile.mapper` (sched, resolved) | `priority_assignment` |

rlm keeps reading a schema-3 model: `SystemModel::from_yaml_str` rewrites
the old field names before the typed read, as it did for schema 2 and 3,
so a saved model and every model-YAML test string here keep loading. A
model rlm writes after the bump uses the new names, so anything in
nano-ros that reads model YAML WITHOUT going through `from_yaml_str`
(CMake readers such as `NanoRosEntityInventory.cmake`,
`NanoRosSchemaReader.cmake`, Python tooling) must accept both names for
one release.

- Rename the field reads listed above.
- Decide whether nano-ros's own `guards`/`ingress` names follow (see
  above) and record the decision in the relevant RFC.
- Audit the non-Rust model readers for `guards`, `ingress`, `on_demand`.

Acceptance:

- nano-ros builds and passes CI against the rlm tag that carries schema 4.
- A schema-3 model and a schema-4 model of the same system produce the
  same build artifacts (sizing descriptor, monitor tables).

### W4 — Nothing authored needs the R+1 refusal

At rlm's R+1 the old contract keys and rule ids above become errors.
nano-ros has no authored contract or platform file using them today
(survey above); W4 is re-running that survey just before the R+1 bump so
a contract added meanwhile does not break on it.

Acceptance: `git grep` for `guards:`, `ingress:`, `on_demand:`,
`trigger: spontaneous` and `mapper:` in authored contracts and platform
files (not model YAML) returns nothing.

**Survey (2026-11-04, W1 branch).** No authored contract or platform file
uses an old key or rule id. The only YAML hits are the two
`derived_settle` fixtures (model YAML: `contracts.hazards.<h>.guards`, and
`[fault-reaction-budget]` in `meta.diagnostics`). The Rust hits are model
YAML in test strings (`entity_inventory.rs`, `sizing_descriptor.rs`,
`contract_join.rs`, `codegen/entry/emit/mod.rs`, `nros-macros`
`main_macro.rs`) or nano-ros's own names (`nros-cpp` `guards`/`n_guards`,
`nros-entry-lower` `hazard_guards`). Two `entity_inventory.rs` messages
tell the user a subscriber "declares `ingress: { rate_hz, burst }`"; they
describe the model's field and move with W3. Re-run just before rlm's R+1
bump.
