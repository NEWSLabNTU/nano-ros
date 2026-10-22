---
id: 1489
title: "A phase's claims go stale when ANOTHER phase does the work, and nothing
  checks it — phase-417's build list presented 36 closed rows as open for four
  days while the ledger knew better"
status: resolved
type: bug
area: docs, tooling, ci
severity: medium
related: [419, 1226, 0196, phase-417, phase-444, phase-478]
found: 2026-09-25
resolved_in: 2026-10-22
---

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)** — homed 2026-10-17; the phase doc already tracks it in its body.

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

[phase-419](../../roadmap/archived/phase-419-roadmap-claim-verification.md) built exactly
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

## Resolution (2026-10-22)

**Cause.** No rule compared a phase's claims with anything outside the
document. phase-419's gate catches a phase contradicting itself. phase-417's
build list did not contradict itself. Its rows were closed by phase-444,
issue 1437 and phase-467, and none of those had a reason to edit phase-417.

**What was built.** `scripts/lib/roadmap_ledger_claims.py`, reported as a
section of `just roadmap-audit` (and so of the monthly `roadmap-audit.yml`).
It answers the three design questions this way:

* **Binding: derived.** A work item is a heading or bold opener carrying an
  id (`**G1 — …**`, `## W1 — …`). Its binding is the backticked ledger keys
  (`c:wait_for_publishers`, `cpp:Timer::is_ready`) in its own text. Those are
  the ledger's own spellings, so there is no map to go stale. An item is
  closed, and claims nothing, when its own label says so (`— DONE`,
  `(landed)`) or a later bold line names the same id (`**G6 LANDED …**`, the
  convention this phase already used). A key on a ticked `- [x]` line counts
  as claimed done. A named key is **open** if its row is `gap`, or if it has
  no row and inherits a `gap` from its type or a glob (`api-parity.py`'s
  `lookup` fallbacks). It is **closed** if its row has any other verdict or a
  `retired` reason, or if it has no row at all: `api-parity --check` refuses
  every difference that has no row, so a name with none matches upstream. The
  ledger is read by `api-parity.py`'s own `load_ledger`, not by a second
  merge.
* **Scope: three outcomes per phase.** A phase is STALE when an open item
  names keys the ledger has closed, VERIFIED when every key named is still
  `gap`, and NOT APPLICABLE when no open item names a key. An open item that
  claims "N rows" and names no key is listed as **NOT VERIFIED**. That is
  exactly what G1–G5 looked like, so the rule reports them as not verified
  rather than passing them. On `main` today the result is 26 of 26 active
  phases NOT APPLICABLE. phase-488 is the only active phase that names a key,
  and it does so under a heading labelled `landed`.
* **Gate or report: a REPORT.** A gate would go red on whichever pull request
  merged the sibling work, and that PR changed nothing in the phase it would
  be blamed for. That is the coupling this issue says it exists to remove
  ("Nothing here says phase-444 should have edited phase-417"), and it is why
  phase-419 put its cross-file checks on the report. The **negative control
  is on the fast line**: `self_test()` (12 cases, synthetic documents against
  a synthetic ledger written through the real loader) runs inside
  `check-roadmap-claims`'s own self-test. It reads nothing from the tree, so a
  sibling merge cannot turn it red, and it costs milliseconds.
  `roadmap-audit.py` also runs it first and exits 1 if it fails. That is the
  only non-zero exit a report has.

**The live instance, re-measured, and the table above was not quite right.**
Re-derived per key by walking the ledger history from `846c755e9` (the build
list's commit) to HEAD, taking the first commit where each row left `gap`.
G1's two Rust `*_info_by_topic` rows closed in `f7bc19dd6`, not `bc7be55e0`.
G3's `rust:Subscription::qos` closed in `3527dc794`, one of its two fixes; the
other is `38cf5c33d`. **G8 and G9 were stale too**, and the table did not list
them. Every row they name is off `gap`: five SHIPPED through phase-467 and
issue 1587, three `cpp:Client::*_pending_request*` rows and the two
`take_*_with_info` rows DECLINED by phase-444's re-read (`30e998e55`), the
five `can_loan_messages` rows declined in `d5492af85`, and
`rust:BOOT_SET_NAMESPACE` deleted in `e0055f50a`. The whole ledger now holds
one `gap` row, `rust:ServiceInfo`. phase-417 (archived since 2026-10-07)
now carries `**G1 LANDED**` … `**G5 LANDED**` and `**G8/G9 CLOSED in the
ledger**` paragraphs. Each one names its commits and the row keys it closed,
which is the minimum that makes the closure checkable. G8/G9 say CLOSED, not
LANDED, because several of their rows were declined rather than shipped.

**How verified.**
* `python3 scripts/lib/roadmap_ledger_claims.py <doc>` on the pre-fix
  phase-417 reports G8 (7 of 7 keys closed) and G9 (1 of 1) as stale, and
  G1–G5 as NOT VERIFIED (each claims rows and names no key). On the same text
  with the keys added but the closure lines turned back into plain
  continuations, it reports G1 18/18, G2 2/2, G3 13/13, G4 5/5, G5 6/6, G8
  9/9 and G9 3/3 stale. On the fixed doc it reports nothing.
* Mutation-tested through the gate. Each of these turned
  `check-roadmap-claims.py` red, and the file was then restored and confirmed
  byte-identical:
  * a non-`gap` verdict read as open;
  * a missing row read as open;
  * the same-id LANDED line ignored;
  * closed items still checked;
  * UNBOUND never reported;
  * ticked lines not skipped.

  `roadmap-audit.py` exits 1 under a mutation.
* `just check fast` green.

**Not run.** `just ci gate` and the `roadmap-audit.yml` workflow itself. The
rule reads only the API-parity ledger. Other artifacts a phase could be
checked against (fixture rows, the RMW parity map) have no rule, and a phase
that states its claims as counts without keys stays NOT VERIFIED. That is
reported, not fixed: closing it needs the phase to name its keys.
