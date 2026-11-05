# Phase 537 — The Book as a user guide

**Status (2026-11-05). OPEN — not started; W0 (the RFC's tooling, the style guide and the type declarations) first.**
Implements [RFC-0124](../design/0124-book-user-guide-organization.md) D1 to
D12. Follows [phase-532](phase-532-two-books-by-audience.md), whose page map
and `book-move` tooling it reuses.

Depends on:

- [phase-531](phase-531-workspace-first-project-layout.md) W4 (`nros new <ws>`,
  `nros run <node>`, PR #3143) and W5 (one build road), for every
  Tutorial chapter (W5 here);
- [phase-535](phase-535-example-reorg-and-cadence-ci.md), for the example
  pointers of the platform pages (W8);
- a registry row for PX4 (RFC-0124 open question), for whether its page stays
  in the user book.

---

## Why

The published book has twelve parts, 111 pages, about 178,000 words, seventeen
stubs and eleven pages over 3,000 words, with no path through them (RFC-0124,
Motivation). This phase reorganizes it by what the reader is doing.

The order of the work matters for two reasons:

- **Cheap structure before expensive prose.** Moves, merges and the front door
  (W1–W3) make the book approachable within days and rewrite no text.
- **Prose after the CLI settles.** The tutorial and platform rewrites (W5, W8)
  depend on CLI work still in flight. Done now, they would be written twice.

## Work items

Effort: S ≈ 1 day, M ≈ 2–4 days, L ≈ a week or more. Each item lands as one or
more section-sized PRs, rebased onto main rather than merging main in
(RFC-0124 D11).

### W0 — Style guide, page templates, type declarations (S)

- A "Writing the Book" page in nano-ros Internals. It adopts the Google
  developer documentation style guide for voice and the Diátaxis types for page
  kinds, and gives one template per type: the how-to shape of RFC-0124 D5, the
  platform template of D8, a tutorial chapter, an explanation page and a
  reference page.
- A `<!-- type: … · level: … -->` declaration on every user page (D2), set from
  Appendix A of the RFC.
- `check-book-audience` reports, without refusing:
  - pages with no declaration;
  - a type that doesn't match the page's section;
  - pages outside the length budget (D9).

**Acceptance:** every user page declares a type and level, and the report runs
on the fast line.

### W1 — Page map v2: sections, moves, merges (M)

- New section directories: `start/`, `starters/`, `tutorial/`, `guide/`,
  `background/`, `reference/`, `extending/`, `migrating/`.
- `book/page-map.toml` rows follow RFC-0124 Appendix A. This item covers moves
  and stub merges only, with no prose rewrite.
- The seven stubs under 130 words fold into their parent pages.
- `rmw-vs-upstream`, `zenoh-pico-symbols` and `xrce-dds-symbols` move to
  Internals.
- **Fix `book-move` first,** so it remaps the board registry's book-relative
  fields (`chapter`, `book_run`, `book_flash`). phase-532 did that by hand,
  twice.

**Acceptance:**
- `book-move.py --plan` reports nothing pending;
- the sidebar has the eight sections;
- every old URL redirects;
- all book gates pass, including in a clean clone.

### W2 — The front door: choose your path, levels (S)

- `start/choose-your-path` with the five cards of RFC-0124 D10, each with its
  route and budget.
- The level badge rendered from the W0 declaration.

**Acceptance:** each card's route resolves to existing pages, and the route a
card states depends only on pages before it (D10).

### W3 — Split the mixed pages by type (M–L)

Split the eleven pages over 3,000 words, and every other page that mixes types,
into one page per type, linked to each other. The largest are:

- `targets/zephyr.md`: by status (D8);
- `reference/cli.md`, `build/configuration.md`,
  `from-ros2/porting-a-cpp-node.md`: task parts to the User guide, tables to
  Reference;
- `build/growing-your-workspace.md`: Tutorial steps and User guide.

**Acceptance:** no page outside Reference is over 3,000 words, and every page
has one type.

### W4 — Language tabs (M)

- An in-repo mdBook preprocessor for Rust | C | C++ tabs (D9), modelled on
  `scripts/mdbook-board-status.py` and registered in `book/book.toml`.
- Probe extraction (`extract-book-steps.py`) learns that a tabbed block is one
  step per language variant.
- `get-started/first-node-{rust,c,cpp}.md` become one page.

**Acceptance:**
- the docs workflow builds with the preprocessor;
- a tabbed page's probe track extracts each language;
- the three first-node pages are one.

### W5 — The Tutorial path (L)

Ten chapters (RFC-0124 D3):
- each on Verified rows only;
- each fits a two-hour lab;
- each a probe track on the installed track;
- each introducing its nano-ros concept from a concrete example (D4).

Chapter 4 (pick and configure the RMW) covers zenoh, Cyclone DDS and XRCE-DDS
in use (D5). Chapter 6 (services and actions) is new writing on the existing
`server`, `client` and `action` templates.

**Depends on:** phase-531 W4 (#3143) and W5; W4 here (tabs).

**Acceptance:**
- every chapter's probe track replays green with no `awaits`;
- every chapter states its board's generated status;
- an education adoption probe (RFC-0116 D9) completes chapters 1–7 within
  budget.

### W6 — The contract system (M)

- Tutorial chapter 8: write the contract for the talker and listener, see
  static RAM fall, read a refusal (RFC-0124 D6).
- The four Contracts and sizing how-tos, moved out of `build/cpp-workspaces.md`
  and written for every language.
- The contract format reference, generated from ros-launch-manifest's field
  table at the tag pinned in `packages/cli/nros-cli-core/Cargo.toml`. Either the
  docs job fetches it, or a committed copy is kept current by a gate that
  compares it with the pinned tag (the RFC's open question).

**Acceptance:**
- the reference page names the pinned tag;
- a gate refuses a copy that differs from that tag's table;
- chapter 8's measured RAM drop replays in its probe.

### W7 — Generated references (M)

- The Rust, C and C++ API pages become landing pages that link to the rustdoc
  and Doxygen output `docs.yml` already publishes.
- The CLI reference is generated from the CLI's help, either in the docs job or
  committed and gated, like the board pages.
- `system.toml` keys stay generated from `gen-config-surface.py`.

**Acceptance:** no hand-written reference duplicates a generated one, and
`reference/cli` cannot drift from `nros --help`.

### W8 — Platform pages by support level (M–L)

- Platform pages are split and grouped by generated status, each on the D8
  template.
- "Checked by hand" pages show their manual date.
- A **new** "Flash a real board" page, with a generated per-board hardware
  table.
- A gate checks that each platform page's group matches its row's status word.
- PX4's page stays or goes with its registry row.

**Depends on:** phase-535 (example pointers), the PX4 registry row.

**Acceptance:**
- every platform page sits in the group its status word names;
- no user page describes a board or feature with no registry row.

### W8b — Board starters (M–L)

One starter per supported board or platform under `starters/` (RFC-0124 D12), on
the starter template: what you need, provision, scaffold, run in the emulator
or twin, flash the board, see it in ROS 2, if it doesn't work, next.

- **Twin-and-hardware boards first:** Nucleo-F767ZI (Renode twin) and
  Raspberry Pi Pico W (Wokwi twin). For the field and education purposes, the
  same image runs in the twin and then on the board.
- **Then the Verified in emulator boards, then the Verified on host ones.**
- **The list, order and headers are generated from the registry:** a twin and
  `hardware` on the row, or the status word. A row that changes level changes
  slot on regeneration.
- **Each starter's steps are probe fences.** Twin starters replay through the
  twin tracks: `renode-nucleo` today, and a Wokwi track where its
  `WOKWI_CLI_TOKEN` secret is set, failing loudly where it isn't.
- **The Pico W starter is new writing.** The Nucleo starter comes from
  `targets/nucleo-f767zi.md`; the others from the starter parts of the platform
  chapters (W3).

**Depends on:** phase-531 W4 (the scaffold the starters use); W3 (the platform
splits); W4 (tabs, where a starter shows more than one language).

**Acceptance:**
- every board whose row is Verified, or has a twin and `hardware`, has a
  starter;
- each starter depends only on Install;
- each starter's probe track replays, or fails naming its unmet precondition.

### W9 — Tighten the gates (S)

W0's report becomes a refusal:
- a missing or mismatched type;
- a page under 300 words;
- a page outside Reference over its budget;
- a platform page in the wrong status group.

**Acceptance:** `check-book-audience` refuses each of these, with a self-test
case for each.

## Order

```
W0 ──► W1 ──► W2
        ├───► W3
        └───► W4 ──► W5 (also needs phase-531 W4/W5) ──► W6 (chapter 8)
W6 (how-tos, reference) and W7 can start after W1
W8 after W3 and phase-535; W8b after W3 and phase-531 W4
W9 last
```

## Acceptance for the phase

- The user book has the eight sections of RFC-0124 D1, about 80 pages, no page
  under 300 words, and no page outside Reference over 3,000 words.
- Every user page declares one type and level, and the gate refuses otherwise.
- The Tutorial's ten chapters replay green on Verified rows.
- The contract, CLI and API references are generated from their sources.
- Every platform page's group matches its board's generated status word.
- Every supported board has a starter, twin-and-hardware boards first, each
  replaying through its probe track.

## Not in scope

- New platforms or boards: the registry decides what has a page.
- Rewriting the Internals book's structure; only pages moved into it are
  touched.
- Translations.
