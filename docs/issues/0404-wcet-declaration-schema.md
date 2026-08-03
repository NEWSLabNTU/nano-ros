---
id: 404
title: No schema for declaring a measured WCET — where it lives, what it is
  keyed on, and what makes it trustworthy
status: open
type: enhancement
area: orchestration
related: [0259, 0403, rfc-0047, rfc-0060]
---

## Problem

`MapperPath.exec_ms` is `Option<f64>` and, outside rlm's own tests, nothing
ever sets it to `Some(..)`. There is no syntax anywhere in `system.toml`, the
launch tree, or the model for a developer to write down a WCET they measured.

As of rlm v0.1.4 the mapper at least admits this: a chain judged feasible with
missing WCETs raises `ChainFeasibleWithoutWcet` naming the boundaries counted
as zero (issue 0259, step 1). The warning is now the thing with no remedy —
it tells you evidence is missing and offers nowhere to put it.

The design constraint, from the outset: **vacant is the default, and stays
vacant.** A WCET is a property of code on a particular chip at a particular
clock, obtained by measurement. Anything that manufactures one — a default, an
estimate, a value inherited from a different board — reintroduces exactly the
unsoundness 0259 removed, but harder to see, because a declared number carries
more authority than an absent one.

## The questions the schema has to answer

**What is it keyed on?** A WCET measured on an STM32F407 at 168 MHz says
nothing about the same code on a Cortex-M3 in QEMU. Candidates:

- *Board id* — most precise, and the boards that can actually measure are a
  minority (issue 0403: on QEMU the DWT reads zero). Most entries would be
  absent, which is correct but makes the mechanism look broken.
- *Platform family* — fewer entries, but "cortex-m4f" spans clock rates that
  differ several-fold, so the number needs a rate anyway.
- *Named profile* — the declaration names a measurement context
  (`stm32f4-168mhz-release`) and the board selects one. Most explicit about
  the fact that a WCET belongs to a context rather than to code.

**What is the unit?** The mapper works in `ms`; the bench measures cycles.
Converting requires a clock rate, which means either the declaration is in ms
(and the conversion happened where the rate was known) or it is in cycles plus
a rate (and the model does the conversion). The first keeps the model simple;
the second keeps the measurement in the unit it was taken in.

**What granularity?** The mapper wants one `exec_ms` per timer boundary — a
whole callback. The bench measures primitives. Either the declaration is
per-callback and measured as such, or the schema defines how primitives
compose, which is a much larger claim.

**What makes it trustworthy later?** A number with no provenance cannot be
audited or invalidated. At minimum: what was measured, on what hardware, at
what clock, with which build profile, at which commit. And then — does a WCET
measured three commits ago still describe this callback? A stale WCET is worse
than an absent one, because absent is now loud and stale is silent.

## Direction

Nothing here should be designed before 0403 produces an artifact. The schema's
job is to carry a measurement from the producer to the mapper without losing
what makes it meaningful, and it cannot be designed against a producer that
does not yet emit one. Doing it in the other order would produce a schema
shaped around a hypothetical measurement, which is how the keying question
gets answered by guess.

The invariant to preserve, whatever the shape: **absent must remain
representable and must remain the default.** A schema that requires every
boundary to carry a WCET will get zeros written into it, and the tree will be
back where 0259 found it — with the added problem that the zeros are now
signed by a developer.
