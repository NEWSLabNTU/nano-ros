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
  baseline zero, as
  [issue 2551](../issues/archived/2551-user-surface-text-beyond-function-flow-cites-ids.md)
  (the shapes those readings could not see) is too.
  Codes are prepended to those messages.

## Problem

A refusal that a user meets has to answer three questions: what went wrong,
what to do, and where to read more. Today it answers the third with an
internal id, or with nothing.

- RFC-0116 D2 (consolidated in RFC-0124 Principle 5) rules that the user
  surface never names a phase, issue or RFC id.
- phase-517 W5 exempted a build that stops. On 2026-11-11, about 100 CMake
  `FATAL_ERROR` texts and about 25 build-script panic texts cited an issue id
  ([issue 2305](../issues/archived/2305-user-tier-messages-cite-internal-issue-ids.md)).
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
race that collided seven times for issue ids. A number whose reservation
fails is not allocated: there is no local fallback, and the pre-push guard
refuses a new number with no reservation. The six founding codes, allocated
before reservations existed, are the one recorded exception.

### D4 — Every other view is generated from the crate, and drift-gated

A generator binary in the crate (`nros-diagnostics-gen`) writes each view.
Each view's gate re-runs the generator with `--check`.

| view | path | consumer |
| --- | --- | --- |
| explain text | compiled into the CLI (the crate is a dependency) | `nros explain` (D5), offline |
| book reference | `book/src/reference/diagnostic-codes.md` (one page: an index, then a section per code; amended 2026-11-14 from one page per code, by user ruling) | readers; RFC-0124's reference section |
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

## Amendment, draft for ruling (phase-578 W4): a refusal points at its source, and `nros fix` applies the mechanical ones

Not ruled. Drafted 2026-11-18 by nros 118 for phase-578 W4; the questions at
the end are what a ruling settles. D1 to D6 are unchanged.

### What happens today

Configuration is read with serde into plain structures (`leaf_system::read`,
the board catalog, `nros-platform.toml`), and a refusal is raised after that
read. By then the source position is gone, so a refusal names a file and a
key at best and never a line: a retired knob spelling under
`[image.<id>] env` is refused by `refuse_retired_env_keys`, which sees only
the map. Measured shapes of a mechanical remedy that exist today:

