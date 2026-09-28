---
id: 1525
title: "C or C++ on an ESP32 has no build road, and the road that claimed to serve it could not"
status: open
area: build, cli, esp32
severity: low
phases: [468]
rfcs: [0065]
related: [1282, 1499, 1526]
---

# C or C++ on an ESP32 has no build road

`nros build` has three roads (RFC-0065 D3): cargo, cmake and west. None of them
serves an ESP32 image whose package graph crosses languages, and as of the
D3 amendment of 2026-09-28 that combination **refuses** rather than choosing a
road that cannot work:

```
$ nros build demo_bringup:esp32
Error: `demo_bringup:esp32` cannot be built: this image's board is an ESP32
board and its package graph crosses languages, and nano-ros has no build road
for that combination.
…
Tracked as issue 1525, which states what re-adding an ESP-IDF road would need.
```

This issue exists so that refusal points somewhere, and so the acceptance for
undoing it is written down rather than re-derived.

## What used to be there, and why it was not a road

`Driver::IdfPy` answered this combination until the amendment. It exec'd
`idf.py build` in the bringup package directory. Four things, each measured on
`main`:

1. **No project.** `idf.py build` needs an ESP-IDF project root
   (`include($ENV{IDF_PATH}/tools/cmake/project.cmake)`, a `main/` component, an
   sdkconfig). The bringup package is not one, and nothing said it had to be —
   `needs_generated_root()` is false for that driver precisely so nano-ros emits
   nothing there.
2. **No component.** phase-468 W2 (`083d2c10d`) deleted `integrations/nano-ros/`,
   the IDF component shell. The tree has no `idf_component.yml`, no
   `idf_component_register`, no `Kconfig.projbuild`, no `partitions.csv`.
3. **No carrier.** `docs/reference/canonical-build-path.md`'s carrier cell for
   that road was empty and `native_handoff` attached no environment, so every
   resolved knob would have reached the compiler at its compiled-in default.
4. **No tool.** `nros-sdk-index.toml` has no ESP-IDF entry, and stage 3
   preflight never probed for `idf.py`, so the first sign of trouble was
   `execvp` after every earlier stage reported green.

## What DOES work on ESP32 today

* **Pure Rust, bare metal** — `nros-board-esp32-qemu` (esp-hal,
  `riscv32imc-unknown-none-elf`), over the cargo road.
  `book/src/getting-started/esp32.md` is the walkthrough;
  `examples/esp32-c3-baremetal/rust/{talker,listener}` are the leaves.
* **Nothing else.** `book/src/getting-started/esp32.md` already says so: *"C /
  C++ apps on an ESP32 have no in-tree path today."*

Zephyr-on-ESP32 would be the other candidate — `platform = "zephyr"` is the
`West` road and would carry knobs through `$DOTCONFIG` like every other Zephyr
image — but it is unsupported for its own reasons (issue 1282, which dropped
`hal_espressif` from the west allowlist for want of a consumer).

## Acceptance — what re-adding an ESP-IDF road would need

Not "restore the enum variant". A road is four things, and the deleted one had
one of them:

- [ ] **A component.** An IDF component shell nano-ros ships
      (`idf_component.yml` + `idf_component_register` + `Kconfig.projbuild`),
      so an IDF project has something to depend on. This is what W2 deleted;
      restoring it means owning it.
- [ ] **A carrier.** A stated mechanism that delivers a resolved knob to that
      build — an sdkconfig emitter, a `-D` set on the handoff, or an env on the
      `Handoff` — with a row in `docs/reference/canonical-build-path.md`. A road
      with an empty carrier cell is what this issue is about.
- [ ] **A tool.** A `[tool.esp-idf]` / `[sdk.esp-idf]` entry in
      `nros-sdk-index.toml` so `nros setup` can provision it, plus a stage 3
      preflight probe so a missing `idf.py` fails where the remedy is named.
      RFC-0014's own estimate for this is ~1.4 GB, which is a cost worth
      restating before anyone signs up for it.
- [ ] **A lane.** A `fixtures.toml` row and a CI lane that builds it, per
      `library.json`'s standing precedent for this exact framework: *"espidf
      removed on the same precedent (no PIO/IDF test lane). Re-add only
      alongside a real integration."*

Until all four exist, an out-of-tree ESP-IDF integration is the answer and is
not forbidden — `book/src/getting-started/integration-esp-idf.md` describes the
shape.

## Why this is `low` and not `medium`

Nobody has asked for it, no in-tree workspace reaches it, and the previous state
was strictly worse: the combination reported a tool failure instead of an
unsupported configuration. The severity is about the gap, not about the change
that exposed it.
