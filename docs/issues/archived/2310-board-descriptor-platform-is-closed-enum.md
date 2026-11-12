---
id: 2310
title: "A board descriptor's `platform` is a closed enum, so no board can name an out-of-tree platform"
status: resolved
type: bug
area: cli, boards, platform
related: [2292, 2291, phase-545]
found: 2026-11-07
---

**Home: [phase-545](../../roadmap/phase-545-rmw-provider-scan-build-road.md).** Found while fixing issue 2292 (phase-537 W12 follow-up).

## Symptom

A porter follows the Extending road for a new platform:

```
nros new platform acmeos
nros new board acme --for-platform acmeos
```

Both board spellings (`nros new board --for-platform` and `nros board new`)
scaffold the package. Every catalog reader then refuses it:

```
unknown variant `acmeos`, expected one of `posix`, `freertos`, `bare-metal`,
`nuttx`, `zephyr`, `threadx-linux`, `threadx-riscv64`, `stm32`
```

## Code site

`packages/cli/nros-cli-core/src/orchestration/board_descriptor.rs`: the
descriptor's `platform` field deserializes into the closed `PlatformKind`
enum. A platform the user's workspace provides therefore needs a core edit
before any board can name it.

## Why it matters

RFC-0049 promises a porter zero central-file edits, and RFC-0064 R5 D1 says
`--out-of-tree` is the whole difference between nano-ros's boards and a
user's. Both are false for a new platform while the set is closed. The book's
Add a platform and Add a board pages lead a porter straight into this refusal.

## Options

1. **Open the set.** `platform` becomes a name resolved against the platform
   catalog (`nros-platform.toml` `names`, which issue 2293 makes the scaffold
   write). Every `PlatformKind` match (feature selection, family, driver
   road) then reads the platform package's own facts, or treats an unknown
   platform as "no built-in behaviour".
2. **A catch-all variant.** `PlatformKind::External(String)` keeps the
   built-in matches and routes an unknown name to the platform package's
   descriptor. This is smaller, but every match needs an `External` arm.

Either way it is a design decision: opening the set touches every
`PlatformKind` match. It is not a scaffold fix.

## Decision

**Question:** when a board names a platform that is not built in, where does
the CLI get the facts a `PlatformKind` supplies today?

### Measured (origin/main, 2026-11-12)

- `PlatformKind` has 8 variants and is referenced 181 times in 15 files
  under `packages/cli` (`git grep -c PlatformKind`). The most are in
  `board_descriptor.rs` (63), `model_ingest.rs` (26) and `cargo-nano-ros`
  `scaffold.rs` (21; a second, separate enum of the same name).
- The enum answers 7 facts per platform: `platform_feature`, `link_kind`,
  `cmake_feature_set`, `cmake_deploy`, `board_family`, and two derived from
  `board_family` (`honors_entry_sizing`, `sched_task_rtos_key`). Some call
  sites also match on it directly, for example `run_plan.rs` (QEMU board,
  west runner) and `build_next.rs` (book page per platform).
- Only `link_kind` and `board_family` select a code path in the CLI
  (NuttX staging link, sched-task RTOS key). The other 5 facts are tokens
  passed on to cargo or cmake.
- `Stm32` is already an out-of-tree platform written into the enum by hand.
  No in-tree board uses it, and three methods map it to the bare-metal
  answer. That is what option 3 below would do through data instead.
- The platform loader already has a family chain:
  `PlatformConfigFile::inherits`
  (`packages/tooling/nros-platform-config/src/platform_config.rs`). The
  `nros new platform` scaffold writes it commented out
  (`# inherits = "bare-metal"`).
- `board_descriptor.rs` says explicitly why `platform_feature` was NOT moved
  into `nros-platform.toml`. There is no `config/stm32/`, and both ThreadX
  kinds share one feature, so the mapping is keyed on the enum, not on a
  directory.
- Not measured: whether a porter's own `nros-platform-<name>` crate is then
  linked once the board resolves. The "Done when" test answers that.

### Options

