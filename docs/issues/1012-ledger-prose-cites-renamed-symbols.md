---
id: 1012
title: "Parity-ledger `why` prose names symbols a rename retired — 18 rows describe
  current state using dead spellings, and nothing checks it"
status: open
type: bug
area: docs, api
related: [phase-379, issue-0826]
---

## Problem

The parity ledger's `why` field is prose, and it names our own symbols in
backticks. When a rename lands in the code, the prose does not move with it.

Eighteen rows currently describe the *current* state of the tree using a
spelling that no longer exists. All eighteen were retired by phase-379's own
rename rows, so the ledger is stale against renames the ledger itself recorded:

| dead spelling | live spelling | rows |
| --- | --- | ---: |
| `is_server_ready` | deleted (issue 1008) | 2 |
| `server_available` | `service_is_ready` | 5 |
| `try_recv` / `try_recv_raw` | `take` / `take_raw` | 4 |
| `SubscriberOptions` | `SubscriptionOptions` | 4 |
| `QosSettings` | `QoSProfile` | 3 |

`server_available` is the worst of them: five `graph` rows cite it as the
precedent for a *shape*, so a reader following the citation finds nothing and
cannot tell whether the precedent was renamed or was never real.

## What is NOT in scope

Another eighteen rows also name retired spellings and are **correct** to: a
deprecated-alias row exists precisely to document the old name, and a row
recording `LANDED`/`DELETED` history must state what it replaced. Any fix that
sweeps on the spelling alone breaks those. The distinguishing property is
tense — historical rows say what *was*, stale rows say what *is*.

## Why it survived

`api-parity.py --check` validates ledger *structure* (every non-matching record
has a verdict, every verdict is one of the five, no unledgered rows). It has no
opinion about the prose, which is where the claims live. The `provides` field
added in phase-379 W7 is machine-checkable in a way `why` is not, and the two
now disagree on several rows: agent-authored `provides` pointed at the live
symbol while the sentence above it named the dead one.

Found while authoring `provides`: two independent passes each hit rows whose
prose named a symbol that could not be grepped.

## Direction

The cheap half is fixing the eighteen. The durable half is a gate, and it needs
a decision first: a check that every backtick-quoted `nros::`/`nros_` token in a
`why` resolves against `oursyms` would also fire on the legitimately historical
rows, on illustrative codegen spellings (`<pkg>_<msg>_publish`), and on
deliberate counterexamples. Options, cheapest first:

1. Gate only rows whose verdict is not a rename/deprecation, and allow an
   explicit `historical: true` escape.
2. Require dead spellings to be marked in prose (`~~name~~`, or a `was:` field)
   so tense becomes machine-readable rather than inferred.
3. Leave it ungated and re-audit after each rename batch — what happened here,
   and it did not hold for one phase.
