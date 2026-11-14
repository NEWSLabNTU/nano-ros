# Phase 552 — Diagnostic codes, defined once in code

**Status (2026-11-12). READY: every RFC-0129 question is ruled; nothing
started.** Work for [RFC-0129](../design/0129-diagnostic-codes.md).

The user rulings of 2026-11-12, relayed by the manager session:

| question | ruling |
| --- | --- |
| direction | option C |
| Q1–Q3 | "Ok to have NROS-NNNN code. We should have a way to explain it. I prefer to have a proper code constructs so we have a SSoT list of the code and error descriptions." |
| Q4 (ruling A) | a user-reachable refusal carries no id, build-stopping refusals included |
| Q5 | the runtime and colcon surfaces are in scope |

The rulings are quoted in full in the RFC. Per the standing rule of the same
day, this phase doc tracks the RFC's implementation, progress and acceptance.
Each item is claimed with `just claim phase-552-wN` before it starts.

Implements: [RFC-0129](../design/0129-diagnostic-codes.md).

---

## Why

A refusal that a user meets either cites an internal issue id the user cannot
follow, or cites nothing. Ruling Q4 removes the id. A code defined once in a
crate gives the user a stable handle that works offline. A refusal cannot be
written without that code, and the code cannot exist without its explanation
(RFC-0129 Problem).

## Work items

### W1 — The `nros-diagnostics` crate (RFC-0129 D1–D3)

- **Status:** OPEN

- `packages/tooling/nros-diagnostics`: `#![no_std]`, with the `diagnostics!`
  macro, the `Diag` enum, `ALL: &[Entry]`, and one `explain/NROS-NNNN.md` per
  code, read by `include_str!`.
- Retired rows (`Retired(..) => successor`). A retired row keeps its number,
  and nothing can construct it.
- `codes.lock`, generated and committed: every number ever allocated, with its
  name and status.
- A `code-new` recipe that reserves a number on origin, mirroring
  `scripts/reserve-issue-id.sh`.
- Seed codes: the refusals that W5's first batch converts, starting with the
  new platform-family refusal (a board whose platform reaches no built-in
  family).
- If `packages/cli/cli-source-dirs.txt` changes, regenerate it.

**Acceptance:**

- A row whose explanation file is missing fails to compile.
- A test renders one entry in D2's format, with the detail, `help` and
  `explain` lines.
- `codes.lock` round-trips: running the generator again changes nothing.

### W2 — The constructors (RFC-0129 D2)

- **Status:** OPEN

- CLI: `Diag::X.err(..)`, which returns an `eyre::Report`.
- Build script: `Diag::X.panic(..)`.
- Each constructor has a `.help(..)` override, and each prints `-v` detail
  (ids allowed there, per ruling Q4) only when verbose output is on.

**Acceptance:**

- One test per constructor matches the rendered text exactly.
- No rendered line carries an issue, phase or RFC id. This is checked with
  `check-user-surface-ids`'s pattern.

### W3 — The generated views and their freshness gate (RFC-0129 D4, D6)

- **Status:** OPEN

`nros-diagnostics-gen` writes four views:

- the book reference: an index plus one page per code, under
  `book/src/reference/diagnostics/`;
- `cmake/NanoRosDiagnostics.generated.cmake`, which provides
  `nros_refuse(<Name> …)` and refuses an unknown name;
- `nros/diagnostics.h`, with `NROS_DIAG_<NAME>` constants;
- the colcon plugin's `diagnostics.py`.

Gate `check-diagnostics`, on the fast line:

- every view is fresh (`--check`);
- `codes.lock` keeps every number and name it had on `origin/main`;
- no explanation file is an orphan;
- every entry names at least one surface.

It has a self-test with one planted defect per check.

**Acceptance:**

- Each planted defect turns the gate red, and the real tree is green.
- `nros_refuse(Unknown …)` is a configure error. This is shown by a `cmake -P`
  test.
- The book pages build under `just book`.

### W4 — `nros explain <CODE>` (RFC-0129 D5)

- **Status:** OPEN

The argument decides which: a token matching `^NROS-\d{4}$` is a code, and
anything else is a plan path, as today.

- A live code prints its entry offline.
- A retired code names its successor.
- An unknown code is refused with the three nearest codes, through a
  diagnostic.

**Acceptance:**

- These print correctly with no network and no checkout:
  - a live code;
  - a retired code;
  - an unknown code.
- `nros explain <plan>` output is byte-identical to before W4.
- The CLI reference is regenerated (`scripts/gen-cli-reference.py`).

### W5 — Convert the user-reachable refusals, surface by surface (RFC-0129 D2, D6)

- **Status:** OPEN

1. **Ratchet.** Land the per-surface ratchet `check-refusal-constructors`,
   with each surface's baseline measured at landing and recorded in the
   gate's docstring together with the command that measured it.
2. **Conversion order.**
   - **First**, the refusals the probe tracks reach: `just probe bootstrap`,
     `just probe installed` and `just probe <platform>`, plus `nros doctor`.
   - **Then** the rest of the CLI user tier.
   - **Then** cmake and build scripts.
   - **Last**, runtime and colcon.
3. **What each conversion does.** It gets an entry and an explanation, its
   rendering moves onto the constructor, and any id moves to a source comment
   or to the explanation's `## Design background` (ruling Q4).
4. **Runtime surface.** Before converting it, measure the flash cost of a code
   prefix on one embedded image, and report it.

**Acceptance:**

- Every per-surface baseline is zero.
- `check-user-surface-ids` stays green.
- [Issue 2305](../issues/2305-user-tier-messages-cite-internal-issue-ids.md)
  is archived as resolved, with ruling Q4 cited.

### W6 — Close

- **Status:** OPEN

**Acceptance:**

- RFC-0116 D2 states the code rule, citing ruling Q4.
- RFC-0129 is marked Stable.
- ARCHITECTURE.md names the crate as the single source of truth for
  diagnostics, in the same commit.

## Coordination

The no-ids sweep this phase builds on is done.
[Issue 2443](../issues/archived/2443-runtime-log-and-colcon-messages-cite-ids.md)
(runtime and colcon messages) and
[issue 2547](../issues/archived/2547-user-surface-text-formatted-away-or-tabled-cites-ids.md)
(text printed through a variable or a table) are resolved;
`check-user-surface-ids` reads every user surface they named, and CMake's
default-level output, at baseline zero, with no ratchet left, and so is
[issue 2551](../issues/archived/2551-user-surface-text-beyond-function-flow-cites-ids.md)
(text that crosses a function, a field, a loop or a stream, and generated
rustdoc).
So W5 prepends codes to messages that are already self-contained, and rewrites
no one else's text. That gate's id pattern does not match `NROS-NNNN`, and its
self-test asserts it, so prepending a code never trips it.

The phase moved from nros 243 to the AUO book session on 2026-11-14.
