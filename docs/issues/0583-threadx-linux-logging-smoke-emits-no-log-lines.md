---
id: 583
title: "The ThreadX-Linux logging smoke fixture runs, exits 0, and emits no log
  lines at all"
status: open
type: bug
area: platform-threadx
related: [issue-0582, issue-0155, issue-0163, issue-0243]
---

## Symptom

`logging_smoke_threadx_linux_captures_stderr` fails on the assertion, not on a
missing fixture:

```
Expected output to contain '[TRACE] smoke: trace payload', but it was not found.
```

The binary is not broken in any way it reports. Run directly it exits **0** and
prints its full C-side boot sequence:

```
  nros ThreadX Platform (bare)
[app_define] Creating byte pool...
[app_define] Running board network init...
[app_define] Creating app thread...
[app_thread] Calling Rust entry...

Application completed successfully.
  Done
```

So the kernel boots, the app thread starts, the Rust entry is entered AND
returns cleanly. Only the six log lines the fixture exists to produce are
missing — every severity, not a filtered subset.

## What the fixture asks for

`packages/testing/nros-tests/bins/logging-smoke-threadx-linux/src/main.rs` is
short and does nothing conditional:

```rust
let _ = ThreadxLinux::run_bare(|| {
    register_logger(&LOGGER);
    init(sinks::default());
    LOGGER.set_level(Severity::Trace);
    nros_trace!(&LOGGER, "trace payload");
    // … debug/info/warn/error/fatal …
    nros_log::flush();
    Ok::<(), &'static str>(())
});
```

The level is set explicitly to `Trace`, so this is not a threshold problem, and
`flush()` is called, so it is not buffering at exit. The test spawns the binary
with no arguments and no environment, so there is no unset knob involved.

## Candidate causes, none confirmed

Ranked by how well each fits "runs to completion, produces nothing":

1. **The board's log writer is never registered.** `run_bare`'s contract (per
   the fixture's own doc comment) is that the board registers the ThreadX log
   writer before the closure runs. If that registration rides a weak hook or an
   fn-ptr slot whose defining archive member is not pulled, the sink silently
   stays the null one — the link succeeds and the image does nothing. This is
   the force-link class of issues 0155/0163 and of #0582's `+whole-archive`
   work, which is what makes it the leading candidate.
2. **`sinks::default()` resolves to a no-op on this platform** — a cfg path that
   selects no sink rather than stderr.
3. **The closure never runs.** Least likely: `run_bare` returning `Ok` without
   invoking its argument would be a much louder bug, and the C side does report
   entering the Rust entry.

Note that (1) predicts exactly this signature: no error, no partial output, no
diagnostic — because a missing sink is indistinguishable from a quiet program.

## Why it is not attributed to #0582

Found while validating #0582 on an aarch64 host. It cannot be bisected there:
**before #0582 this tree does not link on aarch64 at all** (the
`nros_platform_{tcp,udp}_*` undefined-reference set), so there is no baseline
build to compare against. The fixture has almost certainly never run on this
host.

One #0582 hypothesis was tested and **falsified**: `libglue.a` holds the board's
strong overrides of weak hooks, and losing them to demand-driven member
selection would produce this exact symptom. `+whole-archive` was added to that
archive and the symptom did not change. The modifier was kept — the reasoning
for it stands on its own, and the archive is three objects — but it is NOT a fix
for this issue and should not be read as one.

## Next step

Settle candidate (1) before theorising further: check whether the board's log
writer symbol is present in the linked image and whether its registration ran.

```sh
b=build/cargo-fixtures/threadx-linux/nros-relwithdebinfo/logging-smoke-threadx-linux
nm -C "$b" | grep -i -e 'log_writer' -e 'register_logger' -e 'sinks'
```

If the registration symbol is absent, it is (1) and the fix is the force-link
class. If present, instrument `sinks::default()` on this platform for (2).

Get an x86 baseline in parallel — if the test passes there, the defect is
host-arch-dependent and belongs with #0582's family; if it fails there too, this
is a long-standing ThreadX-Linux gap that no lane was watching.
