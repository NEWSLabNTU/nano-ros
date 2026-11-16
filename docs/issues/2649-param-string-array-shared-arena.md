---
id: 2649
title: "Once any string_array is declared, every parameter slot pays the string-length x element-count product"
status: open
type: enhancement
area: [memory, params]
severity: low
found: 2026-11-16
related: [2647, 2650, 0756, phase-478]
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
— follow-up to I28. Issue 2647 stopped images WITHOUT a string array from paying
for one. This issue is about the images that do declare one.

## Problem

A parameter store slot holds its value inline, and the value is as large as its
largest variant. After issue 2647 the string array is
`Vec<String<MAX_STRING_VALUE_LEN>, MAX_STRING_ARRAY_LEN>`. So as soon as one
`string_array` is declared anywhere in the image, every slot costs about
S x SA bytes (8.3 KiB at 256 x 32), whatever the slot holds. A store of 32
slots with one string array among them is still about 285 KB.

## Possible shape

Move string payloads out of line:

- A string or string-array value holds an offset and a length into one shared
  byte arena.
- The arena is sized from the declarations. That is the sum of each declared
  string parameter's bound, not the slot count times the worst case.
- A slot then costs its scalar or array size plus a handle.

The open questions are what make this more than a type change:

- A `set` that grows a string needs room. Either the arena reserves each
  parameter's bound up front, which keeps no fragmentation and still saves the
  per-slot product, or it compacts.
- A declaration with no contract has no declared bound, so it needs a fallback
  size and a refusal when the arena is full, the way a full store already
  refuses (issue 2634).
- The C and C++ getters copy out today. Out-of-line storage must keep that, so
  no caller holds a pointer across a `set`.

## When

Only once a real image declares string arrays and runs short of RAM. Issue
2647's measurement shows no in-tree example that does.
