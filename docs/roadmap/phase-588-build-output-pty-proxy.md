# Phase 588 -- `nros build` proxies the native tool through a pty, keeps its log, and explains a link overflow

**Status (2026-11-23). OPEN: W1-W4 not started.** Implements the
[RFC-0065](../design/0065-colcon-like-workspace-builder.md) D1 amendment of
2026-11-23 (user ruling: option (a), stay alive and proxy) for
[issue 2836](../issues/2836-link-overflow-unexplained.md). Pairing: the pty
and signal plumbing (W1, W2) is an engineer's; the log reader, the
explanation text and the book (W3, W4) are docs/UX.

## Why

A link that overflows a memory region fails with the linker's bare line:

```text
region `RAM' overflowed by 7680 bytes
```

The user does not learn what the image reserves (a 75,536 B main stack from
a stack-floor row, for the stock Rust talker on nucleo-f767zi-zephyr), what the heap knob is, or which
knob is theirs to move. `nros build` cannot tell them: stage 5 hands the
native tool the terminal by inheritance, so none of its output passes through
`nros build`. That was deliberate (RFC-0065's reconciliation with RFC-0024
§2.4: diagnostics must reach the user byte-identical). A plain pipe would
break that: the child would see no TTY, and colour, cargo's progress bar and
line buffering would change. So the capture goes through a pseudo-terminal.

## Work items

### W1 -- the pty proxy (RFC-0065 D1, amended)

- **Status:** OPEN (engineer)
- **What:** `builder::handoff::wait` runs the native tool on a
  pseudo-terminal whose size is the caller's. `nros build` copies every byte
  the tool writes to its own stdout unchanged, and also to
  `build/<image>/<tool>.log` (`west.log`, `cmake.log`, `cargo.log`). It is
  one function for the west, CMake and cargo roads. When `nros build`'s own
  stdout is not a terminal (CI, a pipe), the child gets no pty either, as
  today, and the bytes are still teed.
- **Acceptance:**
  - colour survives: a `cargo build` with a warning prints it in colour
    through `nros build`, as bare;
  - cargo's progress bar still draws;
  - the log holds the same bytes the terminal got.

### W2 -- signals and the exit status (RFC-0065 D1, amended)

- **Status:** OPEN (engineer)
- **What:** SIGINT and SIGTERM sent to `nros build` reach the child's process
  group; SIGWINCH resizes the pty. `nros build` exits with the child's own
  status, unmapped: a code is that code, and a signal is 128 + signal.
- **Acceptance:**
  - a Ctrl-C mid-build stops the compiler, leaving no orphaned
    ninja/rustc;
  - a failing tool's exit code is `nros build`'s;
  - a resize mid-build reaches the child.

### W3 -- read the log on failure, and explain a region overflow (issue 2836)

- **Status:** OPEN (docs/UX)
- **What:** after a failed build only, scan its log for
  `region '<R>' overflowed by <N> bytes` (GNU ld and LLD spellings). On a
  match, print under the tool's output what the build already knows:
  - the region and N;
  - the image's main stack and its source: the `stack_floor_image` row of
    `config/stack-floors.toml`, its static, peak and margin, and whether the
    walk is only an upper bound;
  - the heap knob and its value;
  - the image's own knob to move, as the way out.

  Never suggest turning the stack-floor check off. A successful build, and a
  failure with no overflow line, print nothing new.
- **Acceptance:**
  - unit tests on the scanner (an overflow line in each linker's spelling
    yields the explanation; a log without one yields nothing);
  - on a real overflowing image, the explanation prints under the linker's
    line.

### W4 -- the book and the reference

- **Status:** OPEN (docs/UX)
- **What:** `reference/build-outputs.md` names the per-tool log and when it
  is written, and the Troubleshoot page's "the build stops on RAM" entry
  shows the explanation.
- **Acceptance:** the pages match what W1-W3 print.
