---
id: 611
title: "West fixture reuse never refreshed `.compile-ok`, so after any CLI rebuild the consumer rejected the fixture PERMANENTLY"
status: resolved
resolved_in: phase-353
type: bug
severity: high
area: build, testing
related: [issue-0509, issue-0574, issue-0196, phase-353, phase-360]
---

## Symptom

Three tier-2 tests failed, and no amount of rebuilding cleared them:

```
West fixture west_bringup_zephyr is STALE — built with a different `nros` CLI
than the current packages/cli/target/release/nros.
```

`cli_bringup_zephyr_adapter_shim_boots_native_sim` and two siblings, on a run
whose `build-test-fixtures lane=all` had just reported all nine families OK.

## Cause — two stamps, two notions of freshness

phase-353 W2 (issue 0509) made `west-fixtures.sh` REUSE a build directory when
its `.inputsig` matches, instead of wiping and rebuilding every run. That was
correct and is worth keeping: it took a no-op lane from 1244 ninja edges to 0.

But the west lane writes TWO stamps, and they answer different questions:

| stamp | records | read by |
| --- | --- | --- |
| `.inputsig` | content signature + the CLI's codegen FINGERPRINT | the staleness gate |
| `.compile-ok` | the CLI's BINARY hash | `require_west_fixture`, at test time |

The reuse branch checked the first and `continue`d without refreshing the
second. So after any CLI rebuild:

1. the codegen fingerprint is unchanged (the tool behaves the same), so
   `.inputsig` still matches;
2. reuse therefore skips the build;
3. `.compile-ok` keeps the OLD binary hash;
4. the consumer compares binary hashes and reports STALE.

And it is PERMANENT: step 1 keeps holding, so the build is skipped forever and
the stamp is never rewritten. Rebuilding cannot clear it — the only escape is
deleting the build directory to force the wipe path.

This is the issue-0196 shape once more, and the third instance in two days after
0574 and 0576: the build side writes one thing, the consumer reads another.

## Fix

Re-stamp `.compile-ok` on the reuse branch.

That is the honest claim rather than a paper-over. The reuse branch is only
taken when `.inputsig` matched, and that signature covers what the tool WOULD
PRODUCE (its codegen fingerprint, phase-360) — a strictly better question than
"is this the same binary". The stamp records the weaker fact, so refreshing it
to the tool the signature just vouched for is correct.

The deeper fix is for `require_west_fixture` to compare fingerprints rather than
binary hashes, so a behaviour-preserving CLI rebuild is a non-event everywhere.
Not done here; the stamp refresh removes the failure without changing the
consumer's contract.

## Verified

Append a comment to a CLI source, `just setup-cli`, run the lane:

```
west fixtures: 4/4 ok (4 reused, 0 built).
current CLI : 9c9dca08dd43281d
stamp says  : 9c9dca08dd43281d   => consumer accepts
```

Before the fix those two hashes differed, and differed again after every
subsequent rebuild.

## How it was found

Not by a gate — by the first tier-2 sweep to reach a verdict after the reuse
change landed. Worth noting for #0604: the reuse optimisation and the consumer
check were each individually reasonable, and the interaction was invisible until
a full run exercised both.
