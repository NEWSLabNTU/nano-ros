---
id: 574
title: "Tier 2 and tier 3 demand `.inputsig` for the four west compile-checks, and no builder writes it"
status: open
type: bug
area: build
related: [issue-0536, issue-0537, issue-0535, issue-0482, phase-350]
---

## Symptom

A COMPLETE, green `just build-test-fixtures lane=tier2` — all eight modules
`OK`, zero failures — is immediately followed by `ci-matrix` failing before it
runs a single test:

```
ERROR: 4 compile-check fixture(s) are missing or stale:
  west_bringup_zephyr     (missing build/compile-check-fixtures/west_bringup_zephyr/.inputsig)
  zephyr_self_pkg_sibling (missing …/.inputsig)
  zephyr_self_pkg_rust    (missing …/.inputsig)
  west_board_import       (missing …/.inputsig)
  Run `just build-test-fixtures` before test-all.
error: recipe `_lane-gate` failed with exit code 1
```

Running `just build-test-fixtures` again does not help, which is the tell.

## Cause — the gate requires a file no producer writes

Three components, and they do not meet:

| component | behaviour |
| --- | --- |
| `check-fixtures-stale.sh` | under scope `coords` (tier 2) and `all` (tier 3), requires `build/compile-check-fixtures/<id>/.inputsig` for EVERY compile-check row, west included |
| `compile-check-fixtures.sh` | the only writer of `.inputsig`. Its builder loop is `cargo-check cargo-build cross-build cmake-configure cxx-syntax` — **`west-build` and `west-configure` are absent**, so it never writes one for these four |
| `west-fixtures.sh` | builds them successfully (`west fixtures: 4/4 ok`) but stamps `build/west-fixtures/<id>/.compile-ok` — a different FILENAME in a different TREE |

So on tier 2 and tier 3 the requirement is unsatisfiable by construction. Verified
on this host: after a full green `lane=tier2` build AND a direct
`scripts/build/west-fixtures.sh` run reporting `4/4 ok`, all four `.inputsig`
paths are still absent while all four `build/west-fixtures/<id>/` trees exist.

The manifest says the split is deliberate — *"Built by the WEST lane
(west-fixtures.sh), never by compile-check-fixtures.sh: west needs a provisioned
Zephyr workspace, so the lane that owns one runs them."* That is a good reason
for two BUILDERS. It is not a reason for two stamp conventions, and the gate
knows only one of them.

## How the native lane escaped it

`check-fixtures-stale.sh` already carries the exemption — for tier 1 only:

```sh
if [ "$SCOPE" = "native" ]; then
    … | awk -F'\x1f' '$2 !~ /^west-/'     # drop west rows
else
    …                                     # coords/all keep demanding them
fi
```

with the reasoning that `all` and `coords` "either build west or select by
coordinate, and silently dropping a west row there would hide a real staleness".
The intent is right; the premise is not. Those lanes do build west — but the
build writes `.compile-ok` under `build/west-fixtures/`, so the gate's check
cannot see it and fails on a fixture that is present and fresh.

Issue 0536 added these four rows and 0537 recorded a related builder-visibility
failure; this is the third instalment of the same seam.

## Direction

Make the west stamp visible to the gate, rather than exempting more scopes —
an exemption would restore #482's real hazard (a stale west fixture passing
silently). Either:

* have `west-fixtures.sh` also write `build/compile-check-fixtures/<id>/.inputsig`
  using `compile-check-signature.sh`, so ONE stamp convention covers every
  compile-check row regardless of which lane built it; or
* teach `check-fixtures-stale.sh` to resolve a west row to
  `build/west-fixtures/<id>/.compile-ok` — the stamp that actually exists, and
  which already records the builder (phase-350 W2) so "configure only" stays
  checkable.

The first is preferable: the gate then has one rule, and a future builder that
forgets the stamp fails loudly instead of inventing a third convention.

## Workaround

`NROS_SKIP_FIXTURE_CHECK=1 just ci-matrix` — which disables the staleness check
for EVERY fixture, not just these four, so it is a way to get a test signal and
not a fix.

Found 2026-08-14 while running tier 2 for issue 0528's acceptance.
