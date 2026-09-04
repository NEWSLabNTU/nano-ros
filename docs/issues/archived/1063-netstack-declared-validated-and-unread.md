---
id: 1063
title: "`netstack` is declared by users, validated against the board, emitted as `NROS_NETSTACK` — and read by nothing"
status: resolved
type: bug
area: build, config
severity: medium
found: 2026-09-04
related: [0941, 0940, 0949, 0842, phase-349, phase-351, phase-206]
---

# A user-facing knob whose value reaches no build

## RESOLVED 2026-09-05 — option B, and the deciding evidence was not the count

The emission is gone; the validation stays.

The reason I first gave was that the choice is degenerate — no board declares
more than one netstack, so the emitted value is always `supported_netstacks[0]`.
True, but weak: it invites "then add a consumer when a board declares two."

**The deciding fact is that the only plausible consumer already knows.** A
netstack is selected by a per-board cmake file —
`cmake/board/nano-ros-board-riscv64-qemu.cmake:240` calls
`nros_threadx_build_netstack_netxduo(...)` unconditionally — and a file that is
already per-board does not need an environment variable to tell it which stack
it uses. Even a board with two stacks would branch inside its own file.

**The validation half is live and was kept**, which the original filing got
right for the wrong reason. `NanoRosBoardFacts.cmake:126-129` matches on the
error text and turns it into a configure `FATAL_ERROR`, so `resolve_netstack`
is load-bearing even though its Ok value is now discarded.

That discard created a new hazard: nothing would stop a later edit deleting the
call. `an_unsupported_netstack_fails_the_resolution` is what refuses it, and its
doc comment now says so. Verified by negative control — with the call removed
that test FAILS while every other test in the module still passes, which is
exactly the trap it exists to close.

The four tests that asserted the emission now assert its ABSENCE, so re-adding
`NROS_NETSTACK` without a reader fails.

**Not done, deliberately:** option C (refusing the user-facing `netstack` key).
Twelve site-config blocks use it, it is harmless once validated, and it is
phase-351's decision rather than this issue's.

**On issue 0941:** it used a missing `NROS_NETSTACK` as one symptom that board
facts were undelivered. `NROS_BOARD` and `NROS_BOARD_TOML` carry the identical
signal and remain, so 0941 loses nothing it cannot get from those two.

## The chain, and where it stops

1. A user declares `netstack` in a site-config `[deploy.<target>.nros]` block.
   **Twelve such blocks across seven bringups in six workspaces**, and issue
   0941 records that *"every one of them declaring a netstack"*.
2. `BoardDescriptor::resolve_netstack`
   (`packages/cli/nros-cli-core/src/orchestration/board_descriptor.rs`)
   validates the request against the board's `supported_netstacks` and returns
   a typed `NetstackError` naming what IS available.
3. `nros board-facts` emits the result as `NROS_NETSTACK`
   (`packages/cli/nros-cli-core/src/cmd/board_facts.rs:245`).
4. **Nothing reads it.** Tree-wide, the only non-test occurrences are the
   writer and a COMMENT in `cmake/NanoRosWorkspace.cmake:258`. Four tests in
   `board_facts.rs` assert the emission; none asserts an effect.

So a user states a preference, the tool validates it and reports success, and
the value changes nothing about the image.

## Why this is not simply "delete the dead variable"

**The validation half is real and worth keeping.** `resolve_netstack` is the
seam that knows both the board's domain and the user's request, and phase-351 W4
built it deliberately so an unsupported pair fails there rather than as a link
error inside a stack nobody selected. Deleting the emission must not delete that.

**But the choice is degenerate today.** Measured across
`packages/boards/*/nros-board.toml` on 2026-09-04:

| `supported_netstacks` | boards |
| --- | ---: |
| `[]` | 5 |
| `["lwip"]` | 3 |
| `["smoltcp"]` | 2 |
| `["netxduo"]` | 2 |

**No board declares more than one.** So `resolve_netstack` can never *choose*:
it either passes through the single declared value or rejects a request that
disagrees with it. The emitted value is always exactly `supported_netstacks[0]`
— information the descriptor already carries and the board crate already
encodes in its features.

**And it interacts with an open issue.** 0941 (`nros_resolve_board_facts` fails
SOFT) treats a missing `NROS_NETSTACK` as one of the observable symptoms that
board facts were never delivered. Removing the emission without settling 0941
would remove a signal that issue currently relies on.

## The class

This is the shape phase-349 named while flagging this very variable:

> `NROS_NETSTACK` is emitted too … and **nothing reads it** — the same
> declared-but-unread shape, now with a writer and no consumer rather than a
> reader and no writer.

Its sibling is `BoardTransportConfig::set_interfaces`, which phase-206 W5
deletes. Two live instances of one class; this is the one that is *harder*,
because unlike `set_interfaces` it has real users who believe it works.

## Options, none of them free

**A. Give it a consumer.** Something in the build selects or asserts the stack
from `NROS_NETSTACK`. Honest only if there is a decision to make — and with
≤1 netstack per board there is not, until a board declares two.

**B. Delete the emission, keep the validation.** Smallest change, and correct
under today's data. Costs: four tests to update, and 0941 loses a symptom.
Should carry a note that a board declaring two netstacks re-opens the question —
and that the emission then returns **with a consumer in the same commit**, not
before.

**C. Refuse the knob.** If the user's `netstack` can only ever restate what the
board already declares, the most ROS-2-like answer is that the user does not
name a netstack at all — the board does, the way the OS owns the device. This
is the largest change (it removes a user-facing key) and belongs to phase-351,
whose W4 built the validation.

**Recommendation: B now, with the note, and C as the question for phase-351.**
A is only worth building the day a board has two stacks.

## Not covered

* Whether any out-of-tree board build consumes `NROS_NETSTACK`. The variable is
  part of `nros board-facts` output, which is a CLI surface; nothing in-tree
  documents it as a contract, but absence of an in-tree reader is not proof of
  absence of an out-of-tree one.
* Whether the twelve site-config blocks 0941 found would behave differently
  under any of the three options — they are currently unreachable for a
  different reason, which 0941 owns.
