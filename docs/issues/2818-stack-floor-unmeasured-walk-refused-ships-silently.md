---
id: 2818
title: "An unmeasured image whose stack-floor walk is refused ships on its default stack with one warning line, and the walk has no frame for Cyclone's register entry or the FreeRTOS port's assembly"
status: open
type: limitation
severity: high
area: [memory, stack-floor, freertos, cyclonedds]
related: [2400, 2784, phase-557]
found: 2026-11-22
---

## What

The mps3-an536 FreeRTOS Cyclone C++ workspace image (`demo_bringup`,
`mps3_an536_entry`) overflows its app task stack at boot. It was found in the
tier-2 run 44916540238.

- On the carrier's 64 KiB default it prints `*** STACK OVERFLOW: app ***`
  after its first delivery.
- At 128 KiB it reports `app task stack peak 77916 of 131072 bytes`.

The stack-floor check ran on that build and caught nothing. Its output ended:

```text
- functions reached with no frame in any input (a missing frame would count as 0): nros_rmw_cyclonedds_register, vPortDisableInterrupts, vPortEnableInterrupts, vPortYield
nros-stack-floor: WARNING: the walk was refused for an UNMEASURED image; not enforced
```

Two gaps meet here.

1. **The walk is refused, and for an unmeasured image a refusal enforces
   nothing.** That is ruling (A)(2) of issue 2400, 2026-11-10: "An unmeasured
   image WARNS and is never refused, even under CHECK". The ruling was made for
   a walk that RAN and produced a floor the twin had not yet confirmed. Here no
   floor was produced at all, and the result is one `WARNING` line in a build
   log. It does not say:
   - which image or RMW it is about;
   - what stack the image ships with;
   - that the stack is therefore unchecked.

   The image ships on a default whose only measurement is a zenoh image on
   another board.
2. **The walk has no frame for four functions it reaches:**
   - `nros_rmw_cyclonedds_register`, Cyclone's register entry, from the
     backend's cmake-built `vtable.cpp`;
   - the FreeRTOS port's `vPortDisableInterrupts`, `vPortEnableInterrupts` and
     `vPortYield`, which are assembly.

   So any Cyclone image on FreeRTOS is refused, whatever its shape, and it
   stays unchecked until both kinds of input reach the walk.

The app stack's default and the image's regression are a separate matter,
being investigated on their own: the same entry ran on 64 KiB on 2026-10-02.
That fix does not close this issue. A refused walk would still enforce nothing
on the next image that outgrows its default.

## Done when

- [ ] The walk has a frame for each of the four functions, or each is stated
  as an input with a reason, so a Cyclone FreeRTOS image's walk runs and
  produces a floor. Acceptance: that image's walk is not refused.
- [ ] A refused walk on an unmeasured image says what it refused for:
  - the image and RMW;
  - the stack the image ships with;
  - that this stack is unchecked;
  - the functions with no frame.

  Within ruling (A), this is a louder warning, not a refusal.
- [x] Whether a refused walk (no floor at all) should FAIL an unmeasured
  image's build: **no, decided by the maintainer 2026-11-22.** Ruling (A)(2)
  stands, and an unmeasured image warns loudly and is never refused. The fix is
  a walk that sees the frames, plus a warning that names what it could not
  check.

## The cause, found 2026-11-22

Four Cyclone service functions reserved a 64 KiB stack-local wire buffer each
(`uint8_t wire[65536]`, `sub sp, #65536` in the prologue):

- `service_take_request_len`;
- `service_send_response`;
- `service_send_request_raw`;
- `service_take_response_raw_len`.

`Executor::pass` reaches them every spin for the default parameter services,
which every image has had since 2026-11-14. The backend fix moves the buffers
off the stack: the image then peaks at 19,552 B on 64 KiB.

That makes the image the pre-fix image this issue's walk needs as a negative
control. The walk must price those 65,536 B frames plus `Executor::pass`
(9,216 B), and its warning must say so: one frame alone is the whole stack.
## Why the walk could not have seen them (read 2026-11-22)

It did not stop at the frameless edges. A missing frame counts as 0, and the
walk still sums the rest of the path, then refuses. The 64 KiB frames were on
no path at all:

- `Executor::pass` reaches each `service_*` function only through the RMW
  vtable, which is an indirect call.
- `config/stack-floors.toml` has an indirect policy only for zenoh
  (`[indirect.zenoh] mode = "bind"`, issue 2402). Cyclone falls to
  `[indirect.default]`, whose `mode = "allowance:0"` charges an indirect call
  0 bytes and walks nothing behind it.

So with all four missing frames supplied, the walk would have produced a floor
near the measured 19 KB without the buffers, and passed the 64 KiB stack.
Supplying the frames is necessary but not enough.

Two smaller defects make it worse:

- A `WalkRefused` carries only text. The lower bound the walk had already
  summed is dropped, so the warning cannot say how deep it got.
- The `allowance:0` warning ("not counted") is one line among the build's
  output, like the refusal itself.

## Done when, revised

- [ ] **Cyclone's vtable calls are bound and followed** (`[indirect.cyclonedds]
  mode = "bind"`, the zenoh machinery from issues 2402 and 2516). Acceptance:
  the pre-fix an536 image's walk prices the four 65,536 B frames.
- [ ] The four frameless functions get a frame or a stated reason, as above.
- [ ] A refused walk keeps its lower bound. When that bound is already above
  the image's stack, the warning says so: a certain overflow, not an estimate.
- [ ] Every other backend on `[indirect.default]` is listed, with whether its
  vtable calls are walked. Same class: an RMW with no bind policy has every
  frame behind its vtable priced at 0.
