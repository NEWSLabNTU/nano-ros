# Phase 452 — what a new user copies, and what builds it

**Status (2026-09-11). Opened to give five homeless issues one owner. Nothing in
this phase has landed; W1–W5 are open. W5 is blocked on a decision, not on
effort, and says so.**

## Why this phase exists

Everything a user meets first — the book pages they read, the templates they
copy, the scaffold `nros new` emits, the rustdoc they browse — is authored by us
and built by no lane. Five open issues say so, and none had a phase.

The pattern that makes them one phase rather than five chores is in
[#1107](../issues/1107-book-teaches-entry-pkg-per-target.md)'s own headline: **a
book page is what a new user copies, so a stale one propagates.** These surfaces
are the only part of the tree where being wrong is *replicated* rather than
merely observed, and they are precisely the part with no compile step:

| issue | the surface | what checks it today |
| --- | --- | --- |
| [#1058](../issues/1058-scaffold-output-is-grepped-never-built.md) | `nros new` scaffold output | ~30 substring assertions; nothing compiles the result |
| [#1108](../issues/1108-templates-materialize-dead-entry-pkgs.md) | four copy-out templates | nothing — two declare no `[image.*]`, so `nros build` refuses them outright |
| [#1107](../issues/1107-book-teaches-entry-pkg-per-target.md) | book pages | nothing — an out-of-tree consumer was scaffolded from the retired shape |
| [#1116](../issues/1116-rustdoc-diagnostics-outside-the-published-crate-set.md) | rustdoc outside the six published crates | nothing — ~70 diagnostics, five crates fail to document at all |
| [#1141](../issues/1141-book-visual-identity-favicon-logo-accent-css.md) | the book's front door | nothing — no favicon, logo or accent CSS |

Three of the five (#1107, #1108, and the consumer scaffolded from them) are one
regression with three faces: phase-383 W9/W10.a retired the per-target entry
package — *an image is a ROW, not a directory* — and retired it across
`examples/workspaces/**`, gated by `check-no-tracked-workspace-roots`. The
templates and the book were outside that gate's reach by construction, so they
still teach the shape `nros build` refuses. That is the
[phase-450](phase-450-gate-reach-narrower-than-its-rule.md) class arriving in
the one place where the output is copied by a stranger.

## The shape, stated once

**A surface a user copies must be built by a lane, not asserted about.** A
substring match answers "did we emit the string we meant to emit", which is a
question about the template and not about the scaffold; a user's first build is
the first compile the emitted code ever gets.

## Work items

Ordered so the two that hand a user a broken tree come first.

### W1 — the templates build

[Issue 1108](../issues/1108-templates-materialize-dead-entry-pkgs.md). Four
copy-out templates still materialize a `robot_entry` package; two declare no
`[image.*]` at all.

- [ ] The four templates carry the `[image.*]` shape `nros build` reads.
- [ ] A lane copies each template out and builds it. Copying out is the point —
      a template built in place is not the thing a user gets.

### W2 — the scaffold compiles in its own test

[Issue 1058](../issues/1058-scaffold-output-is-grepped-never-built.md). A
template can name three undeclared types and every test passes.

- [ ] Every scaffold variant is COMPILED by its test, not grepped.
- [ ] The three undeclared types the issue found are a red before they are a
      fix — a test that passes on the broken input proves nothing about the
      new one.
- [ ] No compilation inside the test process: this is a build-stage fixture and
      the test consumes the artifact, per CLAUDE.md's rule.

### W3 — the book teaches the shape the tree builds

[Issue 1107](../issues/1107-book-teaches-entry-pkg-per-target.md). The evidence
that the tree moved is already in the workspaces: none of
`examples/workspaces/{c,cpp,rust,mixed}/` tracks a root build file.

- [ ] The pages teaching one Entry package per deploy target are rewritten to
      the row shape.
- [ ] The `probe=NN` bootstrap mechanism (`just probe bootstrap`) covers at
      least one page that carries the new shape, so the book's own claim is
      executed rather than proofread.

W1 and W3 land together or the book and the templates disagree, which is the
state they are in now.

### W4 — rustdoc is clean where the tree is documented, or says where it is not

[Issue 1116](../issues/1116-rustdoc-diagnostics-outside-the-published-crate-set.md).
`just check rustdoc-links` runs on a pull-request lane for the **deployed** six
crates, deliberately — that scope is what keeps the docs deploy green.
Workspace-wide there are ~70 diagnostics and five crates that
`could not document` at all.

- [ ] The five that fail to document, document.
- [ ] The remaining diagnostics are either fixed or held by a ratchet that may
      only shrink. A ratchet, not a widened lane: the deployed-set lane exists
      to keep the docs deploy green and must not be made slower or redder for
      crates the book does not publish.

### W5 — the front door has an identity

[Issue 1141](../issues/1141-book-visual-identity-favicon-logo-accent-css.md).
What is left of archived phase-188: workstream 188.B, two theming files in
`book/theme/` wired through `[output.html]`.

**Blocked on a decision, not on effort** — it was scoped and deferred pending a
logo decision nobody has made in three and a half months. Naming it here is the
point: it is not in progress, and it will not start itself.

- [ ] A logo decision, recorded.
- [ ] Favicon, logo and accent CSS wired; `mdbook build book` still clean.

## Acceptance for the phase

* Every copy-out template and every scaffold variant is built by a lane, from a
  copy made the way a user makes it.
* No book page teaches a shape `nros build` refuses, checked by building one.

## Non-goals

* The workspace examples themselves — phase-383 W10.a already retired the shape
  there and `check-no-tracked-workspace-roots` holds it. This phase is the
  surfaces that gate could not reach.
* Documentation content beyond the shape it teaches. W3 is about the build shape
  being current, not a rewrite of the book.
