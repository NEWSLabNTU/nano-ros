---
id: 439
title: "A lane-narrowed build kills any recipe that names a fixture by `--id`, so `just ci-matrix` cannot run"
status: resolved
type: bug
area: build
related: [issue-0393, issue-0406, phase-337]
---

## Symptom

`just build-test-fixtures lane=tier2` fails three of its eight modules, each the
same way:

```
fixtures: id 'threadx-riscv64-logging-smoke' is a fixture, but not for platform=threadx-riscv64 lang=rust.
          It is declared for:
            platform=threadx-riscv64 lang=rust rmw=–
fixtures: id 'threadx-linux-logging-smoke'   … not for platform=threadx-linux lang=rust.
fixtures: id 'freertos-logging-smoke-mps2'   … not for platform=freertos lang=rust.
```

Read the first one twice: the requested coordinates and the declared ones are
IDENTICAL. The message says the row is not for the coordinates it is plainly
declared for, which is what makes this so hard to act on.

Because the build dies, no `.fixtures-built` stamp is written, so
`just ci-matrix` fails its `_lane-gate` before running anything. **Tier 2 is
currently unrunnable.**

## Root cause — two guards that each behave correctly alone

`fixtures-build.sh` builds its row set with

```sh
python3 scripts/build/fixtures-manifest.py list \
    --platform "$platform" --lang "$lang" ${rmw:+--rmw "$rmw"} \
    ${fixture_id:+--id "$fixture_id"} "${coords_args[@]}"
```

* **issue 0393** added `--coords-from` (`NROS_FIXTURE_COORDS`), narrowing a build
  to one CI lane's `platform,lang,rmw` coordinates.
* **issue 0406** added: an explicit `--id` that matches zero rows is a WRONG
  INVOCATION, not an empty sweep — diagnose it instead of exiting 0 silently.

Both are right in isolation. Together they are wrong, because 0406's premise
("the id matched nothing, so the caller mistyped it") stops holding once 0393 can
remove rows for a reason that has nothing to do with the caller.

Concretely: tier 2's coordinate for that platform is
`threadx-riscv64,c,cyclonedds`, and the logging-smoke row is `lang=rust` with no
rmw. The lane filter drops it, the record set is empty, and 0406's guard fires on
an invocation that was entirely correct. The recipe
(`just/threadx-riscv64.just:153`) hard-codes `--id threadx-riscv64-logging-smoke`
and cannot know which lane it is running under.

The misleading message follows from the same confusion: the guard prints the
row's declared coordinates and the caller's requested ones, which match, because
the thing that actually excluded the row — the lane — is not in either.

## Why only now

The three failing recipes are `--id` calls; every other row arrives through an
unnarrowed sweep. And `NROS_FIXTURE_ID` (the env-var form the workspace and
compile-check builders read) already returns 0 for this case in
`nros_fixture_id_no_match` — only the `--id` FLAG path is fatal. So the bug needs
a lane build AND a flag-narrowed recipe, which is `lane=tier2` /
`lane=tier2-nightly` and nothing else. `lane=all`, `lane=native` and `lane=tier1`
are all unaffected, which is why phase-337 never hit it until it tried to satisfy
its own `just ci-matrix` acceptance line.

## Fix shape

Keep both guards; teach the fatal one to tell the two cases apart. When the
narrowed query returns nothing AND a coords file is in play, re-run the SAME
query without `--coords-from`:

* row exists un-narrowed ⇒ correct invocation, out-of-lane row ⇒ say so and
  exit 0, exactly as the `NROS_FIXTURE_ID` path already does;
* row absent even un-narrowed ⇒ the genuine 0406 typo ⇒ diagnose and exit 2.

That restores 0406's guarantee (a mistyped id is never silent) without letting it
claim a lane's own narrowing is the caller's mistake.

## Related

- issue 0393 — lane-scoped fixture builds (`--coords-from`).
- issue 0406 — explicit `--id` matching nothing must not exit 0 silently.
- phase-337 — its `just ci-matrix` acceptance criterion is blocked by this.

## Resolution

Landed in `9c6420144` as the fix shape above describes; the issue was left open.
Verified 2026-08-06 against the exact three invocations reported, with
`NROS_FIXTURE_COORDS` pointing at `lane-coords tier2`
(`freertos,mixed,zenoh` / `threadx-linux,c,cyclonedds` /
`threadx-riscv64,c,cyclonedds`):

```
$ bash scripts/build/fixtures-build.sh threadx-riscv64 rust --id threadx-riscv64-logging-smoke
fixtures: id 'threadx-riscv64-logging-smoke' is not in this lane's coordinates;
          this threadx-riscv64/rust stage builds nothing.
RC=0
```

All three exit 0, same message shape. `nros_fixture_id_out_of_lane` lives in
`scripts/build/fixture-id-guard.sh` and is shared by both flag-narrowed builders
rather than inlined twice — the right call, since a second spelling of a
reconciliation between two guards is how the two guards diverged in the first
place.

0406's guarantee is intact in BOTH modes — a mistyped id is still fatal whether
or not a lane is active:

```
$ … --id threadx-riscv64-logging-smoek        # lane active
fixtures: no row anywhere carries id 'threadx-riscv64-logging-smoek'.
RC=2
$ … --id threadx-riscv64-logging-smoek        # no lane
RC=2
```

That second case is the one worth having checked. The fix works by re-querying
without the lane filter, so the failure mode to fear was a guard that had become
lenient generally rather than lenient about lanes specifically.
