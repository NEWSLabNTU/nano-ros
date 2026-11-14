# Phase 552 — Diagnostic codes, defined once in code

**Status (2026-11-14). IN PROGRESS: W1, W2 and W4 done; W3's book view and
gate done, its CMake, C header and colcon views open; W5 started (six codes on
the first-build paths); W6 open.** Work for
[RFC-0129](../design/0129-diagnostic-codes.md).

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

- **Status:** DONE (2026-11-14, branch feat/552-diagnostics-crate, no PR yet)

Notes:

- The package is `nros-diagnostic-codes`, not D1's `nros-diagnostics`: that
  name belongs to the on-target contract reporter
  (`packages/core/nros-diagnostics`), and cargo refuses two packages of one
  name in one workspace. It is a root-workspace member, `no_std` with no
  dependencies; it checks for `thumbv7m-none-eabi` and `thumbv6m-none-eabi`.
- `diagnostics!` takes `NNNN Name { title, surfaces: [..], remedy }` and
  `NNNN Retired(Name) => NNNN,`. A live row's explanation is
  `explain/NROS-NNNN.md` through `include_str!` (a `compile_fail` doctest
  shows a missing file refusing). A retired row has no `Diag` variant; a
  test-only table in `src/tests.rs` covers retirement.
- `Refusal` renders D2's three lines; `.help(..)` replaces the remedy and
  `.note(..)` adds a line only under verbose output.
- `nros-diagnostics-gen` is a `[[bin]]` behind the `std` feature, so the
  library stays `no_std`. `codes.lock` is committed and a test compares it
  with a fresh rendering.
- `just code-new <Name>` runs `scripts/reserve-diag-code.sh`
  (`refs/diag-codes/NNNN`), mirroring the issue-id reservation. It was tested
  against a scratch bare repository, never against origin.
- **Seed codes.** NROS-0001 to NROS-0006 were allocated in the founding
  commit, before the reservation existed; `codes.lock` and `src/codes.rs`
  say so. Every later code is reserved with `just code-new`.

| code | name | sites |
| --- | --- | --- |
| NROS-0001 | `AmentPrefixUnset` | configure, for `nros configure`, `nros build` and `nros run` |
| NROS-0002 | `BoardUnknown` | the catalog's `matches no board` (`orchestration/image.rs`), `board_facts::unresolved_board` and its callers |
| NROS-0003 | `NoImageDeclared` | `builder/plan.rs` (`nros build`), `cmd/locate.rs` |
| NROS-0004 | `ImageNoBoard` | `leaf_system.rs` (two), `leaf_settings.rs`, `build.rs`, `orchestration/image.rs`, and the workspace reader's refusal from `nros-orchestration-ir` |
| NROS-0005 | `PlatformNoFamily` | the four refusals of `PlatformKind::resolve_family` |
| NROS-0006 | `UnknownDiagCode` | `nros explain` given a code nothing has |

### W2 — The constructors (RFC-0129 D2)

- CLI: `Diag::X.err(..)`, which returns an `eyre::Report`.
- Build script: `Diag::X.panic(..)`.
- Each constructor has a `.help(..)` override, and each prints `-v` detail
  (ids allowed there, per ruling Q4) only when verbose output is on.

**Acceptance:**

- One test per constructor matches the rendered text exactly.
- No rendered line carries an issue, phase or RFC id. This is checked with
  `check-user-surface-ids`'s pattern.

- **Status:** DONE (2026-11-14, branch feat/552-diagnostics-crate, no PR yet; build-script conversions are W5)

Notes:

- CLI: `nros-cli-core/src/diag.rs`. `DiagExt` on `Diag` gives
  `.err(format_args!(..))` (an `eyre::Report`) and `.refusal(..)`, with
  `.help(..)` and `.note(..)`; a `String`-error site takes `.to_string()`.
  The note prints under `-v` / `NROS_VERBOSE=1` through
  `build_notes::verbose()`. `main` prints an unwrapped refusal bare, with no
  `Error: ` in front; a refusal under added context keeps eyre's report. A
  site that adds context to a refusal it did not build uses
  `diag::prefix_detail`, so the code stays at the head of the line.
