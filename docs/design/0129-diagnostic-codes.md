---
rfc: 0129
title: "Diagnostic codes, defined once in code and explained by one verb"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-552]
amends: [0116]
supersedes: []
superseded-by: null
---

# RFC-0129 — Diagnostic codes, defined once in code and explained by one verb

**Status:** Draft (2026-11-12). Every question that shapes the design is
ruled; the rulings are quoted under "Rulings". The RFC stays Draft until
phase-552 lands the crate, the verb and the gates.

Work: [phase-552](../roadmap/phase-552-diagnostic-codes.md). Per the standing
rule of 2026-11-12, every design's implementation is tracked by a phase doc.
This RFC amends [RFC-0116](0116-adoption-purposes-and-front-doors.md) D2, as
ruling Q4 states.

## Rulings

All five are user rulings of 2026-11-12, relayed by the manager session.

- **Direction (option C).** Stable diagnostic codes, an explain verb, a single
  list of codes, and gates. The gates check that every user-reachable refusal
  has a code and that every code is explained.
- **Q1 (code format), Q2 (where the list lives) and Q3 (the verb).** One
  ruling, quoted: "Ok to have NROS-NNNN code. We should have a way to explain
  it. I prefer to have a proper code constructs so we have a SSoT list of the
  code and error descriptions."
  - Q1: the format is `NROS-NNNN`.
  - Q3: an explain command, `nros explain <CODE>` (D5).
  - Q2: the single source of truth is **code constructs**. A small crate
    defines each diagnostic once, and a refusal is constructed from that
    definition. Every other view is generated from the crate and gated for
    drift. A hand-maintained TOML file was proposed and is replaced.
- **Q4 (ruling A).** "User-reachable refusals, build-stopping ones included,
  carry NO id; ids go to source comments, `-v`, and later the code page's
  Design background." This is the RFC-0116 D2 amendment. It also settles the
  question issue 2305 left open: whether a build-stopping refusal may cite an
  issue id.
- **Q5 (scope).** The runtime and colcon surfaces are in scope. The sweep
  that made those messages self-contained is done:
  [issue 2443](../issues/archived/2443-runtime-log-and-colcon-messages-cite-ids.md)
  and [issue 2547](../issues/archived/2547-user-surface-text-formatted-away-or-tabled-cites-ids.md)
  are resolved, and `check-user-surface-ids` reads every surface they named at
  baseline zero (what it cannot yet see is
  [issue 2551](../issues/2551-user-surface-text-beyond-function-flow-cites-ids.md)).
  Codes are prepended to those messages.

## Problem

A refusal that a user meets has to answer three questions: what went wrong,
what to do, and where to read more. Today it answers the third with an
internal id, or with nothing.

- RFC-0116 D2 (consolidated in RFC-0124 Principle 5) rules that the user
  surface never names a phase, issue or RFC id.
- phase-517 W5 exempted a build that stops. On 2026-11-11, about 100 CMake
  `FATAL_ERROR` texts and about 25 build-script panic texts cited an issue id
  ([issue 2305](../issues/2305-user-tier-messages-cite-internal-issue-ids.md)).
  Ruling Q4 closes that exemption.

The id was there for a reason: it is the route to the explanation. A code
keeps that route without a checkout. It is stable, short and searchable, and
it resolves offline to a page. The page can link the history for whoever
wants it.

A list of codes kept apart from the code that emits them drifts the way every
hand-mirrored table in this tree has drifted. One example is the parity map
that disagreed with the shape tool by 25 symbols. So the list is the code
itself (ruling Q2): a refusal cannot exist without an entry, and an entry
cannot exist without its explanation.

### Measured (origin/main, 2026-11-12)

