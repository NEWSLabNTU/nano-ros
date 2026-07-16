# /audit skill — automated periodic codebase audit

Date: 2026-07-16 · Status: approved (design dialogue in-session)

## Goal

Automate the recurring quality / architecture / UX audit as a repo skill
(`/audit`), codifying the existing
[`codebase-audit-checklist.md`](../../development/codebase-audit-checklist.md)
run modes so an audit is one command instead of a hand-assembled session.

## Decisions (from the design dialogue)

- **Vehicle:** local user-invoked skill (`.claude/skills/audit/`), NOT a
  scheduled cloud agent and NOT a CI lane. The operator controls when and how
  deep.
- **Output:** findings report + auto-filed issues. The skill writes
  `docs/development/audit-findings-<YYYY-MM-DD>.md`, diffs against the
  previous findings log, and files a numbered issue per confirmed P1/P2 not
  already tracked. P3s stay in the findings doc. Commit lands locally; the
  skill never pushes.
- **Depth:** `/audit [quick|deep] [categories]`. `quick` (default) =
  checklist detection greps + a few parallel reader agents to confirm/kill
  hits. `deep` = Workflow fan-out, one agent per checklist category with an
  adversarial verify pass. Optional category filter (e.g. `/audit deep C,I`).
- **Checklist extensions:** (J1) copy-out example cleanliness — no low-level
  or macro boilerplate a user would wrongly copy; (C4) configuration-hierarchy
  conformance vs RFC-0049; (F3) bootstrap-doc drift — static cross-read of the
  book's setup pages vs `activate.sh` / `justfile` / `nros-sdk-index.toml`.
- **Clean-system bootstrap:** a REAL pristine-container run of the book's
  setup steps cannot be a grep — filed as its own issue (#204, runner-class
  work like #200). `/audit` only carries the static F3 drift check.

## Components

1. `.claude/skills/audit/SKILL.md` — the skill. References the checklist as
   the single source of categories/greps; does not duplicate them.
2. `docs/development/codebase-audit-checklist.md` — gains J1, C4, F3.
3. `docs/issues/0204-*.md` — the containerized bootstrap-verification probe.

## Flow (both depths share the tail)

1. Parse args → depth + category set (default: all).
2. Gather baseline: latest `audit-findings-*.md` + open/archived issue titles.
3. Collect candidate findings (grep+readers for quick; Workflow fan-out +
   adversarial verify for deep).
4. Dedupe vs baseline → classify **new / carried-over / resolved-since-last**.
5. Write the findings doc (ranked P1→P3, classification marked).
6. File issues for confirmed P1/P2 not already tracked — fetch origin and
   take max(issue id)+1 across open+archived first (multi-session rule);
   update the issues README.
7. Commit locally (findings doc + any issues). No push.

## Validation

One real `/audit quick` run end-to-end after landing: report produced,
classification section present, no issue filed unless a finding is real.
