---
id: 1519
title: "A Zephyr image's `board` only reaches `west -b` correctly when it authors
  the framework's own board string, which is the one thing the field forbids"
status: open
type: bug
area: [cli, examples]
severity: medium
found: 2026-09-27
related: [1517, 1288, 0606]
---

## What this is

`ImageBlock::board` states its own rule:

> nano-ros board id — resolved through `packages/boards/board-support.toml`
> (RFC-0065 D9). **NEVER a framework's own board string**: the registry carries
> `framework_board` for platforms that have one, so `native_sim/native/64` is a
> resolution RESULT, not something authored here.

For a Zephyr image, `nros build` passes that authored string to
`west build -b` unless the descriptor says otherwise. Issue 1517 fixed half of
that — `BoardDescriptor::west_build_board` now reads `[board.zephyr] west_board`,
so the three descriptors that state one project correctly. The `zephyr`
descriptor is the remaining half: it states **no `[board.zephyr]` table at all**
(`packages/boards/zephyr/nros-board.toml`) and instead carries the Zephyr id as
a second NAME:

```toml
names = ["zephyr", "native_sim/native/64"]
```

which is exactly the smuggling `BoardZephyr::west_board`'s doc-comment was added
to retire ("Note it is also what `names` was being used to smuggle").

So, measured over the 269 board-bearing `[image.*]` rows in the 163
`examples/**/system.toml` (2026-09-27; `[deploy.*]` carries no board in
`examples/` any more — zero rows):

* **23 rows** write `board = "zephyr"`. Through `nros build` those reach
  `west build -b zephyr`, a board west has never heard of.
* **10 rows** write `board = "native_sim/native/64"`. Those reach the right `-b`
  — *because* they author the framework string the field forbids.
* **1 row** writes a nano-ros board id for a non-native_sim Zephyr board:
  `[image.fvp]`, after issue 1517. It works only because that descriptor states
  `[board.zephyr] west_board`.

Both readings of the rule are in the tree at once, and the one that works is the
one that is wrong. The two written rules disagree too, which is how that happened:
`ImageBlock::board` says "NEVER a framework's own board string", while
`check-deploy-board-resolves`'s own failure text says "A `[deploy.*].board` names
the DOWNSTREAM ecosystem's board" — and that gate reads `[image.*]` rows as well
(issue 0951), so it advises the opposite of the field it is checking. `[deploy.*]`
and `[image.*]` may legitimately differ here; nothing says which.

## Why nothing catches it

The Zephyr fixtures and lanes do not go through `nros build`: `just zephyr
build-*` calls `west build` itself with an explicit `-b`, or with none where a
board crate supplies it. So the `-b` `nros build` would emit is never executed
by anything in CI, and a row's board is never compared against the board its
image is actually built for. Issue 1517 is the same blind spot one board over.

## What a fix has to decide

Stating `west_board = "native_sim/native/64"` under `[board.zephyr]` and
dropping the second name would make `board = "zephyr"` project correctly and
make the 10 framework-string rows redundant — but it changes the `-b` for 23
rows, and `native_sim/native/64` is load-bearing in more than the descriptor:
`BOARD_KEYS` carries it (`board_key_table.rs` documents why it must, phase-445
W5), `check-derived-descriptor-fields` reads the descriptors, and
`build_verb_pipeline.rs` asserts the literal `west build -b native_sim/native/64`.
Acceptance is a Zephyr BUILD, not a gate — which is why 1517 filed this rather
than guessing at it.

Worth settling at the same time: the OUTER `BoardDescriptor::west_board` is
declared by nothing in the tree and now has no reader that `[board.zephyr]`
would not serve. If the answer is one field, it should be one field.
