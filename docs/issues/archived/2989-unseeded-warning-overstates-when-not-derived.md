---
id: 2989
title: "The unseeded-configure WARNING says the defaults 'can be smaller than this image needs' when nothing larger was derived"
status: resolved
type: ux
severity: low
area: [cmake, configure, diagnostics]
found: 2026-11-27
resolved: 2026-11-27
resolved_in: "fix-2989-unseeded-warning"
related: [2983]
---

## What happened

The autoware safety island's Zephyr FVP lane, at nano-ros 52fcb58126 (run
45868911896), configures its `fvp-tap-can` image with a plain
`west build -d build/zephyr-fvp-tap-can`. That configure prints:

    nros: message sizes not derived (97 of 213 message types are unbounded);
    size knobs keep their configured values
    …
    nros: this image's derived message-bound sizes was composed after this
    configure's readers ran, and no `nros configure` seeded them, so they used
    their defaults (Kconfig and crate values, which can be smaller than this
    image needs), while the late consumers of this configure read the composed
    answer (build/zephyr-fvp-tap-can/nros/message_bound_knobs.cmake).

The same line was in the nros11 builds. The code that prints it
(`_nros_promise_check_unseeded`, `cmake/NanoRosPromise.cmake:408`) is
unchanged since `nros-0.6.0-nros11`.

## Why it overstates

The WARNING is the non-derived arm of the unseeded check. When the composed
answer IS derived, the same function stops the configure with a FATAL,
because the early readers and the late consumers would then disagree. When
it is not derived, as here, both halves use the configured knob values:
- the readers got the defaults;
- the composed fragment says "size knobs keep their configured values".

They agree, and nothing larger was derived for the readers to miss. The
parenthetical "which can be smaller than this image needs" describes the
derived case, which is the FATAL. Here it suggests an undersize that cannot
happen. It also cost a release-acceptance round on 2026-11-27, to establish
that this line was not a remnant of issue 2983.

## What a fix looks like

- In the non-derived arm, say what is true: neither the readers nor the
  composer derived a size, so both used the configured values, and the
  image is sized as configured. Name what would make it derived. For message
  bounds that is bounding the unbounded types, which the "not derived" line
  above already counts.
- Keep the "can be smaller" wording for the arm it belongs to, the FATAL.
- Consider whether the non-derived arm needs a WARNING at all, or only the
  detail line (`nros_message_detail`). A configure that agrees with itself
  is not a fault. A warning on every such configure teaches readers to
  ignore the warning that does matter.

The user ruled on 2026-11-27, via the release session, that this is
post-release work.

## Fix (2026-11-27)

The non-derived arm no longer warns. It prints a STATUS line instead, which
says that the readers used their defaults and that the composed answer uses
the configured values too, so the image is sized as configured. It also
says what would make the sizes derived. It stays a visible line rather than
a verbose-only one, because two gates read it: `resolved-seed` reads "they
used their defaults", and `sizing-descriptor-reader` reads the label.
The "can be smaller than this image needs" wording moved into the derived
arm's FATAL, the only arm it is true of.

`just check promise-unseeded` (`tests/cmake-promise-unseeded-tests.sh`) runs
both arms of the real function under `cmake -P`:
- derived: the configure stops, saying the defaults can be smaller and that
  the answer is derived;
- not derived: the configure goes on, with no WARNING, no "can be smaller",
  and the "sized as configured" line.

On main's code the three not-derived checks fail, as they should.

