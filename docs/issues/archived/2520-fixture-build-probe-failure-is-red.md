---
id: 2520
title: "A probe failure that leaves an image on default sizes is only a WARNING, so a fixture build stays green while it ships the wrong sizes"
status: resolved
resolved_in: "the change that archives this issue"
type: bug
severity: medium
area: [sizing, tooling, testing, build]
related: [issue-2519, issue-1593, issue-2097, issue-1758, phase-559]
found: 2026-11-14
---

**Home: the C/C++ metadata probe (`orchestration/metadata_refresh.rs`) and the
fixture builders (`scripts/build/{fixtures,workspace-fixtures,compile-check-fixtures}-build.sh`)**

## Symptom

On main, the tier-2 nightly on 2026-11-12 and on 2026-11-13 built
`riscv64_threadx_c_typed_carrier` and `nuttx_c_typed_carrier` with

```
sync: source metadata — no producer for … (probe failed at this source last sync; …)
nros build:   resolved → … (entity counts not derived; configured defaults kept)
```

The lane was green both nights. The images ran on DEFAULT pool sizes, not
sizes derived from what the components declare. Issue 2519 fixed the cause
of that probe failure (a probe inheriting a cross configure's `CC`). It also
turned the failure from a cached quiet line into a loud WARNING. A warning in
a CI log is still green, and green is the only thing a nightly reports. So the
next cause of a whole-project probe failure will ship the same way.

## Why a warning is right for one caller and wrong for the other

- **An interactive `nros build` / `nros configure`.** A developer reads the
  warning, and refusing would block a build that does run. Keep the warning.
- **A fixture build.** It is the evidence that an image was built as its
  declarations say. An image on default sizes is not the image the fixture
  claims to be, and nobody reads the log of a green lane. This is issue 1758's
  rule one layer down: a missing precondition is a red, never a pass that
  prints something.

## Proposal

1. Add a declared strictness for the probe:
   `NROS_PROBE_FAILURE = warn | refuse`, default `warn`. Register it in the
   knob registry so it has one spelling (`knobs.toml`, `scripts/gen-knob-registry.py`).
   - A whole-project configure failure under `refuse` exits non-zero. The
     refusal names the batch and CMake's error (`first_error_line`), and says
     that the image would otherwise be built on default sizes.
   - Per-component failures keep their marker. Under `refuse` they also
     refuse (see the ruling below).
2. Every fixture builder sets `refuse`: `fixtures-build.sh`,
   `workspace-fixtures-build.sh` and `compile-check-fixtures.sh`. It is
   DECLARED by the lane, never inferred from `CI` or from a TTY, so that a
   local run of the same lane is as red as the remote one.
3. Sweep the other "configured defaults kept" paths in the same change, not
   only the probe batch, so the class is fixed rather than one site. Each place
   where a resolve proceeds with `entity counts not derived` or
   `message bounds not seeded` asks the same strictness. A fixture row that
   legitimately has no inventory (the `entity-census take … nothing to take`
   rows for an owned entry with no component) must say so in its manifest row,
   so that `refuse` is not answered by a blanket exemption.
4. Gate: a selftest that runs a fixture builder against a probe project that
   cannot configure, and asserts a red with the reason in its output. It is the
   negative control for step 2.

## Ruling on per-component failures

Per-component probe failures also refuse in a fixture build, under the same
policy (manager, 2026-11-14; landed with issue 2608).

## Ruling on step 3: refuse on the REASON (2026-11-19)

This replaces both earlier placements of the no-inventory statement: the
fixture manifest row (2026-11-14), and then `system.toml` as a required
statement (2026-11-19, the same day). Both were the manager's calls, not the
maintainer's.

Measured before ruling, from the real fixture builds of 2026-11-18 (nightly
44242045728 and run-matrix 44250365361): about 170 resolved images keep
configured defaults. The ruling first read them as ordinary examples that
declare no `*.contract.yaml`. That was wrong (corrected 2026-11-19, measured):
on main every image of the canonical workspaces derives, from the metadata
probe's sidecars, contract or not; in those CI jobs every sidecar was read as
stale, which is issue 2720. The ruling's rule does not rest on that reading:
an image that genuinely declares nothing keeps the designed defaults, and a
rule such an image trips by design is not the rule wanted. So the verdict
follows WHY the resolve kept defaults, as a typed value the resolve returns,
never a match on its message:

- nothing declared (no wiring and no parameters, an owned entry with no
  model, no interface dependency): a note, at every policy;
- declared but not derivable (a launch or model that does not resolve, a
  contract that does not compose, a bound table the probe never generated):
  a warning under `warn`, and under `refuse` an error that names what to fix.

`[image.<id>] no_inventory = "<reason>"` is optional. It records why an image
declares nothing, in place of the note. It needs a non-empty reason. It is
refused on an image that does derive an inventory, and it never excuses a
failure.

One failure is tolerated by name: the metadata probe generates no bound table
for `action_msgs` / `builtin_interfaces`. It is keyed on the exact 14
`(image, package)` pairs measured, tracked as issue 2721, and warns until that
lands. Any other image or package with the same reason refuses.

## Partly done (2026-11-16, phase-559 W11)

Steps 1 and 2 landed with [issue 2608](2608-metadata-probe-failure-reads-as-no-producer.md),
widened by the manager: `NROS_PROBE_FAILURE=warn|refuse` (a CLI environment
setting, not a knob-registry row: the registry holds build knobs with Kconfig
and TOML spellings, and this is neither), `refuse` in the three fixture
builders, every `just` lane and the release dry run. Per-component failures
refuse too, per the ruling above.

Open: step 3 (the other "configured defaults kept" paths ask the same
strictness, with a no-inventory statement per manifest row) and step 4's
gate (a fixture builder run against a probe project that cannot configure).

## Fix (step 3, 2026-11-19)

- `resolve::NotDerived { NothingDeclared, Failed, MissingBoundTable }` is
  built where the resolve decides it. `resolve_image`'s composition and
  every message-bound seed producer (`write_west_seed_over`,
  `write_leaf_west_seed`) return it. A bare reason converts to `Failed`, so
  only a site that knows nothing was declared can say so.
- `resolve::report_not_derived` is the one reporter, reached from
  `resolve_image` and the five seed sites (`generate_entry`,
  `seed_hand_written_west_bounds`, `resolve_rust_west_image`,
  `resolve_coordinate`, `configure_zephyr_leaf`). Each of those now returns
  `Result`, so a refusal stops the plan.
- `resolve::KNOWN_BOUND_TABLE_GAPS` holds the 14 pairs.
- `ImageBlock::no_inventory`, documented in the `system.toml` reference.

Tests:
- `cmd::build::not_derived_verdict_tests`, six cases. Four are red with the
  fix neutralised; the two positive cases pass both ways by design.
- `resolve::not_derived_tests`: the known-gap exemption is exactly the pair.
- `a_leaf_that_states_no_entities_gets_no_seed` now asserts the kind.

Step 4's gate (a fixture builder run against a probe project that cannot
configure) is `check-workspace-probes`, which landed with issue 2608.

What the measurement exposed is issue 2720: in the nightly's fixture jobs
every metadata sidecar reads stale, so those images lose the probe's counts.

Sweep: `git grep -n 'message bounds not seeded\|configured defaults kept\|NotDerived' -- packages/cli`
