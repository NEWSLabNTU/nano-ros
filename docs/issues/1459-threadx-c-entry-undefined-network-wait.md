---
id: 1459
title: "`threadx_linux`'s C fixtures fail to link on `undefined reference to
  nros_board_network_wait` — the weak definition is header-emitted, and the image
  that calls it has no TU that includes the header"
status: open
type: bug
area: [boards, threadx, testing]
severity: high
found: 2026-09-22
related: [1355]
---

## What happens

Nightly run **35698520560** (schedule, 2026-09-22T07:13), job **106651319072**
(`threadx_linux`), step `build-fixture-extras`:

```
FAILED: c_service_server
/usr/bin/ld: …/packages/boards/nros-board-common/c/nros_rtos_run_components.c.o:
  in function `nros_board_rtos_run_components':
nros_rtos_run_components.c:(.text+0x2a): undefined reference to `nros_board_network_wait'
collect2: error: ld returned 1 exit status
make: *** [build/fixtures-build-make/threadx-linux-c-zenoh-all-123358-16001.mk:16: fixture-0002] Error 1
```

## Why this is interesting rather than puzzling

**The tree predicts this failure in prose, in the header that is supposed to
prevent it.** `packages/api/nros-c/include/nros/main.h:205-224` explains that
the weak body used to live in `main.hpp`, that this "linked only because every
generated embedded entry is a `.cpp` including `main.hpp`", and that a pure C
entry "compiles fine and then fails at LINK with `undefined reference to
nros_board_network_wait`, because nothing else in the image defines it". The
fix recorded there is that the definition now lives in `main.h`:

```c
void nros_board_network_wait(void);
#if defined(__GNUC__) || defined(__clang__)
__attribute__((weak)) void nros_board_network_wait(void) {}
#endif
```

and `nros_rtos_run_components.c:99` declares it `extern` and calls it at :249.

That argument holds only if **some TU in the image includes `<nros/main.h>`**.
A header-emitted weak definition is emitted by its includers and by nobody
else. In the `c_service_server` fixture, evidently none does — the caller
declares the symbol `extern` and the header is never pulled in, so the weak
body is emitted nowhere and the link fails exactly as the comment describes.

## What this is NOT

- **Not 1355.** That is `threadx_riscv64` and a port/target mismatch. This is
  `threadx_linux`, a link error in a C fixture. (Note the same run's
  `threadx_riscv64` reports only `0/12 ok, 12 failed`, with 1355's decisive
  `THREADX_PORT=… targets x86_64` line absent from the job log — attributing
  that one needs its own look.)
- **Not 1353.** No disk exhaustion; the compile succeeded and `ld` spoke.
- **Not a missing override.** The header states, measured, that nothing in the
  tree provides a strong definition; the weak one is meant to be the answer.

## The sites, and why this is filed rather than patched

At least three are defensible and they say different things:

1. **`nros_rtos_run_components.c` includes `<nros/main.h>`** instead of
   declaring the symbol `extern`. Smallest change, and it makes the calling TU
   carry its own definition — but that file is shared by every RTOS board, so
   it pulls the entry header into images that never wanted it.
2. **The generated C entry includes `<nros/main.h>`.** Matches the C++ path
   exactly (there the entry TU is what emits the body), and keeps the board
   file free of the entry header. Needs the C entry emitter to do it for every
   road, which is the kind of "one spelling, many roads" that phase-432 is
   about.
3. **A real stub TU in `nros-board-common`.** The header's comment argues
   against this — a SECOND spelling of one symbol, plus a dependence on
   archive extraction — and that argument still stands.

Acceptance is the `threadx_linux` nightly job building `c_service_server`
again, and a C-only fixture in a lane that would have caught it: the C++ path
has always worked, so any check that compiles only the C++ entry proves nothing
about this.

## Cause measured, and fixed (2026-09-29)

**The failing images have no entry at all.** `examples/threadx-linux/c/talker`
is `nano_ros_add_executable(c_talker src/main.c)` — a hand-written `main.c`
that neither includes `<nros/main.h>` nor calls the shared runner. The runner
is in the image only because issue 1286 (2026-09-11) added
`nros_rtos_run_components.c` to `THREADX_STARTUP_SOURCE` for BOTH ThreadX
boards, which compiles it into every ThreadX image unconditionally. The four
reported targets (`c_talker`, `c_listener`, `c_service_client`,
`c_service_server`) are all of that shape.

That makes **option 2 above insufficient**: making the generated C entry
include `<nros/main.h>` would not have reached these images, because they
contain no generated entry. The premise that failed is the one the rv-virt
board file states in so many words — "nros_board_link_app links every app with
--gc-sections, and an unreferenced section's undefined references are not
reported". True of `rv-virt-threadx`; `threadx-linux`'s `nros_board_link_app`
is empty. So the runner's unreferenced function survived the link, and its
undefined reference was reported.

Dated by the job history: `threadx_linux` was green on 2026-09-09 and
2026-09-10 and red from 2026-09-11 — the day 1286 landed.

**Fix** (neither of options 1–3): the runner declares the hook as a **weak
reference** and calls it only when something defines it. That resolves to the
header's weak default or a board's strong override whenever either is linked —
every image that actually calls the runner, since its entry includes
`<nros/main.h>` — and when neither is, the default would have been a no-op, so
skipping is the same behaviour. It is a reference, not a definition, so
option 3's objection (a second spelling of one symbol) does not apply, and
unlike option 1 it pulls the entry header into nothing. Adding `--gc-sections`
to `threadx-linux` was also rejected: it changes every ThreadX Linux image's
link to rescue one TU, and leaves the runner correct only on boards that
remember the flag.

Verified locally, since `threadx-linux` is a host build: `examples/threadx-linux/
c/talker` reproduced the exact link error before the fix and links after. The
weak reference was exercised in all three states (no definition → skipped;
the `main.h` weak default → called; a strong override → the override runs),
and the TU compiles clean under gcc and clang with `-Wall -Wextra
-Wmissing-prototypes -Waddress -Werror`.

Stays **open**: acceptance above is the nightly `threadx_linux` job building
`c_service_server` again, plus a C-only fixture in a lane that would catch a
recurrence — and the second half is not done.
