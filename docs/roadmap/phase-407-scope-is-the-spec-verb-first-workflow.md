# phase-407 — the scope you name IS the specification

**Status (2026-08-31). Design agreed; W1–W4 open.**

Implements the workflow half of RFC-0061's tier discipline. Sibling of
phase-399 (the justfile surface) and phase-395 (the CI event design); it does
not supersede either, it makes their vocabularies agree.

## The defect

A test that cannot run skips, and a skip is indistinguishable from a pass.

That is CORRECT for the local dev cycle and must stay: a developer working on
Zephyr does not provision FreeRTOS, NuttX, ThreadX and ESP-IDF, and a run that
demanded all of them would be a run nobody performs. The skip is what makes a
focused checkout usable.

The caveat is the whole issue: **when someone EXPECTS a platform to be covered
and the prerequisite is missing, the same mechanism reports success.** Forget to
provision, forget to build the fixtures, and the tests skip and the lane is
green. Nothing distinguishes "I did not ask for FreeRTOS" from "I asked for
FreeRTOS and it silently did not happen".

Measured on 2026-08-31, three instances of one shape:

* `just zephyr build-fixtures` on an unprovisioned host calls
  `nros_lane_skip "ZEPHYR_WORKSPACE not set up — run \`just zephyr setup\`"` and
  exits 78. **The user typed `zephyr`.** There is no lane ambiguity to resolve
  and the remedy is already in the message; it is simply not a failure.
* `nros_fixtures_stamp_write` records `nros_lane_coords_file "$lane"` — the
  lane's NOMINAL coordinates from the manifest, not what the build achieved. A
  platform that skipped still appears in the stamp as covered, so
  `_require-fixtures` answers "yes, covered" and the run proceeds to skip its
  tests. **The skip is laundered into a coverage claim** that every downstream
  consumer then reads instead of reality.
* `host-tests.yml` sets `NROS_FIXTURES_OPTIONAL=1` unconditionally in CI, which
  converts absent fixtures to skips wholesale. Its own comment notes the full
  tier "leaves the var unset and still hard-fails" — which makes correctness
  depend on remembering to unset a variable.

This is the same shape as three defects already fixed this week, one level up:
`check-submodule-pins` skipping silently on an unresolvable baseline,
`check-feature-set-ssot` matching a spelling no site used, and `post-submit`
reporting success with its only expensive job skipped. A lane that reports
success for work it did not do is a gate that reports OK for a comparison it
did not make.

## The rule

> **Named → must work. Unnamed → may skip, and is always reported.**

Naming IS the specification. There is no second declaration to keep in step —
which is the trap an earlier draft of this design fell into: it added an
"expected coverage" statement beside the selection, i.e. two sources of one
fact, the exact antipattern phase-405 spent a week deleting.

Skip legitimacy reduces to one question: **was this platform NAMED, or merely
INCLUDED by a preset or the default?**

The default scope is DERIVED, never recorded: "what I have provisioned" is a
probe (`doctor` already performs it per item, with remedies), so it cannot drift
from a stored file.

## Surface: `just <verb> [scope…]`

One vocabulary in one argument position.

| | today | after |
| --- | --- | --- |
| provision | `just setup zephyr` | `just setup zephyr` (unchanged) |
| readiness | `just doctor tier=all` | `just doctor zephyr` |
| fixtures | `just build-test-fixtures lane=tier2` | `just build tier2` |
| test | `just zephyr test` | `just test zephyr` |

`just setup <platform>` is ALREADY verb-first — the platform modules are the
inconsistent half, and this makes the rest follow the half that was right.

Modules do not disappear; `zephyr::build-fixtures` remains as implementation.
What changes is that the DOCUMENTED surface is verb-first, so there is one shape
to learn rather than two.

**Scope is one namespace.** Platform names and preset names occupy the same
position. `native` is already both a platform module and a lane name — they must
denote the same scope, and a gate must assert that no preset name collides with
a platform meaning something else.

