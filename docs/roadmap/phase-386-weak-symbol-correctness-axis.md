# Phase 386 — a weak body is either CORRECT or merely LINKABLE, and nothing records which

**Status (2026-08-26). W1 + W2 LANDED, W3 measured and split, W4 open.** Two
real defects fixed; the phase's own opening claim ("17 symbols can fail
silently") was too broad and is corrected below — two could. W3's investigation
found a gate hole that outranks the labelling question it was opened for: a
COVERAGE row whose image is not built is skipped silently, so
`nros_board_register_netif` has been unverified while the gate reported green
(**W3b**, the next piece worth doing).

## The axis that exists

`scripts/weak-symbols-allowlist.txt` classifies every row as **`override-default`**
or **`optional-hook`**, which answers:

> Is a strong override GUARANTEED to exist?

That drives `check-weak-symbols-image.sh`, which `nm`s built images and verifies
each `override-default` symbol is actually overridden.

## The axis that does not

Grouping all 43 by *why* the weakness exists gives five causes, and they differ
on a question the allowlist never asks:

> If nobody overrides it, is the weak body CORRECT, or merely LINKABLE?

| # | group | example | weak body is |
| --- | --- | --- | --- |
| 1 | link-time optionality across crates | `weak_platform_log_stubs.c`, `nros_rmw_cffi_register`, `register_app_descriptors` | **correct** — a no-op log sink is a real configuration |
| 2 | layered default the lower layer ships | `nros_platform_panic` (4 platforms), `network_wait`, board `main` | **correct** — allowlist says so: "the weak body IS a valid runtime default" |
| 3 | out-of-tree consumer supplies the strong def | S32Z270 netif (ASI `ethif_shim.c`), `threadx_hooks`, PX4 uORB callbacks | **fail-loud by design** — returns -1 and prints |
| 4 | vendored library references symbols we alias | `zpico platform_aliases.c` (9 syms) | **linkable, wrong if reached** |
| 5 | freestanding libc gap | `nros-platform-threadx` (8 syms: open/close/read/write/lseek/pipe/stdin) | **linkable, wrong if reached** |

Groups 1-2 are safe: nothing degrades when nobody overrides. Group 3 is safe
*provided* it wins the archive-order tie — which is exactly 0769, still open.

**Groups 4 and 5 are the untracked risk.** A `read()` stub on ThreadX, or
`_z_send_serial_internal` on a board that does use serial, links cleanly and
then misbehaves at runtime with no diagnostic. 17 symbols sit in this state and
nothing distinguishes them from the 4 safe ones.

## Why the existing labels do not cover it

The two axes are independent:

* a group-2 symbol is `optional-hook` **and** correct;
* a group-5 libc stub is `optional-hook` **and** wrong if reached.

They carry the same label and the same gate treatment. Meanwhile all four
mislabels found so far (`_tx_initialize_low_level`, zpico's
`smoltcp_{init,cleanup}`, `network_glue.c`) erred in the *group-3* direction —
claiming a guarantee that was not there. Nobody has looked for group-4/5 errors,
because there is no field in which such an error could be expressed.

## Work items

**W1 — add the axis, do not replace the existing one.** A second column
recording `body: correct | fail-loud | linkable-only`. Additive, so
`check-weak-symbols-image.sh`'s `override-default` handling is untouched.

Recording the reason from 0769: the classification is **not documentation, it
drives a gate**. Relabelling `network_glue.c` to `optional-hook` and dropping its
`[img:]` set would silently stop verifying that LAN9118 MPS2 images override
`register_netif`/`poll_netif` — a change made in the name of accuracy that
narrows coverage. Any schema work here must be additive for the same reason.

**W2 — make `linkable-only` bodies fail loud where they can.** Group 5's libc
stubs and group 4's serial aliases currently fail silently. A stub that cannot do
its job should say so once, by the same argument RFC-0052 makes for S32Z270:
"a bundle-only image says what is missing instead of timing out silently".
Needs care per symbol — some are called on paths where printing is itself
unsafe, and `read()` returning -1 may be the correct freestanding answer rather
than an error.

**W3 — decouple "guaranteed" from "expected in these images".** 0769 found the
row shape conflates them: `network_glue.c` wants to be an `optional-hook` that
still carries an `[img:]` expectation. Whether the image gate accepts that
combination is untested; its header reads as though `[img:]` is only meaningful
for `override-default`.

