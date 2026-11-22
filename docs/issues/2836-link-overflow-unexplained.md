---
id: 2836
title: "A link that overflows RAM or flash fails with the linker's bare `region overflowed` line; `nros build` never sees it, so it cannot say what the image reserves"
status: open
type: enhancement
severity: medium
area: [cli, build, ux, stack-floor]
related: [issue-2783, issue-2400, phase-557]
found: 2026-11-22
---

**Home: [phase-557](../roadmap/phase-557-stack-floor-measured-frames.md)**,
beside [issue 2783](2783-graph-discovery-overflows-rust-template-ram.md),
whose first symptom this is.

## What happens

A static addition to an image that has little RAM to spare fails at the
link. Turning on zenoh graph discovery in the stock Rust talker template on
`nucleo-f767zi-zephyr` gives:

```text
region `RAM' overflowed by 7680 bytes
zephyr/zephyr_pre0.elf section `noinit' will not fit in region `RAM'
```

That line is all the user gets. They don't learn that the image's
`stack.conf` reserves a 75,536 B main stack from a stack-floor row, what the
heap knob is, or which knob is theirs to move. The same holds for a CMake or
cargo image that overflows a region.

## Why `nros build` cannot explain it today

`builder/handoff.rs` hands the tool the terminal itself, by design. West,
ninja and cargo inherit stdout and stderr, so colour, progress output,
`isatty` and signals behave as under a bare `cargo build`. Nothing of the
linker's output passes through `nros build`, so there is nothing to scan.

## Wanted (manager ruling, 2026-11-22)

- **Capture once, for every road.** While the tool's output streams to the
  terminal as now, it is also written to `build/<image>/<tool>.log`
  (`west.log` for the west road). One function in `handoff`, used by the
  west, CMake and cargo roads alike. Nothing is re-run after a failure: that
  would double the output, and on a big image it is not quick.
- **Keep the terminal a terminal.** A plain pipe makes the child see no
  TTY: colour, cargo's progress bar and line buffering change. The capture
  should keep the child on a pseudo-terminal (or otherwise preserve
  `isatty`), and say so where it cannot.
- **Explain from the log, on failure only.** When the build fails and its
  log holds `region '<R>' overflowed by <N> bytes`, print what the build
  already knows:
  - the region and N;
  - the main stack and where it came from (the image's `stack_floor_image`
    row in `config/stack-floors.toml`, its static, peak and margin, and
    whether the walk is only an upper bound);
  - the heap knob and its value;
  - the image's own stack knob, as the way out.

  Never suggest turning the stack-floor check off: that is not a fix, and the
  check stays on by default.
- A successful build prints nothing new.

## Acceptance

- Tests on the log scanner: a captured overflow line yields the
  explanation, and a log without one yields nothing.
- On the 2783 image (stock talker, discovery on), the failed build prints
  the explanation under the linker's line.
- The terminal behaviour of a successful build is unchanged, measured: cargo's
  progress bar and colour still appear.
