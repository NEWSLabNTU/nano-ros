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

The third is the most honest: a booted image that emits nothing is the only
check that cannot be fooled by a spelling. It costs a fixture per board family
and it found this defect on its first run.
