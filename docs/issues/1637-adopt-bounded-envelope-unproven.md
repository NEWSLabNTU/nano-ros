---
id: 1637
title: "`adopt-bounded` claims a doc comment states an envelope, and nothing
  checks that the comment exists or is true — 93 ledger rows make that claim"
status: open
type: tech-debt
area: [api, tooling]
severity: low
found: 2026-10-02
related: [1463, rfc-0089]
---

## The claim

RFC-0089's `adopt-bounded` means "same name and contract, weaker inside an
envelope that the DOC COMMENT states — the envelope is part of the API". It is
the one disposition that asserts something OUTSIDE the ledger. Measured
2026-10-02: **93 rows** carry it (79 `divergence`, 10 `declined`, 3 `rename`,
1 `gap`).

## Why it is ungated

`scripts/api-parity.py` validates that a disposition is one of the four words,
and since issue 1463 a `gap` on a declared name must name an `owed` witness
that is checked against the tree. Neither asks whether the envelope a row
cites exists. Issue 1463's own row (`c:log_severity_t`) showed it can be false
in the worst direction: `nros-c/src/log.rs` asserted the OPPOSITE of the
envelope the row said it stated, and `log.h` stated none.

## What 1463 did not do, and why

1463 priced this as candidate 1 and rejected it as THE fix for 1463, because
it catches a false envelope, not a closed gap. It is still a real gap of its
own. The shape that would close it is the one 1463 landed for `owed`: an
`envelope` object naming `{file, text}` that must be found in the tree,
required on `adopt-bounded` rows. Requiring it on all 93 at once fails the tree
the day it lands, so it needs the staging `--require-disposition` had — e.g.
validated when present, then required per shard as each is re-read.
