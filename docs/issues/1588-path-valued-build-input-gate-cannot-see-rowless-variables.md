---
id: 1588
title: "The build-input path gate keys on a variable having a row, so a
  path-valued name with no in-repo default is outside its subject by
  construction"
status: open
type: tech-debt
area: build
severity: low
found: 2026-09-29
related: [0196, 1280, 1452, 1527, 1558, 1560, phase-471, RFC-0101]
---

## What this is

`check-build-script-path-resolution` (phase-471 W3/W6) asks: does a build script
or build-script library that resolves a **path-valued SDK variable** go through
`nros_build_paths`? Its POPULATION is derived — build scripts plus the
build-script libraries reachable from their `[build-dependencies]`. Its
SUBJECT is not: the set of path-valued variables comes from
`just/sdk-env.just`'s exports plus the board descriptors' `cargo_config [env]`
rows, both read by `scripts/nros-build-wiring.py`.

That subject is an authored list one level down. A variable is in it because
somebody wrote a row for it, and two path-valued build inputs have no row and
can have none:

| variable | why it has no row |
| --- | --- |
| `ZENOH_PICO_DIR` | names a user's own CMake install prefix; there is no in-repo default to export |
| `NV_SPE_FSP_DIR` | the NVIDIA Orin SPE FSP ships under an SDK-Manager EULA and can never be vendored |

Issue 1560 fixed both call sites. Nothing stops the next one.

## Why the obvious fix is the wrong one

Issue 1560's remedy offered two directions and asked for a decision rather than
a drift. **Requiring every path-valued build input to have an `sdk-env.just`
row is rejected**: it has no answer for `NV_SPE_FSP_DIR`, and rooting the gate
more firmly in a hand-authored field re-creates one level up the problem the
gate exists to answer — issue 1452's finding, that a check is only as complete
as whoever wrote its subject.

**The chosen direction is to widen the subject from "a variable with a row" to
"a name whose value is used as a path"**, which is derivable from the source
and needs no list.

## The discriminator already exists in this gate

The gate solved the same problem one level down. A private helper takes the
variable NAME as an argument, so no literal match can tell which variables it
resolves — and phase-471 W6 did not answer that with a list of helper names. It
asked the TYPES: `PATH_TYPED`, because *a path resolver says so in its types*.
That is the same question at variable scope: an `env::var("X")` whose value
reaches a `PathBuf`/`Path` is a path input, whatever rows exist for `X`.

## Why it is filed rather than written

Writing it now would be the change phase-471 W2 refused to make: a gate whose
reach nobody has measured. The two things to measure first, in this order:

1. **The false-positive rate of the type-flow test.** The gate already carries
   a scar here — widening the population to build-script libraries reported
   `declared_fact(name: &str) -> Option<String>` and `declared_floored(name:
   &str) -> Option<usize>` in `nros-zpico-build`, RFC-0049 COUNT knobs with
   nothing to do with issue 1280. A variable-scope version is more exposed, not
   less, because `env::var("X")` and the `PathBuf` it flows into are often not
   in the same statement.
2. **What a rowless path variable's correct answer even is.** Both known ones
   name a tree OUTSIDE every nano-ros checkout, which is the arm
   `reroot_foreign` deliberately leaves alone. So the gate would be enforcing a
   rule about the CALL whose VALUE arm is a no-op today — worth doing (issue
   1560 says why: a rule with a "when it would not have mattered anyway" arm
   cannot be checked) but not worth false reports.

## Acceptance

The gate's subject is derived from the source rather than from a row, it
reports `ZENOH_PICO_DIR` and `NV_SPE_FSP_DIR` if their call sites regress, and
its self-test carries a negative control for the false positive in (1) above —
a non-path `env::var` of a string parameter must NOT be reported.

## Related

* Issue 1560 — the four sites, and the decision this records.
* Issue 1452 — a reach WIDER than the rule, and why deriving the subject is the
  remedy in both directions.
* RFC-0101 D3/D4 — the rule, and the `sdk-env.just` row as one carrier of it.
