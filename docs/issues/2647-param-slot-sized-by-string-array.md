---
id: 2647
title: "Every parameter store slot is sized for a 32 x 256 B string array when an image declares a string and any array, even with no string array"
status: open
type: enhancement
area: [memory, params]
severity: medium
found: 2026-11-16
related: [0756, 1033, 2634, 2649, 2650, phase-478]
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
— I26. The parameter store is the largest RAM item in the parameter surface on
an embedded image, and this is the biggest lever in it.

## Problem

A parameter store slot holds one `ParameterValue`, and an enum is as large as
its largest variant. That variant is the string array,
`Vec<String<MAX_STRING_VALUE_LEN>, MAX_ARRAY_LEN>`: 32 strings of 256 bytes at
the defaults, about 8.3 KiB. Every slot pays it, whatever type the slot holds,
so a store of bools and doubles costs the same as a store of string arrays.

The build already derives each bound from the contract's `params:`
declarations. A bound no declared type uses derives to 0. But the string array
is the PRODUCT of two bounds that two different needs size:

- `MAX_STRING_VALUE_LEN` is needed by any `string` (or `string_array`);
- `MAX_ARRAY_LEN` is needed by any `bool_array`, `integer_array`,
  `double_array` (or `string_array`).

So an image with one `string` parameter and one `double_array`, and no string
array anywhere, sizes every slot for 32 x 256-byte strings.

## Measured

Host x86_64, `size_of` with the knobs stated by environment (S is the string
length, A the array length, B the byte-array length):

| S / A / B | `ParameterValue` | `ParameterStorage<32>` |
| --- | --- | --- |
| 256 / 32 / 256 (defaults) | 8,464 B | 285,696 B |
| 256 / 0 / 0 | 272 B | 23,552 B |
| 0 / 32 / 0 | 272 B | 23,552 B |
| 0 / 0 / 0 | 16 B | 15,360 B |

The first row is what a string plus any array costs. The second and third rows
are what the same image would cost if the string array were not in the product.

### On an embedded image, before and after the fix

`heap-free-poc-mps2` (thumbv7m, mps2-an385) holds a caller-owned
`ParameterStorage<4>` in a static, `PARAMS`, so `just mem-report` reads the
store directly. No in-tree embedded image declares both a `string` and an array
without a `string_array`. So the second row states the capacities that such a
declaration produces through the entity inventory: the board's
`NROS_MAX_STRING_VALUE_LEN=256` and `NROS_MAX_ARRAY_LEN=32` (both needed), with
the derived `NROS_DECLARED_MAX_STRING_ARRAY_LEN=0` and
`NROS_DECLARED_MAX_BYTE_ARRAY_LEN=0`. "Before" ignores the string-array
variable, because it has no such bound.

| Image state | `PARAMS` before | `PARAMS` after | Per slot |
| --- | --- | --- | --- |
| nothing stated (defaults) | 35,104 B | 35,104 B | 8,776 B, unchanged |
| a `string` + a `double_array`, no `string_array` | 35,104 B | 2,880 B | 8,776 B to 720 B |

`just mem-report --baseline` on the second row gives `section RAM -32,224,
symbol RAM -32,224`. Every other symbol is unchanged.

## Fix (the shape agreed before building)

1. A `string_array`'s element count is its own bound, `MAX_STRING_ARRAY_LEN`.
   - It is a registry knob like its siblings:
     - the environment variable `NROS_MAX_STRING_ARRAY_LEN`;
     - `[knobs.params] max_string_array_len` on a board;
     - `CONFIG_NROS_MAX_STRING_ARRAY_LEN` in Kconfig, with -1 meaning derive.
   - Only a declared `string_array` needs it. The descriptor field is
     `[params] needs_max_string_array_len`, and the CMake and Zephyr carriers
     are `NROS_DERIVED_MAX_STRING_ARRAY_LEN` and
     `NROS_PARAM_NEEDS_MAX_STRING_ARRAY_LEN`.
   - `MAX_ARRAY_LEN` now bounds only the bool, integer and double arrays.
   - With no declarations the default stays 32, so an image that states
     nothing is unchanged.
2. `ParameterValue::StringArray` is
   `Vec<String<MAX_STRING_VALUE_LEN>, MAX_STRING_ARRAY_LEN>`, so the largest
   variant is the largest of S, 8 x A, B and S x SA (where SA is the new bound).
3. Every reader of the old coupling moves with it:
   - the parameter services' derived buffer bound (`param_sizing.rs`) prices a
     string array at SA elements;
   - the C string-array view table (`nros-c` `parameter.rs`) has SA rows;
   - a launch list of strings is refused past SA.

## Not in this fix

- [Issue 2649](2649-param-string-array-shared-arena.md): once any `string_array`
  is declared, every slot still pays S x SA. An out-of-line shared string arena
  would make the slot hold an offset and a length instead.
- [Issue 2650](2650-param-descriptor-description-per-slot.md): every slot also
  carries a 256 B description, about 460 B a slot even when every value bound
  is 0.

## Acceptance

- A unit test asserts that `ParameterValue` is no larger than its largest
  variant plus a tag, with the string array sized by `MAX_STRING_ARRAY_LEN`
  (`nros-params` `types.rs`).
- `just mem-report` on an image with a `string` and a `double_array` and no
  `string_array`, before and after, with the delta written here.
