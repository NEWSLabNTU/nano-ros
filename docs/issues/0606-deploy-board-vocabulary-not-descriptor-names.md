---
id: 606
title: "`[deploy.*].board` and a descriptor's `names` are different vocabularies —
  three consumers now work around it, and `nros sync` silently skips those leaves"
status: open
type: bug
area: cli
related: [issue-0440, rfc-0072, phase-341, phase-351]
---

## Symptom

Every in-tree site block says

```toml
[deploy.mps2-an385-freertos]
board = "mps2-an385-freertos"
```

and that board's descriptor does not list that spelling:

```toml
# packages/boards/nros-board-mps2-an385-freertos/nros-board.toml
names = ["freertos", "freeRTOS", "FreeRTOS"]
```

`BoardCatalog::resolve_deploy` matches on `names`, so those deploys resolve to
NOTHING. `nros sync` reports it once, at the end, as a count:

```
sync: board configs — N deploy key(s) resolve to no single board
```

and skips their board projections. The leaf keeps whatever it has; nothing
fails; the next reader sees a green sync.

## Three workarounds, no fix

Discovered three separate times during phase-351, each time papered over locally
because the alternative was refusing a board the tree can plainly identify:

1. `scripts/check-site-config.py` keys its netstack map by BOTH the declared
   `names` and the directory (`packages/boards/nros-board-<x>` → `<x>`).
2. `nros ws board-facts`'s `resolve_board()` does the same fallback in Rust.
3. The standalone-leaf path maps `[package.metadata.nros.entry] deploy` onto the
   board directly, because those manifests carry no `board =` key at all — a
   THIRD spelling of the same relation.

Three workarounds and no issue is how a convention rots: each is correct
locally, and together they mean no single place decides what a board is called.

## What is actually undecided

Whether `[deploy.*].board` names a BOARD PACKAGE (the directory) or a board
ALIAS (a `names` entry). phase-341 W3 closed part of this by adding deploy
spellings to `names` — `qemu-mps2-an385`, `rtic-mps2-an385`,
`threadx-qemu-riscv64`, `qemu-esp32-baremetal` are all there for exactly this
reason — but the set was completed by inspection, not by a rule, so the ones
nobody hit are still missing.

## Fix shapes (pick one, then delete the workarounds)

* **Extend `names`** for every `[deploy.*].board` value in the tree, and gate
  that every deploy value resolves. Cheapest, and matches what phase-341 W3
  already did; the risk is the next board added out of band.
* **Make the directory an implicit alias** in `BoardCatalog` — one resolution
  rule, and the three workarounds collapse into it. Watch for ambiguity: one
  directory serves several witnesses (the two NuttX boards).
* **Declare it the other way**: `[deploy.*]` names the board PACKAGE, and
  `names` is only for humans. Biggest change, clearest result.

## Acceptance

* one place decides how a `[deploy.*].board` maps to a descriptor;
* `nros sync` resolves every in-tree deploy, or names the one it cannot and
  fails rather than counting it;
* the three fallbacks above are deleted, not left as a fourth opinion.
