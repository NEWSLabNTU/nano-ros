---
id: 1519
title: "A Zephyr image's `board` only reaches `west -b` correctly when it authors
  the framework's own board string, which is the one thing the field forbids"
status: resolved
type: bug
area: [cli, examples]
severity: medium
found: 2026-09-27
resolved: 2026-10-03
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

## Resolution (2026-10-03)

### Re-measured on main before the change

Over the 285 board-bearing rows in the 189 tracked `system.toml` files
(`[deploy.*]` still carries no board anywhere; `tmp/`-script census, every
`[image.*]` / `[image_defaults]` / `[deploy.*]` `board`):

* **Half (a) — 24 rows `board = "zephyr"` — was already FIXED** by phase-470
  W5.a (PR #1367), which gave the `zephyr` descriptor
  `[board.zephyr] west_board = "native_sim/native/64"`, read through
  `BoardDescriptor::west_build_board`. Measured with `nros build <img>
  --dry-run`: `examples/zephyr/rust/talker` (`zephyr`) and
  `examples/workspaces/realtime-rust` `demo_bringup:zephyr` both print
  `-> board zephyr … west build -b native_sim/native/64`. (24, not 23: the
  issue's count predates one more row.)
* **Half (b) — 13 rows, not 10.** The 10 counted in `examples/` (rust ×2,
  c, cpp ×2, mixed, realtime-c ×2, realtime-cpp, derived-tiers-cpp) plus 3
  test fixtures the census here reached and the issue's did not:
  `multi_pkg_workspace_zephyr/demo_bringup` and both
  `zephyr_self_pkg/{self,sibling}/alpha_pkg`. And two PRODUCERS of the same
  spelling: `nros new entry` defaulted `--board` to `native_sim/native/64` and
  wrote it into the user's `[image.*]`, and the CLI/book docs taught it.

### What landed

* **The descriptor states the Zephyr id once.** `packages/boards/zephyr/
  nros-board.toml` is `names = ["zephyr"]` + `[board.zephyr] west_board`, and
  its `package.xml` stops announcing the second name
  (`check-provider-announcements` holds the two equal).
  `BoardDescriptor::answers_to` already derived the `[board.zephyr]` id, so
  `native_sim/native/64` still RESOLVES — a `[deploy.*].board` may name the
  downstream id (issue 0606) — it just is not a name any more.
* **An image authoring a framework id is REFUSED**, naming the id to write:
  `image::refuse_framework_board`, applied by `resolve_image_board` (`nros
  build`, the planner, `nros new entry`) and by `tier_resolver`'s image arm
  (`codegen-system`). The framework id is `BoardDescriptor::framework_board()`
  (the board-agnostic `west_board`, else `[board.zephyr] west_board`), which
  `west_build_board` now reads too — one accessor, not two `or_else` chains.
  A descriptor whose ONLY name is its framework id is accepted, since refusing
  would leave no legal spelling.
* **The 13 rows migrated to `board = "zephyr"`** (`nros board`'s catalog id
  for that descriptor; it has no other).
* **The second key left the entry tables**: `nros_orchestration_ir::
  BOARD_PATHS` / `framework_for_board_key` and `nros_entry_lower::BOARD_KEYS`
  carried `native_sim/native/64` only because bringups authored it
  (`board_key_table.rs` said so). Keeping it would leave `nros::main!` accepting
  what `nros build` refuses; it is now in `an_unknown_key_errors_at_every_
  consumer`'s list instead.
* **`nros new entry`** defaults `--board zephyr`, resolves it through the
  catalog, and names Zephyr's `boards/<west board>.conf` from the RESOLVED id.
* **Docs**: RFC-0085 D7 carries an amendment note; the book's
  `integration-zephyr.md` and `workspace-entry-pkg.md` teach `board = "zephyr"`.

The OUTER `BoardDescriptor::west_board` stays: RFC-0085 D9 documents it for a
workspace-local board (`names = ["my-board"]`, `west_board = "qemu_cortex_m3"`)
and the book teaches that shape, so deleting it would break a documented user
surface for no in-tree gain. It is now one input to `framework_board()`, not a
second rule.

### Verified

* **`-b` unchanged for every migrated row** — `nros build <img> --dry-run`
  before (a CLI built from unmodified main) and after (this change), full `west build …` line
  compared byte for byte: identical for all 11 rows the dry-run reaches
  (`examples/workspaces/{rust ×2, c, cpp ×2, mixed, realtime-c ×2,
  realtime-cpp, derived-tiers-cpp}` + `zephyr_self_pkg/self`), each
  `west build -b native_sim/native/64`. The other two fixture rows
  (`multi_pkg_workspace_zephyr`, `zephyr_self_pkg/sibling`) fail the dry-run
  identically before and after (`names entry … which is not a package in this
  workspace` — their applications are built by `west-fixtures.sh` with an
  explicit `-b`, not by `nros build`); their board feeds only
  `codegen-system`'s tier RTOS, whose `zephyr` answer is unit-tested.
* **A real Zephyr build**: `nros build demo_bringup:zephyr` in
  `examples/workspaces/rust` (generated Rust entry, so `nros::main!` reads the
  migrated board through the narrowed key table) — `zephyr.exe`
  links, `CMakeCache` `BOARD=native_sim/native/64`, `.config`
  `CONFIG_BOARD_NATIVE_SIM_NATIVE_64=y`, and it boots (`*** Booting Zephyr OS
  build v3.7.0 ***`, then the expected no-router `ConnectionFailed`).
  And a C one, `examples/workspaces/c` `demo_bringup:zephyr` (generated C
  entry, so `entry_pack_for` / `BOARD_KEYS` read `zephyr`): `zephyr.exe`
  links, `BOARD=native_sim/native/64`, boots the same way. Both builds ran in
  this worktree's own `cp -al` copy of `zephyr-workspace/` and the SDK.

### Gate

`check-deploy-board-resolves` (fast line) was the natural home — it already
read every `[image.*]` board — and it had both defects this issue names:

* its failure text said "a `[deploy.*].board` names the DOWNSTREAM
  ecosystem's board … the descriptor must CLAIM that spelling in its `names`"
  about rows that are mostly `[image.*]`, i.e. it prescribed the smuggling.
  It now states the two tables' different rules;
* its alias map read `names` + the directory only, NARROWER than
  `answers_to` (0196's shape), which is half of why the id had to be smuggled.
  It now also reads `[board.zephyr] west_board`.

It gained the rule — an `[image.*]` / `[image_defaults]` value equal to a
descriptor's framework id is refused, naming the id to write — with a real
population (the 13 rows above, measured red against one restored row) and a
selftest on the normal path, so it left `.config/gate-selftest-baseline.txt`.

### Not done here

The same smuggling exists for ecosystems with NO typed field for their own id:
`qemu-armv7a-nsh` (NuttX, one `[image.*]` row) and `esp32dev` (PlatformIO)
sit in `names`. With nothing in the descriptor to say which name is the
framework's, no rule can tell them apart mechanically; a `[board.nuttx]` /
`[board.esp32]` id field would be the prerequisite.
