---
rfc: 0111
title: "An image's entry framework is its own field, not a word in the board name"
status: Draft
since: 2026-10
last-reviewed: 2026-10
implements-tracked-by: []
amends: [RFC-0093]
supersedes: []
superseded-by: null
---

# RFC-0111 — An image's entry framework is its own field

**Status:** Draft (2026-10-27). Written before any code. It decides how an
image chooses RTIC (or any other entry framework) once the board word
`rtic-mps2-an385` stops carrying that choice. No phase yet.

**Narrowed by [RFC-0112](0112-framework-support-tiers.md) (2026-10-29).** The
`framework` field takes Tier 1 values only (frameworks with `nros::main!`
codegen, admitted under RFC-0112 D3). `zephyr` and `esp32` are platform entry
shapes that the board derives, so the field refuses them. Every other framework
integrates through RFC-0112's host-driven contract and never appears here. D1,
D2 and the open questions below are amended to match.

## Summary

`rtic-mps2-an385` is not a second spelling of the MPS2-AN385 bare-metal
board. It is the same board with a different entry framework chosen, and the
choice rides in the board word. RFC-0093 R2 says a board name is
`<where>-<stack>`, and a framework is neither.

This RFC moves the choice into its own field on the image:

```toml
[image.mps2-an385-baremetal]
board = "mps2-an385-baremetal"
framework = "rtic"
```

The board descriptor lists the frameworks it can boot. Every consumer that
asks "which framework" reads the image field through one resolver, never the
board key. The old word becomes a refusal that names both replacements. This
is the same retirement route `qemu-mps2-an385` took in issue 2063.

## What the word does today

One string, `rtic-mps2-an385`, answers four questions. Each consumer derives
its answer from the string on its own.

| Question | Where it is answered | How |
|---|---|---|
| Which board crate and ZST? | `nros_orchestration_ir::BOARD_PATHS` | a row `rtic-mps2-an385 → ::nros_board_mps2_an385::RticMps2An385` |
| Which entry shape does `nros::main!` emit? | `framework_for_board_key` (orchestration-ir), read by `try_framework_for` in `nros-macros` | `match key { "rtic-mps2-an385" => "rtic", … }` |
| Which board-crate feature turns that shape on? | `entry_surface_features` (`nros-cli-core/src/builder/entry.rs`) | the same key lookup, so `rtic`, else `board-entry` |
| Which callback dispatch is legal? | `nros ws check`'s framework × dispatch lint (`check_workspace.rs`) | the board crate's `[package.metadata.nros.board] framework`, which this crate deliberately does NOT carry |

What that costs, all measured on main on 2026-10-27:

- **The search order is load-bearing.** `deploy_candidates` searches the board
  word before the image id, because `[image.mps2-an385-baremetal] board =
  "rtic-mps2-an385"` names the plain board in its own id. Once
  `mps2-an385-baremetal` became a `BOARD_PATHS` key, a different order
  silently built `board-entry` instead of RTIC (phase-517 W2).
- **The dispatch lint is off for RTIC.** The board crate's `Cargo.toml` says
  so: declaring `framework = "rtic"` on the crate would make the lint reject
  `dispatch = "inline"` on every direct-exec image of the same board. So the
  rule "RTIC requires Deferred dispatch" is enforced only for a board crate
  that has one framework, and this board has two.
- **The framework count multiplies board words.** With a second framework
  (the vocabulary already lists `embassy`) the board needs a third word.
  Phase-337 W7.a removed `rtic-stm32f4` and `embassy-stm32f4` together with
  their board, which is the shape this would bring back.
- **RFC-0093 §3 exempted the word**, as a deploy value rather than a board
  key. Phase-517 W3 made one board vocabulary from image to setup, so the
  exemption no longer separates two vocabularies. It is the last
  non-conforming alias on this descriptor (issue 2063).

## Decisions

### D1 — `framework` is a field of the image

