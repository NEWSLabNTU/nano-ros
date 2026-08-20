---
id: 710
title: "A board's boot path need not be a `pub fn run*`, so issue 0708's rule and its gate miss the ones that are not"
status: open
type: bug
severity: medium
area: platform, testing
related: [issue-0708, issue-0589, issue-0420]
---

## What 0708 established, and where it stops

Issue 0708 found that a board which never calls `nros_log::init` drops every
library-emitted record silently, fixed it at every board boot funnel, and gated
it: `check-board-log-sink.py` requires each `pub fn run*` in a board crate to
reach `init_default`.

That rule names a SPELLING, not the thing it cares about. A boot path that is not
spelled `pub fn run*` is invisible to both the fix and the gate, and two exist:

| image | how it actually enters |
| --- | --- |
| `logging-smoke-nuttx-qemu-arm` | `nros-board-nuttx-qemu`'s `pub extern "C" fn nsh_main` — NuttX's init task, which calls `nsh_initialize()` then the image's `main` |
| `logging-smoke-mps2-baremetal` | `#[entry] fn main()` from `cortex_m_rt`, in the FIXTURE — no board code runs at all |

## How it was found

Not by reading — 0708's gate was green throughout. Issue 0708's follow-up
converted the `logging-smoke-*` fixtures to rely on their board rather than
publish their own sink list, which turns each into an assertion ABOUT the board.
Two then failed on a booted image:

```
logging_smoke_qemu_baremetal_mps2   Expected output to contain
                                    '[TRACE] smoke: trace payload' — output empty
logging_smoke_nuttx_qemu_arm        QEMU timed out waiting for log output
```

Both were reverted rather than shipped, because a fixture relying on a board that
does not publish is the silent regression 0708 exists to prevent.

## The two cases are NOT the same defect

Worth separating, because the obvious remedy is wrong for one of them:

* **NuttX — a board funnel the rule could not see.** `nsh_main` IS the board's
  boot path; it simply is not spelled `run*`. The board owns this path and must
  publish on it. Fixed here.
* **mps2-baremetal — the image bypasses the board.** `#[entry] fn main()` in the
  fixture means no board code runs between reset and the image's own `main`.
  There is no board funnel to fix. An image that takes its own entry owns its own
  logging setup, and that fixture's `init(sinks::default())` is CORRECT — which
  is why the conversion was reverted there rather than "fixed".

So the rule is not "every board publishes for every image". It is: **a board
publishes at every boot path it OWNS**, and an image that bypasses the board owns
its own. `check-board-log-sink` currently states neither half.

## Residue — the gate still names a spelling

Extending the gate to `pub extern "C" fn` entry points is not simply a wider
regex: a board crate exports many `extern "C"` symbols that are not boot paths
(`nros_platform_log_write`, thread trampolines, FFI shims), and flagging those
would make the gate noisy enough to be deleted — the failure mode recorded on
`check-lane-skip-protocol` in issue 0695.

What distinguishes a boot funnel is that control passes through it ONCE on the
way to the application, which no regex sees. Options, none free:

* an explicit registry — a board declares its boot paths, and the gate checks the
  declared set (honest, but a declaration can go stale silently);
* an attribute/marker macro on boot funnels, so the gate has something exact to
  match;
* keep the source gate narrow and rely on the RUNTIME assertion instead — the
  converted fixtures, which is what caught this. That argues for finishing 0708's
  follow-up across every board rather than widening the grep.

## Progress — the runtime assertion, extended

Taking the third option. Board families whose logging fixture now publishes NO
sink list of its own, so a pass proves the BOARD published one:

| board | boot funnel it asserts | |
| --- | --- | --- |
| threadx-linux | `run_bare` | PASS |
| threadx-riscv64 | delegates to `nros-board-threadx` | PASS |
| zephyr-native-sim | `run_tiers` | PASS |
| freertos-mps2 | `nros_board_freertos::run_entry` | PASS |
| nuttx-qemu-arm | `nsh_main` (this issue) | PASS |
| esp32-qemu | `run_bare` | PASS |

`nros-board-esp32-qemu` did not depend on `nros-log` at all — the crate was named
only in a comment — so issue 0708's call there had never compiled in any
configuration that reached it. Same shape as `nros-board-mps2-an385`, where the
dep was optional behind two features while the funnel's module was ungated. Both
are now unconditional, for the reason the issue gives: a funnel that cannot
publish is the defect.

### The one family still unasserted, and why it is not a simple fix

`mps2-baremetal` has no image that exercises its board's funnels.
`logging-smoke-mps2-baremetal` enters through `#[entry] fn main()` in the fixture
and never reaches board code, so converting it asserts nothing — it would only
break a correct image. The board's three funnels (`entry.rs`, `rtic.rs`,
`node.rs::run_bare`) are covered by the source gate and by nothing at runtime.

Closing that needs a DIFFERENT image — one that boots through the board entry and
emits a record — not a change to this fixture. Left open deliberately rather than
papered over by converting the one image that cannot test it.

**Correction (same day): the image boots through the funnel but emits through the
OTHER facade, so it cannot carry the assertion either.**

`examples/qemu-arm-baremetal/rust/listener` does declare
`nros-board-mps2-an385 = { features = ["board-entry"] }` and enters via
`nros::main!()`, so it reaches `entry.rs` — the funnel that publishes. But every
line it emits is `log::info!` (`src/lib.rs:30-37`), which is the **log crate**,
not `nros_log`. Those records reach the console through the board's separate
log-crate logger and never touch the sink list, so a grep of this image's output
is green whether or not `init_default()` ever ran.

That is precisely the trap listed three lines below this paragraph — "the record
must come from `nros_log`, not `printf`" — with a third facade in the role of
`printf`, and I walked into it while writing the route down. It is also the
original issue-0708 confusion recurring: ThreadX and NuttX wired `log` and not
`nros_log`, and the two facades coexisting is exactly what makes a board look
instrumented while `nros_log` is dead.

So the mps2 gap is NOT a missing assertion on an existing image. It needs an
image that both (a) boots through a board funnel and (b) emits through
`nros_log`. Today no mps2 image does both: the smoke fixture does (b) and not
(a); every example does (a) and not (b). That is a real piece of work — either
give an existing example an `nros_info!`, or give the smoke fixture a board
entry — and neither is a one-liner.

**The image exists (superseded — see the correction above).** `examples/qemu-arm-baremetal/rust/listener` declares
`nros-board-mps2-an385 = { features = ["board-entry"] }` and enters through
`nros::main!()`, so it boots via `entry.rs` — the funnel that publishes. The gap
is therefore not "no image reaches the board" but "no ASSERTION rides the image
that does".

What remains for whoever takes it:

* the record must come from `nros_log`, not `printf` — a board that publishes no
  sink list still prints its C-side banner, so grepping boot output proves
  nothing unless the line asserted is one `nros_log` dispatched. `nros_info!`
  from the node body qualifies; the platform banner does not;
* the assertion belongs on an existing qemu-arm-baremetal runtime cell rather
  than a new fixture, since the image is already built and booted there;
* it closes `entry.rs` only. `rtic.rs` and `node.rs::run_bare` are separate
  funnels with separate images, and the runtime coverage claim should say which
  funnel it covers — the mistake this whole issue is about is a check whose
  coverage is narrower than the rule it appears to enforce.

The third option is the most honest: a booted image that emits nothing is the only
check that cannot be fooled by a spelling. It costs a fixture per board family
and it found this defect on its first run.