**W4 — resolve 0769's tie.** Two weak defaults for `nros_board_register_netif`
coexist and archive order picks the winner; the fail-loud guarantee rides on
winning a tie it does not control. Strong-vs-weak cannot fix it — the contract is
that the out-of-tree consumer supplies the strong override, so promoting
S32Z270's would turn that into a duplicate-symbol error. Design call, needs the
board owner.

## What this phase does NOT claim

That any group-4/5 symbol is currently causing a failure. None is known to.
The claim is narrower: **17 symbols can fail silently and nothing records that
they can**, so a future breakage in them is undiagnosable by the audit that
exists to prevent exactly this.

Ordering note: W1 is cheap and unblocks the rest. W2 is the one with real value
and the most per-symbol judgement. W4 is independent and can proceed in parallel.


## W2 partly landed — and it corrected this phase's own classification

**ThreadX libc stubs (group 5, 6 of 8 symbols): done.** They now set `errno`
before returning -1.

This phase filed them as "linkable, wrong if reached". **That was wrong.**
Returning -1 IS the correct POSIX answer on a freestanding target — there is no
filesystem, so `open` genuinely cannot open. The real defect was narrower and
worse: they returned -1 **without setting `errno`**, so a caller doing the
standard `if (rc < 0) perror(...)` read a STALE errno and got a confident,
unrelated diagnosis. Silence would have been better than a wrong answer.

`ENOSYS` where the operation does not exist here at all (open, pipe); `EBADF`
where a descriptor was supplied that cannot be valid, since nothing on this
target can have produced one.

### The constraint that shapes the rest of W2

**This group cannot fail loud by PRINTING.** `write` is how printing reaches the
console, so a diagnostic inside it recurses — exactly issue 0589's Zephyr
hazard, where a Rust `println!` re-entered `zvfs_write` and exhausted the stack
with no message at all. `errno` is the only channel available.