`[image.<id>] framework = "<name>"`, with a value from the Tier 1 list
(RFC-0112 D5): `owned-spin`, `rtic`, and `embassy` until issue 2134 is ruled.
`zephyr` and `esp32` are refused with a message saying the board decides them.
The field is optional; D2 says what an absent field means.

It sits on the image, not on a component or the system:

- A framework owns the image's reset vector and boot. Two components on one
  image cannot disagree about it.
- Two images of one system can disagree. A Linux bridge and an RTIC
  controller in one bringup is legitimate.

It is a nano-ros BUILD fact. Per the RFC-0060 amendment (2026-10-10), it never
enters the SystemModel. `nros sync` carries it in the `nros.toml` overlay
beside the model, as it carries the other image facts.

It is Rust-only today: every framework other than `owned-spin` is a Rust entry
shape. A `framework` on an image whose entry is C or C++ is refused, naming
the language. Nothing is silently ignored.

### D2 — The board descriptor lists the frameworks it can boot

```toml
[[board]]
names = ["mps2-an385-baremetal", "baremetal", "bare-metal"]
frameworks = ["owned-spin", "rtic"]   # first = default
```

- **Absent field:** the image gets the first entry, so every existing
  single-framework board keeps its behaviour and its images need no edit.
- **Value outside the list:** a refusal that names the list.
- **Descriptor with no `frameworks` key:** it means `["owned-spin"]`.
  `framework_for_board_key`'s `zephyr` and `esp32-*` rows are not frameworks
  (RFC-0112 D5). They move into their descriptors as a separate
  `entry_shape`, which the macro reads from the board and the image field
  cannot set. They move in the same change, so the table and the descriptors
  never both answer.
- **`frameworks` lists Tier 1 names only.** A Tier 0 recipe (tokio, GLib,
  Arduino and the like) is user code and never appears in a descriptor.

An out-of-tree board keeps its route. `[package.metadata.nros.board]
framework` on its crate is read as a one-entry `frameworks` list, through
`NROS_BOARD_FRAMEWORK` as today (issue 0415). An unknown name is still an
error, never a fall-through.

### D3 — One resolver answers "which framework", and every consumer asks it

`nros_orchestration_ir::image_framework(image, descriptor) -> Result<&str,
Refusal>` replaces `framework_for_board_key`. The four consumers in the table
above all call it:

- **The macro** gets the value from `leaf_system`, which already reads the
  image table for `board`, `locator` and `os_config`.
- **`entry_surface_features`** keys the board-crate feature on the resolved
  framework. That makes the `deploy_candidates` order irrelevant to framework
  choice: it only finds the board.
- **The dispatch lint** reads the IMAGE's framework instead of the crate's.
  That turns the RTIC-requires-Deferred rule back on for this board without
  touching any direct-exec image.
- **`BOARD_PATHS`** keeps one row per board. The ZST an image needs comes from
  the (board, framework) pair, and the descriptor names it per framework:
  `entry_types = { owned-spin = "Mps2An385", rtic = "RticMps2An385" }`.

The narrower rule the board crate used to need ("a crate-level framework
cannot fit a two-framework board") goes away with the field. The crate keeps
its `rtic` and `board-entry` features, selected by D3.

### D4 — `rtic-mps2-an385` retires as a refusal, with two replacements

The word becomes a row in `nros_entry_lower::RETIRED_BOARD_KEYS`. Its refusal
text names both halves:

```
board `rtic-mps2-an385` is retired: write
  board = "mps2-an385-baremetal"
  framework = "rtic"
```

The table's replacement column is one string today. It gains an optional
`framework` replacement, so the message is generated, never hand-written per
row. No silent alias maps the old word: that is phase-500 W5's rule, and
issue 2063 followed it.

After this, `check-board-name-reach` and the retired-board ledger carry the
row the same way they carry `qemu-mps2-an385`. RFC-0093 §3's exemption list
loses its last framework word.

### D5 — The image id is a label, and stays one

Every RTIC example today is `[image.mps2-an385-baremetal]` with `board =
"rtic-mps2-an385"`. After migration the id and the board are the same string
and the framework is a third line. Nothing reads the id as a board any more
for framework purposes (D3), so issue 0798's class of bug (an id that happens
to be a board key winning the search) cannot pick the framework, only the
board, and the board it picks is now the right one.