- Build script: `Diag::X.panic(format_args!(..))` and
  `Diag::X.refuse(&..).help(&..).panic()`, in the `no_std` crate; with the
  `std` feature `NROS_VERBOSE=1` turns the note on. No build script is
  converted yet (W5).
- Tests: the exact text of each constructor, in the crate and in
  `diag.rs`, with the gate's id pattern asserting no id. The seed sites' old
  texts quoted in the troubleshooting index and the message-generation page
  are replaced by the new ones.
- The workspace reader's "names no `board`" refusal cited an RFC id in user
  text, which `check-user-surface-ids` does not read (one of the shapes the
  Coordination section names); the id moved to a source comment. Its other callers still print it as plain
  text: converting it where it is raised would add a dependency edge to
  `nros-orchestration-ir`, which ten tracked lockfiles carry.

### W3 — The generated views and their freshness gate (RFC-0129 D4, D6)

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

- **Status:** OPEN (the book view and the gate are done, 2026-11-14,
  branch feat/552-diagnostics-crate, no PR yet; the CMake, C header and colcon views are open)

Notes:

- Done: the book reference (`index.md` and one page per live code, on the
  page map as generated pages and in SUMMARY after the troubleshooting
  index), and `just check diagnostics` (`scripts/check/check-diagnostics.py`,
  fast lane, `needs:git-history`). The gate runs the generator's `--check`
  (fresh views, no orphan page or explanation, every row has a surface) and
  holds `codes.lock` to origin/main's (NOT VERIFIED without an origin/main).
  Its self-test plants a stale page, a missing lock, an orphan page, an
  orphan explanation and a removed, renamed or renumbered lock row. A row
  with no surface cannot be planted without compiling: the macro refuses it.
- The pages built with `mdbook build` on a copy of the book (the board-status
  preprocessor left out); a full `just book` was not run.
- SUMMARY lists the code pages flat, as the fork pages are. Hundreds of codes
  will want a Reference group (`book_pages.GROUPS`). A new page's SUMMARY
  title is authored by hand after `book-move.py --summary`.
- Open: `cmake/NanoRosDiagnostics.generated.cmake` with `nros_refuse`, the
  `cmake -P` test, `nros/diagnostics.h`, and the colcon `diagnostics.py`.

### W4 — `nros explain <CODE>` (RFC-0129 D5)

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

- **Status:** DONE (2026-11-14, branch feat/552-diagnostics-crate, no PR yet)

A live code prints its title, its
explanation without the `## Design background` section, and its remedy; a
retired code names its successor (tested on the test-only table); an unknown
code is refused through NROS-0006 with the three nearest codes. A test pins
the pre-W4 plan rendering byte for byte, and the stdout of the built CLI on a
plan matched the pre-W4 binary's. The verb's help reads "A diagnostic code
(`NROS-NNNN`) or a plan path", and `book/src/reference/cli.md` is
regenerated from the built CLI.

### W5 — Convert the user-reachable refusals, surface by surface (RFC-0129 D2, D6)

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

- **Status:** OPEN (started 2026-11-14: the six seed codes of W1 convert the
  refusals a first build reaches on the probe-track paths; the ratchet
  `check-refusal-constructors` is not written yet)

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
default-level output, at baseline zero, with no ratchet left. What it cannot
yet read is [issue 2551](../issues/2551-user-surface-text-beyond-function-flow-cites-ids.md).
So W5 prepends codes to messages that are already self-contained, and rewrites
no one else's text. That gate's id pattern does not match `NROS-NNNN`, and its
self-test asserts it, so prepending a code never trips it.

The phase moved from nros 243 to the AUO book session on 2026-11-14.
