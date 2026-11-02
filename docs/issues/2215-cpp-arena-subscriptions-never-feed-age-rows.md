---
id: 2215
title: "C/C++ arena subscriptions never feed their age row, so a C++ image reports silence at arming and its hazard observer stays latched"
status: open
type: bug
area: [executor, monitor]
found: 2026-11-02
related: [phase-462, issue-2173, issue-0758]
---

## The gap

The silence lease (`silence-runtime`, phase-462 W2) and the hazard observer
built on it (`hazard-detected`, `hazard-route-runtime`, phase-462 W3b,
play_launch phase 96 W4.2) read `SubMonitorCell::takes`. Only the Rust
typed subscription paths feed it (`arena::observe_age`, through `AgeMon`,
and only with an epoch source and a stamped type). The C/C++ callback path
(`nros_cpp_subscription_register` ->
`add_arena_subscription_c_callback_with_capture`) attaches no age cell at
all, so a C++ component's contracted subscriber records nothing, epoch or
not.

## Evidence (safety island, MR-CANHUBK344, nano-ros 4153448193)

Every board act's violation record and trace carry, right after arming:

    silence-runtime /mrm_handler/operation_mode_availability measured=0 declared=500
    hazard-detected safety_island.launch.xml/hpc_loss measured=510 declared=10000

and nothing after it: the availability stream arrives at 10 Hz for the whole
run (the island's own handler takes it; the act tables read its longest gap at 114-118 ms), yet
the hazard never recovers, so the act's real HPC loss (the gate stopped,
~30 s later) is never detected by the observer, and no rung route is judged
during an act. The island's own reaction (its handler's 500 ms timeout) and
its supervisor are unaffected; only the image's contract monitors are
blind. The island's run notes had read this as "no epoch on the board"; the
C++ path would not feed the row with one either.

## Direction

The C++ templates know the message type, so the register call could carry
the type's stamp offset (as the Rust `RosMessage::STAMP_OFFSET`) and the
arena entry could observe the age exactly as the Rust path does. Separately,
with no epoch source the silence lease could count arrivals (a statement
about arrival, which needs no epoch) while the age promise is reported
unjudgeable once, instead of as a silence that latches every hazard on the
guard. Which of the two the policy wants is a design decision (the doc on
`SubMonitorCell::observe` calls "report it silent" the intended verdict for
an unjudgeable age), so it is not changed here.