Anyone applying W2 to group 4 (zpico's 9 serial aliases) must carry that
constraint: check what the symbol is on the path of before adding any output,
and prefer an out-of-band signal to a print.

### Correction to the phase's framing

The axis this phase proposes — CORRECT vs LINKABLE-ONLY — is still the right
one, but "linkable-only" turned out to be two different things:

* **no truthful answer exists** (a genuine stub that lies), and
* **a truthful answer exists and is not being given** (these, which returned the
  right value with the wrong explanation).

The second is the more common shape and the more dangerous one, because the
return value looks correct under test. W1's column should distinguish them.

Verified: `just check-weak-symbols` OK (17 files), and
`just threadx_riscv64 build-fixture-extras` completes with zero errors and no
diagnostics from `platform.c`.

Remaining in W2: group 4's serial aliases, and the two ThreadX symbols that are
not functions (`stdin`, and the `#if !defined(__linux__)` guard itself).


## W2 group 4 — zpico serial: the pair that lied, fixed

Reviewed all 9 zpico serial aliases against what zenoh-pico's own callers do
with the result. They are not uniform, and only two were wrong:

| symbol | returned | verdict |
| --- | --- | --- |
| `_z_open_serial_from_{pins,dev}` | `-1` | **truthful** — caller checks and fails the link |
| `_z_listen_serial_from_{pins,dev}` | `-1` | **truthful** |
| `_z_close_serial` | void | **fine** — closing what was never opened |
| **`_z_send_serial_internal`** | **`0`** | **LIED** |
| **`_z_read_serial_internal`** | **`0`** | **LIED** |

`0` is not an error in this API. zenoh-pico tests `ret == SIZE_MAX`
(`src/system/common/serial.c`, in both `_z_connect_serial` and the read loop),
so `0` reads as "succeeded, moved zero bytes" — a normal state. A send that
discarded every byte and a read that reported "nothing yet" forever both looked
healthy. Now `SIZE_MAX`.

**Not a live bug**, and worth saying so: the open/listen stubs return -1, so
these are unreachable in a default build. They become reachable under a PARTIAL
override — a board shim supplying `_z_open_serial_*` and forgetting send/read
links cleanly, opens the port, and loses all traffic silently. That is the exact
failure this pair should be incapable of.

### Printing was available here and was still not used

The ThreadX group could not print because `write` is the console path. These
are not, so a diagnostic could not recurse — the constraint recorded above does
not bind. It was still not added: the sentinel already reaches a caller with
real error handling (`_Z_ERR_TRANSPORT_RX_FAILED`), and an unconditional print
inside a transport poll loop is its own hazard.

**Returning the value the API already defines beats adding output.** Worth
generalising: "fail loud" means the caller can tell, not that something prints.

Verified: `check-weak-symbols` OK (17 files), `cargo build -p zpico-sys
--release` clean.

### W2 remaining

The two ThreadX non-function symbols (`stdin`, and the
`#if !defined(__linux__)` guard). Both are declarations rather than behaviour,
so neither has a return value to make honest — they may belong in W1's column
instead.


## W2 COMPLETE — and the remainder needed no change

The two symbols left were re-examined rather than assumed to need work.

**`nros_platform_panic` is not W2's business.** It is the eighth weak definition
in `nros-platform-threadx/src/platform.c`, and it is group 2 — a layered default
the platform ships, whose weak body IS a valid runtime state (RFC-0077,
phase-366). Nothing to make honest.

**`void *stdin = NULL` is already fail-loud, by the strongest mechanism
available.** Nothing in-tree reads it (checked: the only `stdin` matches under
`packages/` are `<stdint.h>` includes); it exists to satisfy libc references
that would otherwise fail the link. And `NULL` is the truthful value — a caller
testing `if (stdin == NULL)` learns there is no standard input, and a caller
dereferencing it **faults immediately**. An immediate fault is louder than any
diagnostic this file could produce, and unlike a printed message it cannot be
ignored or lost.

Changing it would make things worse: a non-NULL placeholder would turn a hard
fault into a silent misread, which is the defect this phase exists to remove.

### W2's result, and what it revised

Two of the seventeen `linkable-only` symbols were actually wrong:

| | was | now |
| --- | --- | --- |
| ThreadX libc stubs (6) | `-1`, stale `errno` | `-1` + `ENOSYS`/`EBADF` |
| zpico `_z_{send,read}_serial_internal` | `0` (a success value) | `SIZE_MAX` (the API's error sentinel) |

The other fifteen were already truthful, and the phase's opening claim — that
seventeen symbols "can fail silently" — was too broad. The corrected statement:
**two could, and the other fifteen were correctly reporting failure in a form
their callers understand.**

Three shapes emerged, and they want different treatment, which is the useful
output of W2 for W1's column:

1. **Wrong value** — the API defines an error sentinel and the stub returned a
   success one (zpico send/read). Fix: return the sentinel.
2. **Right value, no explanation** — POSIX -1 with a stale `errno` (ThreadX
   libc). Fix: set the explanation.
3. **Right value, self-enforcing** — `stdin = NULL`, where misuse faults.
   Nothing to fix.

Only shape 1 is a correctness bug. Shape 2 is a diagnosability bug. Shape 3 is
neither, and a column that cannot distinguish the three would send someone to
"fix" shape 3 and make it worse.


## W1 LANDED — the axis is a gated column, not a comment

`scripts/weak-symbols-allowlist.txt` rows now carry `body:<kind>` alongside the
existing classification, and `check-weak-symbols.sh` validates it.

| value | meaning |
| --- | --- |
| `body:correct` | a valid runtime state; nothing is missing when no override exists |
| `body:reports-failure` | cannot do the job and says so in a form the CALLER understands |
| `body:self-enforcing` | misuse faults immediately; **do not "fix" it** |

Current audit: 13 `correct`, 4 `reports-failure`, 0 `self-enforcing`
(`stdin` lives inside a `reports-failure` file rather than having its own row).

**There is deliberately no `body:silent-wrong`.** That state is the bug this
axis exists to surface, and W2 removed the two rows that had it. A row that
would need the value should be fixed, not labelled — the gate rejects it with
that sentence.

### Why validated rather than documented

An unchecked column decays: a new row omits it, and the axis quietly becomes a
comment on whichever rows happened to get one. That is the same
silent-coverage shape as a gate that cannot fail, which is what this whole phase
is about — so leaving the axis unenforced would have reproduced the defect at
the level of the fix.

Both directions verified against a real data row, after a first attempt tested
the wrong thing: the header comment now contains the literal `body:correct`, so
a naive `sed` mutated prose the loop correctly skips, and the gate "passed" a
mutation it had never seen. Controls now confirm a missing token reports
`MISSING`, an invalid one reports `INVALID` with the offending row, and the
restored file is green.

### Additive, as 0769 required

The new token sits in the comment, which both source-level gates strip before
parsing, and it does not touch `[img: …]`. `check-weak-symbols-image.sh`'s
override-default handling is unchanged, so nothing that was being verified
stopped being verified — the constraint 0769 established when it showed a
well-meant relabel would have silently narrowed coverage.


## W3 measured — the conflation is documentary, not mechanical

0769 assumed relabelling `network_glue.c` to `optional-hook` would drop its
`[img:]` set and silently stop verifying the two netif symbols. **Tested rather
than reasoned about, and that is not what happens.**

`check-weak-symbols-image.sh` parses `[img:]` with

```sh
grep -E '^[0-9]' "$allowlist" | sed -n 's/.*\[img:\([^]]*\)\].*/\1/p'
```

— any row starting with a digit. **It never inspects `override-default`.** So an
`optional-hook` row carrying `[img: …]` is already accepted, and the combination
W3 was opened to enable works today. Confirmed by relabelling the row and
re-running: `checked=18 fail=0 warn=0`, unchanged.

So W3 is mostly a DOC fix, not a schema change. The two axes are coupled only in
the header prose, which says an `[img:]` token belongs to "an override-default
line". That sentence is what made a safe edit look dangerous, and it is the thing
to correct.

### But the test exposed something worse

Relabelling produced **zero mentions of `nros_board_register_netif` or
`nros_board_poll_netif` in the gate output — before and after.** Coverage rows
for them exist (`check-weak-symbols-image.sh:47-48`, the FreeRTOS rust entries),
yet nothing is reported either way.

The images those rows name were not built in this tree, so the gate had nothing
to `nm` and said nothing. It reports `fail=0` and a per-symbol `note` only for
symbols with NO coverage row at all — a symbol whose row exists but whose image
is absent falls through both arms.

**That is a coverage hole in the gate that 0769's guarantee depends on**, and it
is more consequential than the labelling question W3 was opened for: the netif
override has been unverified for however long those FreeRTOS images have been
absent from routine builds, and the gate reported green throughout.

### W3 revised

* **W3a** — fix the header prose: `[img:]` is independent of the
  override-default/optional-hook classification, and the parser already treats
  it that way.
* **W3b (new, and the one that matters)** — a COVERAGE row whose image is
  missing must be reported, not skipped. Same class as the absorbing-STALE
  verdict of 0445 and the `required-features` targets nobody builds: a check
  that cannot run should say so rather than pass.


## Checkpoint (2026-08-26)

| wave | state | commit |
| --- | --- | --- |
| **W1** — gated `body:` axis | **landed** | `c61b80a05` |
| **W2** — make silent bodies fail loud | **complete** | `3e4a84305`, `04cc6df98` |
| **W3** — decouple guaranteed vs expected | **measured, split** | `6c9f73918` |
| W3a — header prose | open (small) | — |
| **W3b — report un-runnable coverage rows** | **open, highest value** | — |
| W4 — 0769's archive-order tie | open, needs board owner | — |

### What actually changed in the tree

* ThreadX libc stubs set `errno` (`ENOSYS` / `EBADF`) instead of returning -1
  with a stale value.
* zpico `_z_{send,read}_serial_internal` return `SIZE_MAX`, the sentinel the
  callers test, instead of `0` — which those callers read as SUCCESS.
* All 17 allowlist rows carry a validated `body:` axis; `check-weak-symbols.sh`
  fails on a missing or invalid value, verified in both directions.

### Two corrections this phase made to itself

1. **"17 symbols can fail silently" was wrong** — 2 could. The other 15 were
   already reporting failure in a form their callers understand. The original
   survey grouped by CAUSE and inferred risk from the grouping; reading each
   symbol against its callers showed the inference did not hold.
2. **0769's warning that a relabel would narrow coverage was wrong** — the image
   gate parses `[img:]` from any row starting with a digit and never reads the
   classification. I had declined to make that edit on the strength of the
   warning; testing it took two minutes and would have saved the caution.

Both were found by checking a claim instead of building on it, which is the
habit this phase should be judged on more than the two stub fixes.


## W3b LANDED — un-runnable coverage rows are now reported

`check-weak-symbols-image.sh` reported `warn=0`. It now reports **`warn=4`** on
this tree, and the four were never verified by any previous green run:

```
weak-image: WARN — coverage row(s) matched no image, so these symbols
  were NOT verified in this run (build the fixtures to cover them):
  examples/qemu-arm-freertos/rust (freertos_rs_*entry): nros_board_register_netif nros_board_poll_netif
  build/cargo-fixtures/qemu-arm-freertos (freertos_rs_*entry): nros_board_register_netif nros_board_poll_netif
  examples/qemu-arm-baremetal/rust (qemu-serial-talker): _z_open_serial_from_dev _z_close_serial …
  examples/qemu-arm-baremetal/rust (qemu-serial-listener): _z_open_serial_from_dev _z_close_serial …
```

**Wider than W3 predicted.** The investigation found the netif rows; the fix
shows the zpico **serial aliases** were equally unverified — the same symbols W2
had just corrected from `0` to `SIZE_MAX`, whose override was being checked by
nobody.

### The defect

```sh
[ -d "$base" ] || continue      # missing base: row vanishes
```

plus a `find` that matches nothing. The global `any_artifact` guard only fires
when EVERY row is empty, so a **partial** build — some images present, others
absent — printed `checked=N fail=0` while verifying nothing for the missing
rows. Green meant "nothing I could reach is broken", and read as "the guarantee
holds".

### WARN, not FAIL — deliberately

Not every lane builds every image. A red here would make the gate unrunnable
outside a full sweep, and a gate that cannot pass in normal use gets disabled —
which would cost more coverage than the hole it closed. The symbols are NAMED so
an empty row is legible as "this guarantee went unchecked", which is the thing
that was missing.

Verified both directions: removing the four uncovered rows drops `warn` to 0 and
restoring returns it to 4, with `checked=18 fail=0` unchanged throughout — so
the count tracks uncovered rows and nothing else.

### What this leaves

The warning is now visible, but **the guarantee is still unverified** until
those fixtures are built in a lane that runs this gate. W3b makes the gap
legible; closing it is a fixture-coverage question for whoever owns the FreeRTOS
and baremetal-serial lanes.


## W3a LANDED — the sentence that deterred a correct edit

The allowlist header said an `[img:]` token belongs to "an override-default
line" and that "optional-hook lines carry no token". Both read as rules. Neither
is one: the gate parses `[img:]` from any row starting with a digit and never
inspects the classification.

Replaced with what the parser actually does, plus the distinction the old
wording hid:

```
override-default / optional-hook   is a strong def GUARANTEED to exist?
[img: …]                           which symbols do we EXPECT specific images
                                   to override, and want checked?
```

A row can legitimately be `optional-hook` — no override guaranteed anywhere —
and still carry `[img:]` because the images we DO ship are expected to override
it. `network_glue.c` is exactly that.

The cost was real and is recorded in the header: the wording made a correct
relabel look like it would silently narrow coverage, so the relabel was declined
(by me, in 0769) and the row still carries a classification its own issue calls
wrong.

## W4 LANDED — the tie is dissolved, not won

Both weak `nros_board_register_netif` bodies return -1. Only S32Z270's printed,
so RFC-0052's fail-loud promise rode on winning an archive-order tie it does not
control: if `network_glue.c`'s default won, the image went quiet and timed out.

**Strong-vs-weak cannot break the tie** — the contract is that the out-of-tree
consumer supplies the strong override, so promoting either default turns that
override into a duplicate-symbol link error. 0769 identified this and left it as
a design call.

The fix is not to win the tie but to stop depending on it. The single call site
(`nros_freertos_register_netif`) returned the -1 bare; it now reports it:

```c
int rc = nros_board_register_netif(mac, ip, netmask, gw);
if (rc != 0) {
    printf("nros-board-freertos: no Ethernet — nros_board_register_netif "
           "returned %d. A board overlay or consumer must provide a strong "
           "override (see RFC-0052, issue 0769).\n", rc);
}
return rc;
```

Whichever default is linked, the operator is told. The board-specific message
stays in S32Z270's body and still prints when that one is linked — this is the
floor, not a replacement.

**Generalises the W2 lesson.** There the rule was "return the value the API
already defines, rather than adding output". Here no return value could carry
the information, because both bodies already returned the correct one — so the
diagnostic moved to the place that has exactly one implementation. **Put the
report where the ambiguity is not.**

Verified: `just freertos build-fixture-extras` rc=0 (243 s), no diagnostics from
`network_glue.c`; `check-weak-symbols` OK (17 files).

0769's remaining question — whether the two defaults should be one — is now a
tidiness issue rather than a correctness one.
