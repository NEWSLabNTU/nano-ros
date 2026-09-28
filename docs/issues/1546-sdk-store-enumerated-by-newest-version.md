---
id: 1546
title: "The SDK store is enumerated newest-version-first in two places — pending a ruling on whether issue 0500's ordering survives"
status: open
type: bug
area: [build, sdk]
severity: medium
found: 2026-09-28
related: [phase-472, 0500]
---

## What happens

VERIFIED present on `main`:

- `cmake/toolchain/NanoRosCrossToolchain.cmake:181-182` —
  `file(GLOB _vers … "${_store}/*")` + `list(SORT … NATURAL ORDER DESCENDING)`,
  taking the first hit.
- `scripts/build/riscv64-toolchain.sh:43,70` — `for ver in $(ls -1 "$store" | sort -Vr)`.

`check-sdk-store-not-enumerated` states that the store is "constructed from the
pin and never enumerated", but matches only a literal `/sdk/<tool>/*`. The
cmake site is the shared helper that REPLACED the site the gate's docstring
cites, so the fix moved the defect out of the regex's reach. The gate's docstring
also promises a self-test "below" that does not exist.

## What is NOT established — a ruling is needed

`riscv64-toolchain.sh` cites *"the 0500 rule"* for its newest-first order, and
CLAUDE.md documents 0500 as newest-first enumeration. It also says that ordering
was RETIRED with the globbed prefix in phase-365 W3a. So these two sites are
either live violations citing a retired rule, or a sanctioned exception the gate
should know about. That is a design ruling, not a fact the audit can establish.

## Fix

Rule on it. If pinned: construct both sites from the pin. Either way the gate
matches any store-rooted glob or `ls` followed by a sort, and gains its self-test
(phase-472 W7/W9).
