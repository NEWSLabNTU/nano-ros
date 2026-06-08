# Design spec — docs/ RFC system + CLAUDE/AGENTS trim

Date: 2026-06-08
Status: approved (brainstorming → implementation)

## Problem

`docs/` had no single "finalized design" view and no way to tell which design decisions are
stable vs still evolving. Design rationale was scattered across ~30 design docs, 253 roadmap
phase docs, and inline lore in CLAUDE.md / AGENTS.md. Devs and agents could not answer "what is
the current shape of subsystem X, and is it settled?" without reading phase history.

## Goal

A trackable, Rust-RFC-flavored design-doc system suitable for git-repo developers **and**
agents, that surfaces (a) per-decision stability, (b) one finalized whole-system view, and
(c) the user workflow — while removing duplicated design lore from the auto-loaded instruction
files.

## Decisions (locked during brainstorming)

1. **Scope:** full `docs/` taxonomy + pull design lore out of CLAUDE.md / AGENTS.md.
2. **Model:** living docs + status frontmatter + Changelog (edit in place; not immutable-chain).
3. **Finalized view:** a hand-maintained `docs/design/ARCHITECTURE.md` narrative that links into
   RFCs. Drift rule: flipping an RFC to `Stable` requires updating the matching ARCHITECTURE
   section in the same commit.
4. **Roadmap relationship:** phases stay work-logs; RFCs own design rationale. Phase docs gain an
   `Implements: RFC-NNNN` header. New rule: design rationale goes in an RFC, never phase-only.
   No mass migration — lore migrates lazily as phases are touched.
5. **CLAUDE/AGENTS:** thin router files. Design lore + phase-history narratives route out to
   RFCs / ARCHITECTURE / roadmap. **Carve-out:** keep a one-line operational *pitfall index*
   inline (CLAUDE.md is auto-loaded; RFCs are not — losing the tripwires would regress agents).
6. **Filenames:** rename the ~30 active design docs to `NNNN-slug.md` (Rust-style). Fix inbound
   links in live files (book, active phases, CLAUDE, AGENTS, other design/reference/guide docs).
   Leave `docs/roadmap/archived/**` stale links untouched (frozen history).

## docs/ taxonomy

| Dir | Role |
| --- | --- |
| `docs/design/` | numbered living RFCs — design SSOT |
| `docs/design/ARCHITECTURE.md` | finalized whole-system narrative |
| `docs/design/0000-template.md` | RFC template |
| `docs/roadmap/` | work logs; link RFCs via `Implements:` |
| `docs/reference/` | mechanical specs (schemas, ABIs, interop) |
| `docs/guides/` | how-to + operational pitfalls |
| `docs/research/` | surveys, no normative weight |
| `docs/development/` | contributor process |
| `docs/superpowers/` | brainstorm specs/plans |

## RFC frontmatter

```
rfc, title, status (Draft|Stable|Superseded), since, last-reviewed,
implements-tracked-by: [phase slugs], supersedes: [], superseded-by: null
```

## RFC numbering (assigned)

0001–0004 foundations; 0005–0011 RMW & data plane; 0012–0017 platform/board/toolchain;
0018–0022 language APIs; 0023–0027,0030 codegen/workspace/user-workflow; 0028–0029 domain/safety.
Draft today: 0003, 0024, 0025, 0030. (Full map: `docs/design/README.md`.)

## Implementation phases

1. **Scaffold (this change):** renumber active design docs + add frontmatter; write
   `0000-template.md`, rewrite `README.md` as the RFC index, write `ARCHITECTURE.md`; fix inbound
   links in live files.
2. **Instruction trim (this change):** CLAUDE.md / AGENTS.md → routers + pitfall index; relocate
   deep platform/impl notes to a reference doc so nothing is lost; add the design→RFC rule and the
   ARCHITECTURE drift rule to AGENTS.md.
3. **Lazy (ongoing):** phase docs gain `Implements:` headers and shed inline design rationale into
   the owning RFC as they are next touched.

## Out of scope

- No mass migration of the 214 archived phase docs.
- No new RFC content authored from phase history up front (lazy only).
- No changes to book structure beyond link fixes.

## Risks & mitigations

- *Lost pitfall context* from CLAUDE.md trim → mitigated by the inline one-line pitfall index +
  relocating deep notes to `docs/reference/` rather than deleting.
- *ARCHITECTURE drift* → the Stable-flip-updates-ARCHITECTURE rule in AGENTS.md.
- *Stale archived links* → accepted; archived phase docs are frozen history.
