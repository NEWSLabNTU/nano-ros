---
id: 860
title: "The ESP32-C3 workspace Entry boots ESP-IDF and never reaches
  `Application setup complete` — first runtime look since W2 broke the build"
status: open
type: bug
area: boards, rmw
related: [phase-391, issue-0851, issue-0190]
---

## Symptom

`nros-tests::esp32_emulator test_esp32_workspace_entry_e2e` fails after 60 s:

```
ESP32 workspace Entry did not finish node registration:
ProcessFailed("esp32-qemu did not print `Application setup complete` within 60s")
```

The image is alive and gets a long way in — second-stage bootloader, chip
revision, flash config all print — then stops before node registration
completes. It is a hang or a silent early-return, not a boot failure.

```
I (0) boot: ESP-IDF v5.5.1-838-gd66ebb86d2e 2nd stage bootloader
I (0) boot.esp32c3: SPI Flash Size : 4MB
```

## Why this is being filed now, and what that implies

This is **not a new regression — it is the first time the test could run at
all in a while.** `6c5bd77bf` (phase-391 W2, "rlsf behind the funnel") gave
`zpico-alloc` both its `rlsf` dependency and an `AtomicUsize::fetch_add` on
`foreign_frees`. `riscv32imc-unknown-none-elf` has no `A` extension and so no
atomic CAS, so from W2 until issue 0851 the crate **did not compile for this
target** and no esp32 image existed to test.

0851 fixed the compile; this is what was behind it. So the bug is downstream of
W2's landing, and the search should start there rather than at 0851 — nothing
about a diagnostic counter changing from `fetch_add` to load+store plausibly
hangs a boot.

Note the same wave is implicated in the sibling reds found in the same run
(0859, 0861, 0862 were all pre-existing too), and W2 replaced the ALLOCATOR —
a component whose failure mode on a heap-starved target is exactly "gets part
way through setup and stops".

## Next measurement

1. Does it hang or return early? `Application setup complete` is printed at the
   end of setup; find the last log line the image DOES emit and bisect setup
   from there.
2. Suspect the allocator first, given W2: instrument `zpico-alloc` for an
   exhausted pool at registration time. `foreign_frees` and the pool bound are
   already tracked — read them rather than adding new counters.
3. Compare against the pre-W2 allocator on the same image if the bisect is
   ambiguous.

## Repro

    source ./activate.sh
    just build-test-fixtures lane=tier2
    cargo nextest run -E 'test(test_esp32_workspace_entry_e2e)'

Historic esp32-qemu delivery failures are archived issues 0190 and 0064 — both
resolved, and neither is this.