| surface | sites | note |
| --- | --- | --- |
| `bail!` / `eyre!` in `nros-cli-core/src` | 1,003 | not all user-reachable; `nros dev` verbs and internals are contributor tier (RFC-0117) |
| `FATAL_ERROR` in `cmake/`, `zephyr/`, `*.cmake`, package `CMakeLists.txt` | 540 | about 100 cited an issue id (issue 2305) |
| `panic!(` in `packages/**/build.rs` | 93 | about 25 cited an issue id |
| `cargo:warning` in build scripts and tooling | 37 | id-free since issue 2305 |

The CLI already has two `explain` verbs:

- `nros explain [PLAN]` renders an `nros-plan.json`.
- `nros config explain` shows how a knob resolved.

## Design

### D1 — One crate defines every diagnostic

The crate is `nros-diagnostics`, under `packages/tooling/`. It declares each
diagnostic exactly once, through one declarative macro. (Its package name is
`nros-diagnostic-codes`: `nros-diagnostics` is the on-target contract
reporter's name in the same workspace. phase-552 W1 records this.)

```rust
diagnostics! {
    /// A board names a platform whose `inherits` chain reaches no built-in family.
    0412 PlatformNoFamily {
        title: "a board's platform reaches no built-in family",
        surfaces: [Cli],
        remedy: "add `inherits = \"<family>\"` to the platform's nros-platform.toml",
    }
    0413 Retired(SomeOldRefusal) => 0412,   // retired: number reserved, points at its successor
}
```

The macro generates three things:

- an enum `Diag`, with one variant per live code and an explicit
  discriminant;
- a `const` table `ALL: &[Entry]`, where each entry holds the code, name,
  title, surfaces, remedy, retirement and explanation;
- the explanation, through `include_str!("explain/NROS-0412.md")`. A code
  without its explanation file is a compile error, so "every code has an
  explanation" holds by construction.

The surfaces are `Cli`, `Cmake`, `BuildScript`, `Runtime` and `Colcon`.

The crate is `#![no_std]` and holds only `const` data and formatting. So an
embedded image (the runtime surface), a build script and the CLI depend on
the same crate.

A number is chosen once and never changes. Retiring a code keeps its row
(`Retired(..) => successor`), so the number stays reserved and nothing can
construct it.

### D2 — A refusal is constructed from its diagnostic

A user-reachable refusal is built from its entry, never written as free text.
Each surface has one spelling:

| surface | construction |
| --- | --- |
| CLI | `Diag::PlatformNoFamily.err(format_args!("board `{b}` names platform `{p}`"))`, an `eyre::Report` |
| build script | `Diag::X.panic(…)`, the same text, then a panic |
| CMake | `nros_refuse(PlatformNoFamily "board ${b} …")`, from D4's generated include |
| runtime (C/C++) | `NROS_DIAG_PLATFORM_NO_FAMILY`, the code string from D4's generated header, prefixed to the log text |
| colcon | `diagnostics.refuse("PlatformNoFamily", …)`, from D4's generated Python module |

Every surface renders the same text:

```
error[NROS-0412]: board `acme` names platform `acmeos`
  help: add `inherits = "<family>"` to the platform's nros-platform.toml
  explain: nros explain NROS-0412
```

The detail line is the site's own words. The `help` line is the entry's
remedy. A site that needs a more specific remedy passes one with
`.help(..)`. Per ruling Q4, no rendered line carries an issue id. A
contributor's id goes in a source comment, or under `-v` (`nros build -v`,
`NROS_VERBOSE=1`).

### D3 — Codes are never reused or renumbered

The crate's build generates `codes.lock` and commits it. It is one line per
code ever allocated: the number, the name, and `live` or `retired`.

A gate compares the regenerated file with `origin/main`'s copy. Every pair
there must still be present, with the same number and name, either live or
retired. A row may be added or retired, never removed or renumbered.

Numbers are allocated by a reservation on origin, through a `code-new` recipe
(phase-552 W1) that mirrors `scripts/reserve-issue-id.sh`. Reading "the next number" is the
race that collided seven times for issue ids.

### D4 — Every other view is generated from the crate, and drift-gated