## Alternatives considered

- **Keep the word, document the exemption.** No code, and RFC-0093 R2 keeps a
  standing exception. The dispatch lint stays off for RTIC, and the next
  framework adds another board word. Rejected: the cost recurs every time a
  framework is added.
- **Make RTIC a stack: `mps2-an385-rtic`.** Conforms to R2's shape with a
  rename only. But RTIC runs ON bare metal; it is not a peer of FreeRTOS or
  Zephyr. And it still makes one descriptor answer to one word per framework,
  so the lint and multiplication problems stay. Rejected.
- **Put `framework` on the board crate's Cargo features only** (`features =
  ["rtic"]` in the leaf's manifest). That was the state before issue 1305,
  hand-written per leaf and derived nowhere. Rejected.
- **Put `framework` on `[system]`.** One field for every image, which is
  wrong for a mixed bringup (D1). Rejected.

## Migration and work

The work, for a phase to pick up:

1. **Schema.** Add `framework` to the image table in both readers:
   `leaf_system` and `nros-cli-core`'s `deny_unknown_fields` `DeployBlock`.
   Add `frameworks` and `entry_types` to the board descriptor schema.
2. **Resolver.** `image_framework` (D3). Move `framework_for_board_key`'s rows
   into descriptors, then delete it. Rewire the macro, `entry_surface_features`
   and the dispatch lint.
3. **Mps2 descriptor.** `frameworks = ["owned-spin", "rtic"]`, plus
   `entry_types`. Drop `rtic-mps2-an385` from `names`.
4. **Migrate authored sites.** About 50 tracked files outside `archived/` on
   2026-10-27 (`git grep -l rtic-mps2-an385`):
   - the 8 `examples/mps2-an385-baremetal/rust/*-rtic*` leaves (`system.toml`
     and doc comments);
   - `bins/rtic-run-plan-e2e`;
   - the book's `rtic-integration.md`, `bare-metal.md`,
     `board-support-tiers.md` and `troubleshooting-first-10-min.md`;
   - the CLI, macro and tier-resolver tests;
   - `board-support.toml`;
   - the `NanoRosBareMetalPlatform.cmake` comment.
5. **Retire the word** (D4). Add the refusal row with generated text, a ledger
   row, `retired-names.md`, a changelog `.breaking.md`, and this RFC's
   amendment note in RFC-0093 §3.
6. **Tests.** Each step is mutation-checked:
   - a test per refusal (the old word, a framework the board does not list, a
     framework on a C image);
   - the dispatch lint firing for `framework = "rtic"` with
     `dispatch = "inline"`, now on this board;
   - `nros ws check` accepting a direct-exec image of the same board.
7. **Proof.** All 33 bare-metal rows build (`just build qemu`), `just test
   qemu` passes, and the 4 RTIC run-plan tests and the 8 RTIC examples boot and
   deliver.

## Open questions

- **Default for a multi-framework board.** With `zephyr` and `esp32` out of
  the vocabulary (RFC-0112 D5), this concerns exactly one board today,
  `mps2-an385-baremetal`. D2 makes the first list entry the default, so a plain `board = "mps2-an385-baremetal"` stays direct-exec, as it
  is today. The alternative is to refuse an absent `framework` on any board
  that lists more than one. That is more explicit, and it breaks every
  existing direct-exec mps2 image until each says `owned-spin`. This RFC picks
  the default. The maintainer may rule otherwise.
- **`owned-spin` as a user-facing word.** It is the internal name. Users may
  read `framework = "none"` more naturally. This RFC keeps one spelling per
  value (`FRAMEWORKS`) and does not add an alias.
