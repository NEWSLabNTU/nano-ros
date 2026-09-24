---
id: 1489
title: "A phase's claims go stale when ANOTHER phase does the work, and nothing
  checks it — phase-417's build list presented 36 closed rows as open for four
  days while the ledger knew better"
status: open
type: bug
area: docs, tooling, ci
severity: medium
related: [419, 1226, 0196, phase-417, phase-444]
found: 2026-09-25
---

## What happens

A roadmap phase states work items with counts and sizes. Another phase does
that work. The first phase is never edited, so it keeps presenting finished
work as open — and it does so in the voice of a plan, which is exactly what a
reader (or an agent writing a brief from it) takes at face value.

Measured on `main`, 2026-09-25. `docs/roadmap/phase-417-ros2-api-adoption.md`'s
build-list section:

| group | the doc says | reality |
| --- | --- | --- |
| G1 graph receiver forwarders | "18 rows (C 6, Rust 12). CHEAP." | landed in `bc7be55e0`, phase-444 |
| G2 the two `wait_for_*` helpers | "2 rows (C). CHEAP." | landed in `c902f7f9d`, phase-444 |
| G3 granted-QoS read-back | "5 rows here, moving with 8 siblings — 13 rows as one item. MEDIUM" | landed in `8aed6be82` + two fixes, issue 1437 |
| G4 matched-endpoint counts | "5 rows (C 2, C++ 2, Rust 1). CHEAP" | landed in `f7bc19dd6`, phase-444 |
| G5 entity-name read-back | "6 rows (C++ 2, Rust 4). CHEAP then SMALL." | landed in `97b4ed5b8`, phase-444 |

**36 rows presented as open work.** The whole ledger currently holds **13**
`gap` rows across every group and every shard.

G6 and G7 both carry `**G6 LANDED 2026-09-21**` / `**G7 LANDED 2026-09-21**`
markers — because the wave that did them was scoped from this phase and updated
it on the way out. G1–G5 carry nothing, because phase-444 did them and had no
reason to touch phase-417.

## Why phase-419 does not cover this

[phase-419](../roadmap/phase-419-roadmap-claim-verification.md) built exactly
the right gate for a different defect. Its rules (R1/R3/R4, in
`scripts/check-roadmap-claims.py`, fast line, baseline ratcheted to **0**) catch
a document contradicting **itself** — a header saying "not started" over a body
saying `LANDED`. That is mechanical and needs no judgment, which is why it
could be a gate.

phase-417's build list is not self-contradictory. Every sentence in it was true
when written. It went false because the *tree* moved underneath it, and no rule
in the tree compares a phase's claims against anything outside the document.

So this is a third layer, between phase-419's "mechanical" and "judgment"
columns: a claim that is mechanically checkable, but only against an artifact
the document does not contain.

## What it cost

Four of seven agent briefs written from this phase in one session were stale.
Each was caught, and only because the agents re-measured against the ledger
instead of trusting the brief:

* a wave scoped from G6 found `cpp:Timer::is_ready` needed a whole new FFI slot,
  because the row's "both C halves shipped in stage 3" was true of
  `rcl_timer_is_ready` and not of the seam `nros::Timer` actually has;
* the same wave found `rust:QoSProfile::parameter_services_default` was a
  **rename**, not an addition — implementing it as written would have created a
  fourth spelling of one preset;
* the coordinator (me) repeated the doc's counts to the maintainer as current
  more than once.

The pattern that saved it every time — *read the ledger, not the brief* — is
the argument for making the ledger the checkable authority rather than a
convention each agent has to rediscover.

## What would close this

A rule in the phase-419 family, in `scripts/check-roadmap-claims.py` or beside
it, that reads a phase's claimed row counts against the artifact that actually
knows. For phase-417 that artifact is
`docs/reference/api-parity-ledger/*.json`, where every row carries a `verdict`;
a group claiming N open rows whose rows are all non-`gap` is mechanically
detectable with no judgment.

The design questions, none of them answered here:

* **Binding a claim to its artifact.** The doc would have to say which ledger
  rows a group covers. An authored mapping is the drift this repo refuses
  elsewhere (the RMW parity map's 28 stale slots, the layout gate's three
  authored type names) — so the binding should be derived, or the rule should
  key on something already present, such as the row keys the group names.
* **Scope.** phase-417 has a ledger. Most phases have no such artifact, and for
  those this rule can say nothing — which is fine, and should be *reported* as
  not-applicable rather than silently passing (issue 1043's three outcomes).
* **Gate or report.** phase-419 deliberately made its cross-file work a REPORT
  (`just roadmap-audit`, monthly) and only the self-contradiction rules a gate.
  A count that drifts the moment a sibling phase merges may belong on the report
  side for the same reason.

## Not claimed

Nothing here says phase-444 should have edited phase-417. It should not have to
— that is the coupling this issue exists to remove. And phase-417's build list
is not being blamed for being wrong: it was right, and then it aged, which is
the normal fate of a plan that outlives the work.

Also worth recording, because it nearly became a second false filing: the
monthly `roadmap-audit.yml` has **zero runs ever**, which reads exactly like a
dead lane. Its cron is `0 7 1 * *`, it landed 2026-09-07, and the next 1st of
the month has not arrived. It is fine. (Same shape as the `probe` lane, which
was also read as twelve days dark when it was two days old.)
