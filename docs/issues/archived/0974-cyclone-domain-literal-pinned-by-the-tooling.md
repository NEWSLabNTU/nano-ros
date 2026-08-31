---
id: 974
title: "`nros new` and the book both pinned `CONFIG_NROS_CYCLONE_DOMAIN_ID=0`, the one thing CLAUDE.md says never to do"
status: resolved
type: bug
area: cli, docs, rmw-cyclonedds
related: [issue-0161, issue-0972, issue-0801]
---

## What was wrong

`zephyr/Kconfig:209` declares the knob carefully:

```kconfig
config NROS_CYCLONE_DOMAIN_ID
    int "DDS domain ID"
    default NROS_DOMAIN_ID
    range 0 232
```

`default NROS_DOMAIN_ID` is the whole safety property: the Cyclone domain
TRACKS the generic one unless somebody deliberately separates them. `range 0
232` already respects the ROS spec.

Two places wrote a literal over that default:

* `packages/cli/nros-cli-core/src/cmd/new_entry.rs:612` — the `nros new`
  template for the cyclonedds backend, so **every generated project** carried
  `CONFIG_NROS_CYCLONE_DOMAIN_ID=0`;
* `book/src/getting-started/zephyr.md:188` — the getting-started conf, so the
  documentation taught the pattern.

A user who then sets `CONFIG_NROS_DOMAIN_ID=5` gets an image running Cyclone on
0. Nothing reports an error, because the domain is just the first element of
every discovery key — the peer simply never appears (issue 0801). That is
phase-180's split-brain (issue 0161) reintroduced by the tooling, and CLAUDE.md
has carried *"never pin it to a literal in confs"* ever since.

## Why a document was not enough

The rule existed, was correct, and was written in the two files agents read
most. It still got re-broken — by a code generator and a tutorial, neither of
which anybody re-reads while editing the other. **A convention that only lives
in prose is re-broken by tooling.**

## Fix

Both literals removed, so the Kconfig default does what it was written to do.
Nothing else changes: an image that genuinely wants a separate Cyclone domain
still sets it, and the ASI consumer the knob exists for is unaffected.

Gated by `check-cyclone-domain-not-pinned.py` on the fast line. It allows the
declaration in `zephyr/Kconfig` and prose in `docs/`, `CLAUDE.md` and
`AGENTS.md`, and takes a `nros-allow-cyclone-domain-pin` marker for an image
that deliberately separates the two — so the escape hatch is explicit rather
than a guess about intent.

Verified non-vacuous: run against the pre-fix tree it names both sites; after
the fix it passes. Self-test covers the match, the spacing variant, a bare
mention, the Kconfig declaration, and the allow-list boundaries.

## Not done: the knob itself

The knob was called cumbersome, and there is something to that — it is a second
spelling of "which domain am I on", kept safe by a default rather than by
construction. But removing it is not this issue's call: `main.hpp:320` selects
it over the generic knob specifically to match an out-of-tree consumer (ASI),
and nothing here measures what that consumer needs. What this fix does is remove
the way it silently goes wrong. Whether one knob can replace two is a separate
question, and it wants that consumer in the room.

## Acceptance

* ~~No tracked conf or template pins the Cyclone domain to a literal.~~ Met.
* ~~The default in `zephyr/Kconfig` is what generated projects get.~~ Met.
* ~~The rule is enforced rather than documented.~~ Met, and the gate is proven
  against the defect it was written for.
