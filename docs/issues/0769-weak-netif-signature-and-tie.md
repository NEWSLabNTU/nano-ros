---
id: 769
title: "Two WEAK definitions of `nros_board_register_netif` in crates that
  compose — different signatures, and archive order decides which fail-loud
  message an S32Z270 image gets"
status: open
type: bug
area: boards, build
related: [issue-0050, rfc-0052, phase-372, phase-375]
---

## Problem

One symbol, two weak definitions, in crates that compose:

```c
// packages/boards/nros-board-freertos/c/network_glue.c:51
//   — and its own caller at :117 passes four pointers
__attribute__((weak)) int nros_board_register_netif(
    const uint8_t mac[6], const uint8_t ip[4],
    const uint8_t netmask[4], const uint8_t gw[4]);

// packages/boards/nros-board-s32z270-freertos/c/board_s32z270.c:105 (BEFORE)
__attribute__((weak)) int nros_board_register_netif(void);
```

`nros-board-s32z270-freertos` **depends on** `nros-board-freertos`
(`Cargo.toml:15`), whose `build.rs` compiles `network_glue.c` into
`libfreertos_glue.a`. So an S32Z270 image contains BOTH definitions and the
linker keeps one **by archive order** — issue 0050's stated failure mode
verbatim: "a weak symbol can be silently dropped or the wrong copy chosen with
no error", and the failure is a runtime misbehaviour rather than a link error.

The compiler cannot see it: the two definitions are in separate translation
units that meet only at link time. Put both prototypes in one TU and it is a
hard error —

```
error: conflicting types for 'nros_board_register_netif'; have 'int(void)'
```

— which is how this was confirmed rather than argued.

### What is actually at stake

The allowlist promises S32Z270's default is *"deliberately fail-LOUD
(register_netif returns -1 and prints why) so a bundle-only image says what is
missing instead of timing out silently"* — RFC-0052. Both weak bodies return
-1, so the RETURN is the same either way; only S32Z270's **prints**. If
`network_glue.c`'s weak wins the tie, the image goes quiet and times out, which
is exactly the outcome the fail-loud rule exists to prevent.

The ABI half was the milder half: under AAPCS, calling an `int(void)` with four
register arguments is harmless — the callee ignores them. This is a correctness
and legibility defect, not a live crash.

## Fixed (the signature half) 2026-08-23

S32Z270's weak now takes the same four parameters and ignores them, so the two
definitions are one function. Verified with a positive and a negative control
(both prototypes in one TU compile; the old `(void)` spelling errors).

## NOT fixed — the tie

**Two weak defaults still coexist and archive order still picks the winner.**
Aligning the signature removed the ABI mismatch, not the ambiguity. The
fail-loud guarantee still rides on winning a tie it does not control.

Strong-vs-weak cannot resolve it: the contract is that the CONSUMER (ASI's
`ethif_shim.c`, out of tree) supplies the strong override, so making S32Z270's
definition strong would turn the consumer's override into a duplicate-symbol
link error.

The shapes worth weighing:

* **One default per symbol per image.** S32Z270 is the more specific board and
  carries the better message, so `network_glue.c`'s generic weak should not be
  linked into an S32Z270 image — selection at link time (cmake picks a real
  implementation OR an explicit stub TU) instead of relying on weak resolution.
  This is the shape that eliminates the weak pair rather than aligning it.
* **Drop S32Z270's default** and put the board-specific text in
  `network_glue.c`'s message. Cheapest; loses per-board diagnostics.

## Bearing on the wider weak-symbol question (phase-375 W2)

Found while auditing the `override-default` class for elimination, and it
revises that audit twice:

* The set is **4 files / 11 symbols**, not 5 files. `board_s32z270.c` is
  classified `optional-hook`; the phrase "override-default" appears in its
  comment only because it CONTRASTS itself with `network_glue.c`.
* **`network_glue.c`'s own `override-default` label looks wrong.** Its weak
  returns -1 meaning "no board override → no Ethernet", and `poll_netif` is a
  no-op "for boards that use IRQ-driven RX". Both are supported shipped states,
  which is `optional-hook` — not a placeholder awaiting a guaranteed strong def.
  The image gate passes because the images that are tested (LAN9118 MPS2) do
  override, not because an override is guaranteed.

The audit has now mislabelled three entries — `_tx_initialize_low_level` and
zpico's `smoltcp_{init,cleanup}` were caught earlier by the image gate, this one
by reading. All three erred in the same direction: claiming a strong override
exists where none is guaranteed. Re-audit the remaining classifications before
removing anything on the strength of the label.