1. **Open the set: the facts move into `nros-platform.toml`.** Each platform
   states its feature, link kind, cmake tokens and RTOS family, and
   `PlatformKind` becomes a struct loaded from the catalog.
   - *UX:* best for a porter whose platform matches no existing family.
   - *Maintainability:* rewrites 181 references. The 7 facts become runtime
     data, so adding a platform no longer fails to compile at each site that
     must choose. The CLI's two real branches (`link_kind`, `board_family`)
     still need a closed vocabulary underneath.
   - *Design:* closest to RFC-0049 ("a platform's facts live in its
     package"). It reverses the documented reason for keeping
     `platform_feature` on the enum.
2. **Catch-all variant: `PlatformKind::External(String)`.**
   - *UX:* same as option 3 for the porter.
   - *Maintainability:* every one of the 7 methods and every direct match
     needs an `External` arm. Each arm has two choices. It can guess a
     default, which silently gives an RTOS port bare-metal behaviour. Or it
     can read the platform's descriptor, which makes this option 1 done
     halfway, with two spellings of every fact.
   - *Design:* weakest. "Unknown" becomes a variant that every caller has to
     interpret.
3. **Resolve through `inherits` to a built-in family (recommended).** A
   board's `platform` becomes a name looked up against the catalog's
   `names`.
   - A built-in name resolves as today.
   - Any other name walks that platform's own `inherits` chain until it
     reaches a built-in kind, and that kind supplies all 7 facts.
   - If the chain reaches no built-in, the CLI refuses and names the fix:
     `inherits = "<one of the built-ins>"` in the porter's
     `nros-platform.toml`.

   The descriptor keeps the porter's name for display and for `config
   explain`.
   - *UX:* a porter writes one line the scaffold already suggests. No
     central edit is needed.
   - *Maintainability:* the enum stays closed and every match stays
     exhaustive. The cost is a two-step descriptor load (it needs the
     catalog) and two fields: the name and the resolved kind. `Stm32` can
     then leave the enum as a data row.
   - *Design:* reuses the existing `inherits` ladder. The limit is honest: a
     platform that differs from every family in `link_kind` or
     `board_family` still needs a core change, because those two facts
     select CLI code paths.
4. **Stay closed and document it.** The book's Add a platform page says a new
   platform needs a nano-ros pull request that adds a variant.
   - *UX:* worst. A porter is blocked on upstream.
   - *Maintainability:* zero change.
   - *Design:* contradicts the RFC-0049 promise of zero central edits and
     RFC-0064 R5 D1, so those RFCs would need amending.

### Recommendation

**Option 3.**
- It closes the porter's road with one authored line and no core edit.
- It keeps the compile-time exhaustiveness that caught `Stm32`'s three
  explicit arms.
- It reuses a field the loader and the scaffold already have.
- Option 1 can follow later if a real porter needs a family that does not
  exist. Option 3 does not block it.

## Done when

`nros new platform acmeos && nros new board acme --for-platform acmeos`
followed by `nros board info acme` resolves the board, with no edit outside
the user's workspace. A test covers that road.

## Ruling (2026-11-12)

The user ruled **option 3**, relayed by the manager session: "resolve an
out-of-tree platform via `inherits` to a built-in family; refuse with a fix
hint when the chain hits none."

## Fix (2026-11-12)

**Resolution.** `PlatformKind::resolve_family` resolves the name a board
writes. A built-in name is its own family. Any other name is looked up on the
platform search path. The walk then follows that descriptor's `inherits`,
exactly as written, until it reaches a built-in kind. It walks the authored
strings, not the directories they resolve to, because `nros-platform-threadx`
answers to both `threadx-linux` and `threadx-riscv64`.

**Refusals.** Each refusal names the fix:

- **No descriptor answers the name.** The message names
  `nros new platform <name> --inherits <family>`.
- **The chain ends without reaching a built-in.** The message names
  `inherits = "<family>"`, the descriptor to put it in, and the families.
- **The chain names a missing platform, or loops.** The message names the
  chain.

**Loader.** `BoardFile::parse_resolving` rewrites such a `[[board]]` before the
typed parse:

- `platform` becomes the resolved family.
- The authored name moves to the new derived field
  `BoardDescriptor::platform_name`, which a descriptor may not state.

Every `PlatformKind` match downstream is therefore unchanged and still
exhaustive. A file that names only built-in platforms takes the old parse
untouched, so its error spans survive. A stated `platform_feature` that
disagrees with the resolved family is refused.

**Search path.** `PlatformFamilies` holds it, earliest root first:

1. `$NROS_PLATFORMS_DIR`
2. the catalog's extra board roots (where `nros new platform` and `nros new
   board` write side by side)
3. the parents of package dirs that are platform packages
4. the checkout's `packages/platform` and `config`

It is loaded lazily, so a catalog of built-in boards never reads a platform
descriptor. `PlatformsTree::inherits_of` is the one new reader in
`nros-platform-config`.

**Scaffolds.**

- `nros new platform <name> --inherits <family>` writes a live `inherits`
  line. Without the flag the line stays commented out.
- The board scaffold no longer writes `platform_feature = "platform-<platform>"`.
  The field is derived from the family, and the stated line named no real
  feature for `threadx-linux` (whose family selects `platform-threadx`) or for
  any out-of-tree platform.

**Display.** `nros board info` now prints `platform` (as written) and
`platform_family`.

**Tests.**

- `platform_family_tests` (`board_descriptor.rs`) covers:
  - a built-in name;
  - a two-hop chain to `threadx-linux`;
  - each refusal;
  - a catalog over a porter's side-by-side root, which resolves to
    `bare-metal`;
  - the same catalog refused without `inherits`;
  - a stated `platform_name` refused.
- `board_new_scaffold::an_out_of_tree_platform_and_its_board_resolve` is this
  issue's "Done when", end to end:
  - `nros new platform acmeos --inherits bare-metal`, then
    `nros new board acme --for-platform acmeos`, then `nros board info acme`
    resolves, with `platform_family` `bare-metal`;
  - without `--inherits`, the refusal names `inherits = "<family>"`.

  Mutation-checked: with the loader routed back to the plain parse, the test
  fails.

**Not done here.** Whether a porter's own `nros-platform-<name>` crate is
linked into an image is the build's question, not the catalog's. This issue
makes the board resolve, which is what its "Done when" asks.
