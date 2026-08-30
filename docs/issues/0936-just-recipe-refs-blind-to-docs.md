---
id: 936
title: "`check-just-recipe-refs` never reads a document, so 129 `just <recipe>`
  call sites in docs/ and book/ name recipes that do not exist"
status: open
type: bug
area: tooling
related: [0660, 0196]
---

## What is wrong

`check-just-recipe-refs` (issue 0660) verifies that every `just <recipe>` names a
recipe that exists. Its scan set is `justfile`, `just/*.just` and
`.github/workflows/*.yml` — **no document is ever read**. Its own docstring says
so, and treats it as someone else's job:

> * `check-doc-refs` covers documents, not recipe references.

Nothing covers that gap. `check-doc-refs` validates links and paths, not recipe
names. So a document is the one place a `just` invocation can rot with no gate
watching, and documents are exactly where a human copies an invocation from.

This is the issue-0196 shape — a gate whose coverage is narrower than the rule it
enforces — and it is the second time for this gate. The `.github/workflows` arm
was added only after `just zenohd setup` sat dead in host-tests for days.

## Measured

Scanning `docs/**/*.md` and `book/src/**/*.md` (excluding `archived/`) plus
`README.md`, `AGENTS.md`, `CLAUDE.md` — 448 documents — against the real
namespace (139 root recipes, 19 modules) finds **129 unresolved references**.
The most frequent:

```
 16  just install-local        5  just full-matrix         4  just qemu-baremetal
  4  just rt-test              4  just run                 4  just new
  3  just build-examples-qemu  3  just setup-qemu-network  3  just build-zenoh-pico-arm
  3  just verify-proofs        2  just build-zenohd        2  just setup-network
```

`just build-zenohd` is the retired recipe whose twelve dead callers are the
reason 0660 exists. It was cleaned out of the justfiles and left standing in the
documents.

## Two things to get right before this can be a gate

**Prose false positives.** `just` is an English word: the scan above flags
`just the ...` three times. A document-aware scan must look only inside fenced
blocks and inline code spans, not running text. That alone is most of the work,
and getting it wrong is worse than not having the gate — a baseline with
`just the` in it teaches everyone to ignore the file.

**`packages/cli/` has its own justfile.** Its `CONTRIBUTING.md` resolves against
`packages/cli/justfile` (22 flat recipes, no `check` module), not the root
namespace. A scan that assumes one namespace reports every CLI doc as broken.
That workspace is why this was found: its CONTRIBUTING named four recipes that
never existed (`format-build-tools`, `check-build-tools`, `format-user-libs`,
`check-user-libs`) and one — `just check python` — that a phase-399 rename sweep
had wrongly rewritten from `just check-python`, a spelling correct in the root
namespace and meaningless in that one.

## What would close this

1. A markdown-aware extractor: code fences and inline code spans only.
2. Namespace selection by document location — root for `docs/`, `book/`, the
   top-level `*.md`; `packages/cli/justfile` for `packages/cli/**`.
3. Land it as a RATCHET over the 129 (they are real, but they are not this
   change's to fix), with the baseline reviewed rather than generated blindly —
   every entry should be a recipe that once existed, not a sentence.
4. `archived/` stays excluded: those are records of what was true then.

The five recipes this was found through are already fixed — `check-stack`,
`check-stack-elf`, `check-stack-c`, `check-stack-all` and
`check-tier-priority-plan-image` were the last recipes carrying a `check-`
prefix inside the `check` module, so they were invoked `just check check-stack`.
They are now `just check stack` and friends.