- a retired spelling, renamed (`ZPICO_MAX_QUERYABLES` to
  `NROS_ZENOH_MAX_QUERYABLES`, the knob registry's retired rows);
- a key in the wrong table (an `[image] env` knob row that belongs in
  `[image.<id>.knobs.<group>]`, phase-484 W8);
- a value spelled one way the parser no longer takes (`native`/`posix` for
  the host board, phase-500 W5).

### D7 — a refusal carries the place it is about

A refusal about configuration a person wrote carries a `Location`: the file,
the 1-based line and column, the span's length and the line's text. The crate
stays `no_std`; `Location` holds borrowed strings.

```rust
Diag::RetiredKnobSpelling
    .refuse(&format_args!("`ZPICO_MAX_QUERYABLES` is retired"))
    .at(&loc)                       // new
    .help(&"set NROS_ZENOH_MAX_QUERYABLES instead")
```

Rendered, in rustc's shape (the code line from D2 stays first):

```text
error[NROS-0xxx]: `ZPICO_MAX_QUERYABLES` is retired
  --> src/app_bringup/system.toml:9:7
   |
 9 | env = { ZPICO_MAX_QUERYABLES = "4" }   # a retired spelling
   |         ^^^^^^^^^^^^^^^^^^^^
  help: set NROS_ZENOH_MAX_QUERYABLES instead
  fix: nros fix   (renames the key; prints the diff first)
  explain: nros explain NROS-0xxx
```

Where the location comes from: NOT from threading `Spanned<T>` through every
configuration structure. The CLI keeps each configuration file's text beside
the parsed value (`SourceFile { path, text }`), and a refusal site that knows
its key path (`image.linux.env.ZPICO_MAX_QUERYABLES`) asks
`SourceFile::locate(key_path)`, which re-reads the text with `toml_edit`
(already a dependency) and returns the key's or the value's span. Cost is
paid only on the refusal path; the serde structures do not change, so no
reader of them changes.

A refusal about a comparison names both sides. The primary location is the
line the user wrote (the image's); a secondary one, `.note_at(&loc, "the
board states this")`, is the line that states the fact it was compared with
(the board definition's `threads = false`), rendered as rustc renders a
secondary label:

```text
error[NROS-0yyy]: image `talker` needs threads; board `acme` has none
  --> src/app_bringup/system.toml:12:9
   |
12 | rmw = "zenoh"
   |       ^^^^^^^
  note: the board states this
  --> boards/nros-board-acme/nros-board.toml:7:11
   |
 7 | threads = false
   |           ^^^^^
  help: choose rmw xrce, or a board with threads
```

Several refusals about one thing render as a GROUP: one header (`image
`talker` cannot build for board `acme``), then each child as a full
diagnostic with its own code, location(s) and remedy. A group is a refusal
with children (`Refusal::child`); it carries no code of its own, and
`nros fix` collects fixes from every child the same way it does from a lone
refusal (RFC-0135 D8's pre-build check is the first group).

Only the CLI surface carries a location in this amendment. A build script or
CMake refusal about a file it read can adopt the same shape later; the
runtime surface has no file.

### D8 — `nros fix`

A refusal whose remedy is mechanical carries a `Fix`: the file, the key path,
and one edit of four kinds, the shapes measured above plus the one a missing
key needs:

- `RenameKey { to }`;
- `MoveKey { to_table }`;
- `ReplaceValue { from, to }`;
- `SetKey { table, key, value }`, for a key that is absent (an image that
  states no transport and gets the default, where the remedy is
  `transport = "serial"` in `[image.talker.network]`, RFC-0135 D8).

`nros fix [<path>]` loads the workspace the way `nros build` does, collects
every refusal that carries a fix, applies the edits with `toml_edit` (so
comments and layout are kept), prints a unified diff, and writes only with
`--apply` (or after a yes on a terminal). It never builds. A refusal whose
remedy needs judgement carries no `Fix` and stays a `help:` line. A refusal
that has a fix says so on its own line (`fix: nros fix`), so the reader of a
failed build learns that the verb exists from the place they need it.

Two fixes that touch one key, or a fix whose result is itself refused, stop
`nros fix` before it writes anything, naming both; it does not apply a
partial set.

`nros fix` edits only files under the workspace's roots. A board definition
or package reached from an installed prefix (`/opt/ros/...` through the
ament index) is not the user's to edit: a refusal about such a file carries
no `Fix`, and its remedy renders as `help:`, never `fix:`. `nros fix` refuses
a `Fix` naming a file outside the roots rather than writing it.

**Remedy labels.** A remedy line says which kind it is, so a reader knows
what `nros fix` will do before running it:

- `help:` a remedy that needs judgement (`choose rmw xrce, or a board with
  threads`); `nros fix` does nothing with it;
- `fix:` only when the refusal carries a `Fix`; `nros fix` applies it;
- `run:` a command to run (`nros setup acme`); `nros fix` never executes it,
  and never starts an installer.

RFC-0135 D8's board-package remedies use the same three labels and the same
`Fix` payload, so there is one remedy format.

### D9 — gates

- `check-diagnostics` gains: an entry that declares a fix kind has an
  explanation that names `nros fix`.
- A fixture per fix kind (all four, `SetKey` included): a `system.toml`
  carrying the refused shape;
  `nros build` refuses at the right line and column; `nros fix --apply`
  rewrites it; the rewritten file builds. That is phase-578 W4's acceptance
  ("a retired knob spelling in `system.toml` is reported at its line, and
  `nros fix` rewrites it") as a test.

### Questions for the ruling

1. **Scope of D7.** Locations for every configuration file the CLI reads
   (`system.toml`, `nros-board.toml`, `nros-platform.toml`, `package.xml`,
   launch files), or `system.toml` and `nros-board.toml` first? Draft:
   those two first; the rest as their refusals are touched.
2. **`nros fix` writes without `--apply` on a terminal after a prompt, or
   only ever with `--apply`?** Draft: diff by default, write only with
   `--apply`, so a script and a person get the same behaviour.
3. **Does `nros build` offer the fix inline** (apply and continue), or only
   name the verb? Draft: only name it. A build that edits the user's files
   is a surprise.
4. **Board package `fix:` lines** (RFC-0135 D8): the same `Fix` payload, so
   `nros fix` applies them too? Draft: yes, once RFC-0135 lands; this
   amendment defines the payload and the `help:` / `fix:` / `run:` labels
   they use.

Reviewed 2026-11-18 by nros 241 (RFC-0135's author) against RFC-0135 D8:
direction approved; its five changes are in this draft (the `run:` label,
`SetKey`, the secondary location, groups, workspace-only edits). Questions
1 to 4 agreed as drafted. Still for the user's ruling.

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
  those readings could not see, resolved).
- RFCs: RFC-0116 D2; RFC-0117 (command tiers); RFC-0123 D3; RFC-0124
  Principle 5.
- Prior art: rustc's `error[E0277]` and `rustc --explain E0277`, whose
  explanations are compiled into the compiler and come from one registry
  (`rustc_error_codes`).
