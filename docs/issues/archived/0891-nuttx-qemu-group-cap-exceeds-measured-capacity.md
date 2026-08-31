---
id: 891
title: "Six nuttx `rtos_e2e` cells fail in-sweep and pass solo — the group cap
  was never measured, and a slow boot is reported as a dead image"
status: resolved
type: bug
area: testing
related: [issue-0838, issue-0445, issue-0196, phase-287]
---

## Symptom

Tier 2 (`just ci-matrix`) reports six real failures in `rtos_e2e`, the
`Platform__Nuttx` C and C++ cells of pubsub / service / action:

    nuttx E2E failed — readiness pattern 'Waiting for messages' not observed.
    Output so far (truncated):
    <no output collected: Timeout waiting for condition>

They pass run one at a time. Two run together fail 3/3 on this host.

This is the shape CLAUDE.md already records from phase-287 W7 — "six nuttx
lanes failed 3/3 in-sweep, passed solo" — where the standing advice is to
retest a QEMU red solo before filing. That advice is right and it is also all
there was: nothing stopped the sweep from producing the six false reds again.

## Why it reads as a dead image

`<no output collected>` is not "the image printed nothing interesting". It is
`collect_until` returning `Err(Timeout)`, which it does **only when the capture
is empty** — so a boot that is merely slow is rendered as no boot at all, and
`ensure_ready` then blames the missing banner. Booting the same image by hand
prints normally within seconds.

That is issue 0445's absorbing-verdict class in a second place: the message
that replaces the runtime result explains itself, so the reader stops there. It
cost me an hour of chasing a nonexistent nuttx runtime regression, including a
`rm -rf` + rebuild to rule out issue 0820's museum-binary class (it is not that
— the failure survives a clean rebuild).

## Two causes, both real

**1. Boot budgets sized for a native process, and keyed on the wrong axis.**
All three e2e shapes waited a flat `Duration::from_secs(30)` for the readiness
banner, with no platform scoping at all. On nuttx that wait covers a cold QEMU
boot, the app's 5 s startup sleep and a zenoh session open — the talker-window
comment a few lines below already measured those at ">15 s before the first
publish". 30 s left almost no margin.

The post-boot windows *were* carved out, but on `(Platform::Nuttx, Lang::C)`,
while every reason their comments give — "cold QEMU boot", "QEMU slirp +
zenoh-pico TCP are routinely in the 40–60 s range" — is a property of the
platform. The C++ image on the same board fell through to the native defaults.

**2. A group cap that is not a count of tests.** `[test-groups.qemu-nuttx]`
covers all nine cells (membership verified with `cargo nextest show-config
test-groups`, per 0838), but `max-threads = 9` authorises **eighteen** arm-virt
QEMU instances under `-icount shift=auto`, because each cell spawns a talker
and a listener. 9 appears to be a slot count that was never measured against
what the host can actually emulate.

## Fix

* `boot_budget(platform)` — ONE helper, used at all three banner waits;
  emulated platforms get 90 s, ThreadX-Linux (a native process, no emulator)
  keeps 30 s.
* The three nuttx windows re-keyed from `(platform, lang)` to `platform`.
* `qemu-nuttx` `max-threads` 9 → 1.

Measured effect of the budget change alone: the six cells went from TRY-3-FAIL
to flaky-pass-2/3, and the failure moved on — the listener now reaches
`Waiting for messages` and what is missing is the talker. So the budgets were
genuinely wrong, and they were not the whole story; the cap is the rest.

## What this does NOT fix

The cap throttles *our* parallelism. It cannot throttle other agent sessions
building on the same host, and this host is shared: measurements here were
taken at load 2.4 to 15.4 with other sessions running builds. A constant tuned
against a contended box is not a capacity measurement, which is why the budget
fix matters independently of the cap — it is the part that does not depend on
what else is running.

If these cells flake again on a quiet host, the next thing to question is the
talker's `wait_for_output(talker_window)`: it drains for a fixed window rather
than waiting for a marker, so it cannot early-exit and cannot say what it was
waiting for.

## Measured result, and the one cell this does NOT fix

One run of all nine cells, after both fixes: **5 passed (2 flaky), 4 failed**.
Three of the four "failures" are `Lang__Rust` `skip!` panics at ~0.09 s, which
nextest counts as failures outside `just test-all`'s junit rewrite. So pubsub
and service, C and C++, recovered.

The fourth is real and is a DIFFERENT BUG: `test_rtos_action_e2e` /
`Platform__Nuttx` / `Lang__C` fails 3/3 **run entirely alone**, so it is not
concurrency and 0891 does not explain it. The image gets further than any
timeout would suggest:

    Action client created: /fibonacci
    Sending goal
    Failed to send goal: -2          # NROS_RET_TIMEOUT
    (Is the action server running?)

with the server sitting at `Waiting for action goals` and printing nothing
after. The deadline that expires is INSIDE the image, so no test-side budget
can move it — this needs its own issue and its own diagnosis.

One dead end worth recording so nobody repeats it. Both the service and action
shapes skip the settle delay for NuttX:

    if !matches!(platform, Platform::Nuttx | Platform::ThreadxLinux) {
        std::thread::sleep(platform.stabilization_delay());
    }

and the comment directly above the service one says the delay is to stop "its
first query racing ahead of the server queryable's declaration. Only applies to
QEMU-cold-boot platforms" — while excluding NuttX, which IS one, and which
`stabilization_delay` names explicitly ("QEMU cold-boot + zenoh connect — ~15 s
is typical"). Code contradicting its own comment, and the exact race the `-2`
looks like. **Un-excluding NuttX does not fix it** — measured, still 3/3 `-2`
with a 20 s settle — so the contradiction is real but is not this bug's cause,
and the change was reverted rather than landed for costing 20 s a test to fix
nothing.

## Closed

Both acceptance items were already met when this was written — the fix is in the
tree (`boot_budget(platform)` at `rtos_e2e.rs:598`, `[test-groups.qemu-nuttx]
max-threads = 1`) and the measured result is recorded above. The issue simply
was never closed.

The one cell it explicitly did not cover, `test_rtos_action_e2e` /
`Platform__Nuttx` / `Lang__C`, is [[issue-0867]] and stays open: it fails 3/3
run entirely alone, so it is not concurrency and nothing here explains it.

## Acceptance

* The pubsub and service `Platform__Nuttx` C and C++ cells pass in ONE run
  rather than only one at a time. (Met: 5 passed / 2 flaky, down from six
  failing.)
* A slow boot no longer reports as `<no output collected>` with no indication
  that the wait, rather than the image, is what expired. (Met: `ensure_ready`
  now names an empty capture and points at the budget and the cap.)
* NOT in scope: the `action` / `Lang__C` `-2` above, to be filed separately.
