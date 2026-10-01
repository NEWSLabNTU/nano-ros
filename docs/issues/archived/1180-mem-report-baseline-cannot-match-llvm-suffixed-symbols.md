---
id: 1180
title: "`mem-report --baseline` silently reports no delta for LLVM-internalised symbols — including `EXECUTOR_BACKING`, the largest RAM symbol in every Rust image"
status: resolved
resolved: 2026-10-01
type: bug
area: tooling
related: [phase-392]
---

## Problem

`scripts/nros-mem-report.py --baseline` matches symbols by NAME:

```python
base_syms = {t["symbol"]: t["bytes"] for t in baseline.get("top_ram", [])}
...
if row["symbol"] in base_syms:
    d = row["bytes"] - base_syms[row["symbol"]]
```

A symbol LLVM internalises carries a per-build `.llvm.<hash>` suffix, and the
reporter keeps it in the name. The hash is not stable across builds of the same
crate, so the lookup misses and the row is printed with **no delta annotation** —
which is exactly how the tool prints a symbol that did not change.

Measured, two builds of the same test binary differing only in one source line:

```
baseline: nros_node::executor::backing::EXECUTOR_BACKING (.llvm.12488621184790092911)  86,216
current : nros_node::executor::backing::EXECUTOR_BACKING (.llvm.13033674138034542000)  86,216
```

Same bytes here, so the missing annotation happened to be right — but it would
have been printed identically had the number moved, because the two names never
met. `EXECUTOR_BACKING` is 38.1% of that image's attributed RAM and is the
single symbol phase-392 W6 created *in order to be measurable*, so this is the
worst possible symbol to be unable to diff.

## Why it matters more than it looks

phase-392's standing rule is that no wave claims a saving it did not measure,
and `mem-report --json --baseline` is the named instrument. A `--baseline` run
that cannot match a symbol produces the same output as one that measured no
change, so a wave can read a clean before/after out of a probe that compared
nothing. W5 already had to withdraw a causal claim for the adjacent reason (a
before/after that built the same configuration twice) and W6 discarded a
-11,199 B reading for it.

## Fix

Normalise the symbol key before comparing: strip a trailing `.llvm.<digits>`
(and the sibling `.<digits>` suffixes LLVM appends for local symbols) when
building both `base_syms` and the lookup, keeping the full name for DISPLAY.
Where two symbols collide after normalisation, sum or report both rather than
silently taking one.

Give it a positive control, per `check-gate-selftests`' rule: a fixture pair
whose only difference is the `.llvm.` suffix must report the byte delta, and a
pair with equal bytes must report none. A `--baseline` that cannot fail is the
defect being fixed.

## Workaround until then

Read the number directly and compare it yourself:

```
nm -S <elf> | grep EXECUTOR_BACKING
```

Positive control for that reading, measured: rebuilding with
`NROS_SUBSCRIPTION_BUFFER_SIZE=2048` moves the symbol
`0x150c8` (86,216) -> `0x180c8` (98,504), +12,288 bytes.

## Resolution

`scripts/nros-mem-report.py` now joins on a build-independent KEY
(`symbol_key`): the demangled name with ` (.llvm.N)` / ` (.N)` display
suffixes, raw `.llvm.N`, `.N`, `.constprop/.isra/.part/.lto_priv/.cold…`
clone suffixes and the legacy-Rust `::h<16 hex>` crate hash removed; the
full name is kept for display. Two statics that collide on a key are SUMMED
and COUNTED on both sides, and a count change is printed. The `--json` output
carries the whole RAM table (`ram_symbols`, keyed), not only the top 40.

Nothing that fails to join is silent any more. Every row of the top list
carries `(+N)`, `(=)` for matched-and-unchanged, `(new)`, or — against an
old top-40-only baseline — "no baseline row", which is marked INCOMPLETE and
never used as evidence that a symbol is new. A `baseline join` section lists
matched/changed counts and every NEW and GONE symbol (the 131,072-byte
`LARGE_PAYLOADS` disappearance issue 1125 had to read by hand would now be a
`gone:` line), and the owner table gets a per-owner delta.

A second way the join failed is fixed with it: `llvm-nm -C` (LLVM 14) leaves
a suffixed v0 symbol MANGLED (`_RNv…ARENA_ADVISORY_DONE.0`), and a mangled
name carries the crate disambiguator, which changes between builds. Names are
demangled with `llvm-cxxfilt` when present (it handles v0 with the suffix);
otherwise `nm -C`, zipped by symbol-table order and refused if the counts
differ.

### Measured

Positive control, in the always-on selftest (`selftest_baseline`), the issue's
own pair: `EXECUTOR_BACKING (.llvm.12488621184790092911)` 86,216 →
`(.llvm.13033674138034542000)` 98,504 reports `(+12,288)`; equal bytes report
`(=)`; a symbol on one side only is listed as new/gone. Mutation: replacing
`symbol_key` with the identity fails both `selftest_keys` and
`selftest_baseline`.

On a real image — the Rust tiered `workspace-rust-native-realtime`, rebuilt
with only `NROS_SUBSCRIPTION_BUFFER_SIZE=2048` against a baseline at default:

```
vs baseline: section RAM +24,576, symbol RAM +24,576
       100,848  native_entry::__nros_entry_run::__NROS_TIER_EXECUTOR_BACKING  (+12,288)
       100,840  nros_node::executor::backing::EXECUTOR_BACKING  (+12,288)
       201,688  [executor storage]  (+24,576)
  matched 146 symbol(s), 2 changed; 0 new, 0 gone
```

(the issue's +12,288 for the boot backing, and the same again for the one
spawned tier's slot). And the FreeRTOS mps2-an385 C tiered image against the
main checkout's 2026-09-04 build of the same row (pre-1568):
`__nros_tier_executor_storage 178,144 (new)`, `[executor storage]
(+178,144)`, `matched 240 symbol(s), 9 changed; 30 new, 5 gone`, with the
five gone symbols listed.

### Not measured

That native build is not LTO, so its symbols carry no `.llvm.` suffix; the
`.llvm.` case is covered by the selftest on the issue's literal names, not by
a pair of ThinLTO images. The `.N` case is exercised on real images: the FreeRTOS C image has 125
`.N`-suffixed symbols (e.g. `nros_log::SINKS_PTR.0`, a v0 symbol LLVM 14's
`nm -C` leaves mangled), the Zephyr one `events.0` / `trace_options.0`.
