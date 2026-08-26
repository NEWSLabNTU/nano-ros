# Phase 386 — a weak body is either CORRECT or merely LINKABLE, and nothing records which

**Status (2026-08-26). Survey + plan, nothing landed.** Opened from a review of
all 17 audited files (~43 weak definitions) while working
[issue 0769](../issues/0769-weak-netif-signature-and-tie.md). The audit's
existing classification answers a different question from the one that predicts
damage. Implements nothing yet; issue 0050 owns the allowlist and both gates.

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
