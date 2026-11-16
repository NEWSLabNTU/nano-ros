---
id: 2650
title: "Every parameter store slot carries a 256 B description buffer, about 15 KB at 32 slots when no value bound is paid"
status: open
type: enhancement
area: [memory, params]
severity: low
found: 2026-11-16
related: [2647, 2649, phase-478]
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
— follow-up to I26. This is the next largest part of a slot once issue 2647 has
removed the string-array product.

## Problem

A store slot is `Option<ParameterEntry>`, and the entry carries an
`Option<ParameterDescriptor>` inline. The descriptor holds a second copy of the
name (`MAX_PARAM_NAME_LEN`, 64 B) and a description of
`MAX_PARAM_DESCRIPTION_LEN` bytes, default 256. The description is code-supplied
text, so nothing derives its bound.

Measured on host x86_64 with every value bound at 0: `ParameterValue` is 16 B
and a slot is 480 B. At the default 32 slots that is 15,360 B, almost all of it
descriptor.

## Possible shapes

- Default `MAX_PARAM_DESCRIPTION_LEN` to 0 on embedded boards. This is a board
  fact already (`[knobs.params]`), so it is cheap, but it drops the text that
  `ros2 param describe` shows.
- Hold descriptions out of line, sized by what the code actually passes. That
  needs the same arena machinery as [issue 2649](2649-param-string-array-shared-arena.md).
- Drop the descriptor's copy of the name, since the entry's `Parameter` already
  holds it, and build the name only when the describe reply is written.