## CI is the user workflow, with a different scope

```yaml
      - uses: actions/checkout@v4
      - run: just setup tier2
      - run: just test  tier2
```

A CI job becomes `setup <scope>` then `test <scope>`. **Only the scope differs
between lanes.** The workflow file reads as a transcript of a user session, so
"does CI do what I do?" is answerable by looking at it.

Measured baseline: `pr-checks.yml`'s `check` job is 16 steps, NINE of which
re-source `activate.sh`, plus a hand-rolled submodule init, a hand-rolled
`cargo build --bin nros`, six `nros setup --source` flags, and a separate
fixture-build step. `host-tests.yml` adds `NROS_FIXTURES_OPTIONAL=1`.

What absorbs it:

| boilerplate | absorbed by |
| --- | --- |
| `source ./activate.sh` ×9 | recipes self-activate |
| submodule init, CLI build, `nros setup --source …` | `just setup <scope>` |
| separate fixture-build step | `just test <scope>` |
| `NROS_FIXTURES_OPTIONAL` | deleted — its job was tolerating the mismatch |
| `lane=` ↔ platform translation | one scope vocabulary |

Runner tuning (`NROS_BUILD_JOBS`, `RUSTFLAGS`, job counts) STAYS in `env:`. That
is environment, not boilerplate, and pretending otherwise would hide the one
thing a runner legitimately configures.

## Work items

**W1 — the stamp records what was ACHIEVED, not what was REQUESTED.**
`nros_fixtures_stamp_write` writes the lane's nominal coordinates. It must write
the coordinates whose artifacts exist. Then `_require-fixtures` starts failing
with "you asked for threadx, the build did not produce it" instead of letting
the run skip. Smallest change, and it makes every existing over-claim visible
immediately — including a tier-2 run in flight while this doc was written.

**W2 — a NAMED platform never skips.** ~12 `nros_lane_skip_note …; exit 0` sites
across the platform lanes gain the named/included distinction. Named and
unprovisioned is a FAILURE, and the failure text is the remedy those sites
already print. `nros_lane_skip` keeps its job for a platform the spec did not
name — under a precise spec that should be unreachable, so it degrades to a
caller-bug signal rather than a routine outcome.

**W3 — `just <verb> [scope…]`.** Verb-first surface, preset expansion to
platform sets, `just <plat> <verb>` kept as a deprecated alias for one release.
`just test`'s first positional is currently `verbose`, so its signature changes;
that is a real incompatibility for anyone typing `just test 1`.

**W4 — CI as transcripts.** Only safe once W1–W3 hold, because the YAML stops
carrying the compensating boilerplate. `NROS_FIXTURES_OPTIONAL` is deleted here.

## Accepted consequences

* **`lane=all` changes meaning** — today "everything, minus whatever is
  missing"; after, "every platform, fail if unprovisioned". The best-effort
  meaning moves to the bare `just test`, which is what a developer actually
  wants. This is the change most likely to annoy, and the reason the list forms
  must be easy to type: **if the narrow spec is awkward, people reach for a
  bypass and the gate dies.**
* **`just test 1` breaks** (verbosity moves to a flag).
* **Self-activating recipes** need a prototype before commitment —
  `activate.sh` is the env SSoT and interacts with the `scripts/bin/cargo` shim
  and `NROS_CARGO_FLAGS`.
* **CI does more visible work in `setup`** — same total, one name.

## Acceptance

* A named-but-unprovisioned platform FAILS, naming the prerequisite, at the
  point of decision — not twenty minutes later as a missing artifact.
* The stamp cannot claim a coordinate whose artifact does not exist.
* Every build/test run prints a coverage line: scope, what ran, what did not,
  and how to provision the rest.
* A CI job for any lane is `setup <scope>` + `test <scope>`.
* Gate: no preset name collides with a platform name meaning something else.
* `just ci l1`, `just check fast` and every lane preset NAME are unchanged.