A generator binary in the crate (`nros-diagnostics-gen`) writes each view.
Each view's gate re-runs the generator with `--check`.

| view | path | consumer |
| --- | --- | --- |
| explain text | compiled into the CLI (the crate is a dependency) | `nros explain` (D5), offline |
| book reference | `book/src/reference/diagnostics/` (an index, plus one page per code) | readers; RFC-0124's reference section |
| CMake | `cmake/NanoRosDiagnostics.generated.cmake` | `nros_refuse(<Name> …)` looks up the code by name, and an unknown name is itself a configure error |
| C header | `packages/api/nros-c/include/nros/diagnostics.h` | C/C++ runtime log sites (`#define NROS_DIAG_<NAME> "NROS-NNNN"`) |
| colcon | the colcon plugin's generated `diagnostics.py` | the plugin's refusals |

A build script uses the crate directly, as a build-dependency. Rust runtime
code uses the crate's `const`s.

A book page holds the entry's title, its explanation, its remedy, and a
closing `## Design background`. That section is where the history ids live,
and RFC-0123 D3 already allows ids there. The page is never hand-edited.

### D5 — `nros explain <CODE>`

`nros explain` keeps its plan argument and gains codes. The argument decides
which: a token that matches `^NROS-\d{4}$` is a code, and anything else is a
plan path, as today.

- No existing invocation changes.
- A file literally named `NROS-0412` is reached as `./NROS-0412`.
- A **live** code prints the entry's title, explanation and remedy, offline.
- A **retired** code prints that it is retired and which code replaced it.
- An **unknown** code is refused with the three nearest codes. That refusal
  is itself a diagnostic.

The verb's help says "a diagnostic code (`NROS-NNNN`) or a plan path".

### D6 — Gates

- **`check-diagnostics`** (fast line) runs four checks:
  - every generated view (D4) is fresh;
  - `codes.lock` keeps every number and name it had on `origin/main` (D3);
  - no explanation file lacks an entry (the reverse is a compile error);
  - every entry's `surfaces` names at least one surface.
- **`check-refusal-constructors`** (fast line). It is a ratchet with one
  baseline per surface, which may only shrink.
  - **Rule:** a user-reachable refusal goes through D2's constructor.
  - **Subjects, one per surface:**
    - the CLI's user-tier verbs (RFC-0117);
    - the cmake a default configure reaches;
    - build scripts on a user build;
    - the colcon plugin;
    - error-level runtime logs of a default image.
  - **Relation to `check-user-surface-ids`.** That gate asks a different
    question: whether a message cites an id. It is a set of baseline-zero
    rules (its ratchets were dropped once empty), its id pattern does not
    match `NROS-NNNN`, and its self-test asserts that. The two gates are
    therefore compatible, and neither subsumes the other.
- `check-user-surface-ids` keeps its rule.

## Left to implementation

- The exact macro syntax.
- How a site passes a more specific remedy (`.help(..)` or a second
  argument).
- Whether the CMake include is one generated file or is folded into
  `NanoRosDiagnostics.cmake`.

None of these changes the SSoT rule, the rendered format or the gates.

## Rejected

- **A hand-maintained TOML registry with free-text refusals.** It was proposed
  in this RFC's first draft and replaced by ruling Q2. A list beside the code
  drifts from it, and nothing stops a refusal from being written without an
  entry.
- **Stripping ids and adding nothing.** That loses the route to the
  explanation.
- **A URL in every refusal.** It fails offline. A page may still give its
  URL.

## Design background

- Issues: issue 2305 (ids in user-tier messages); issues 2443 and 2547
  (runtime, colcon and assembled messages, resolved); issue 2551 (the shapes
  the id gate cannot yet read).
- RFCs: RFC-0116 D2; RFC-0117 (command tiers); RFC-0123 D3; RFC-0124
  Principle 5.
- Prior art: rustc's `error[E0277]` and `rustc --explain E0277`, whose
  explanations are compiled into the compiler and come from one registry
  (`rustc_error_codes`).
