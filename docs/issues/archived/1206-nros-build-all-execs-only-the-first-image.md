---
id: 1206
title: "`nros build --all` plans N images, announces ONE, then `execvp`s the FIRST one and never returns — images 2..N are silently never built"
status: resolved
area: [cli, build]
severity: high
related: [phase-383, "RFC-0065"]
---

# `--all` is advertised, planned, printed — and then abandoned by `exec`

## What happens

`nros build` stage 5 is an `execvp`, deliberately: RFC-0065 D1's whole
diagnostic guarantee ("stage 5 is `exec`, not a pipe … a rustc error is
byte-identical to `cargo build`'s, because nothing is capturing it") rests on
the build process *becoming* the compiler. `builder::handoff::exec` says so in
its type — it returns `Result<Infallible, String>`, and its own doc reads
**"Never returns on success."**

`cmd::build::run` nevertheless drives it from a loop over *every* planned
image:

```rust
// packages/cli/nros-cli-core/src/cmd/build.rs:807-846
let plans = plan_builds(&args)?;
for p in &plans {
    eprintln!("nros build: {} -> board {} (platform {}), driver {}", …);
    …
    if args.dry_run { … continue; }
    if let Some(cfg) = &p.configure { run_configure(cfg)?; }
    // Never returns on success: this process BECOMES the build.
    let err = crate::builder::handoff::exec(hand).unwrap_err();
    eyre::bail!("{err}");
}
```

The comment on the last line is correct and is exactly the problem: on the
first successful `exec`, iteration stops — not by `break`, but because the
process ceases to exist. Plans 2..N are never reached.

## Why `plans.len() > 1` is a normal invocation, not a corner

Three ordinary paths produce more than one plan:

* **`--all`.** `build.rs:211-217` expands it to `plan::all_images(&bringups)`,
  every image in the workspace. It is not an internal flag — `plan::resolve`'s
  own error text *advertises* it to the user
  (`builder/plan.rs:248`: `build all:   nros build --all`), and a test asserts
  the offer is present (`plan.rs:315`).
* **Several positional images.** `images: Vec<String>` (`build.rs:34`);
  `plan::resolve` pushes one plan per requested name (`plan.rs:179-185`).
* **Several `default_images`.** A bare `nros build` maps every declared default
  through `pick_one` (`plan.rs:188-197`).

`examples/workspaces/rust` declares eight images across three cross
toolchains — the workspace RFC-0065 D1 uses as its worked example for why
`--all` exists at all.

## Why it is silent

The loop prints one `nros build: <image> -> board … driver …` line per plan
**before** the exec, so the terminal shows all N announcements. The user then
sees one build run to completion and the process exit 0 (the first image's
status, inherited by `execvp`). Nothing distinguishes "built 8 images" from
"built 1 image and announced 7". The exit code carries the first image's
verdict, so a broken image 5 cannot make the command fail.

`--dry-run` is the one mode that behaves as documented (`build.rs:828-834`
`continue`s instead of exec'ing), which is why every dry-run-based test of the
multi-image path passes.

## Coverage

There is no test exercising `--all` (or two positional images) through `run()`
on the non-dry-run path. `Args { all: false, … }` is the only construction in
the test module (`build.rs:2640`). The planner is tested; the driver loop is
not.

## The design collision underneath

This is not only a coding slip — `exec` and multi-image are mutually exclusive
as specified. RFC-0065 D1 gives stage 5 two properties that cannot both hold
for N > 1:

1. the process **becomes** the native tool, so diagnostics are untouched; and
2. `--all` builds **every** image.

Any fix chooses. Candidate shapes, in increasing cost:

* **Refuse.** `plans.len() > 1 && !dry_run` → error naming the images and
  telling the user to run them one at a time, or to use a wrapper. Cheapest,
  honest, and deletes the advertised `--all` capability.
* **Fork per plan, exec in the child, wait in the parent, exec the LAST one.**
  Keeps property 1 for the final image only; the earlier N-1 have their stderr
  inherited but their process is not replaced. Output is still uncaptured
  (no `Stdio::piped()`), so the diagnostic guarantee arguably survives —
  worth measuring against `handoff.rs`'s stated invariant before claiming it.
* **`spawn` + `wait` for all N**, keeping stdio inherited, and reserve `exec`
  for the single-plan case. Two code paths, which `--dry-run`'s design note
  ("the property that makes `--dry-run` trivially correct instead of a second
  code path") explicitly argues against.

Whichever is chosen, the accompanying test must run the non-dry-run path with
two plans and assert both artifacts exist — a dry-run assertion cannot catch
this class.

## Evidence

* `packages/cli/nros-cli-core/src/cmd/build.rs:807-846` — the loop.
* `packages/cli/nros-cli-core/src/builder/handoff.rs:129-131,175-186` —
  `exec`'s `Infallible` return and `cmd.exec()`.
* `packages/cli/nros-cli-core/src/cmd/build.rs:211-217` — `--all` expansion.
* `packages/cli/nros-cli-core/src/builder/plan.rs:179-197,248` — multi-plan
  resolution and the user-facing `--all` offer.

Found by reading, not by running: reproducing it needs a workspace whose
images all build, which is a multi-hour fixture cycle. The read is unambiguous
(`execvp` does not return), but a reproduction on
`examples/workspaces/rust` would settle the exact observed output.

## Reproduced, 2026-09-08 — and the report was wrong about the output

Run on `examples/workspaces/rust` after `nros sync`, with two images that both
build, so the first `exec` SUCCEEDS (the case the bug needs):

```
$ nros build native_service_client native_service_server ; echo "rc=$?"
nros build: demo_bringup:native_service_client -> board native (platform posix), driver cargo
    Finished `dev` profile [optimized + debuginfo] target(s) in 8.90s
rc=0

$ ls target/debug/ | grep native_service
native_service_client_entry
```

Two plans, **one** announcement, **one** artifact, exit **0**.
`native_service_server_entry` does not exist. `--all --dry-run` on the same
workspace plans **17** images, so the plan set is real and this is not a
resolution problem.

**Correction to "Why it is silent" above.** It claimed "the loop prints one
`nros build: …` line per plan *before* the exec, so the terminal shows all N
announcements." It does not. The announcement is inside the loop, immediately
before that plan's exec, so exactly ONE is ever printed. The observed output is
therefore *more* silent than reported: nothing on the terminal refers to images
2..N at all. A user cannot tell a multi-image invocation from a single-image
one by reading its output.

## Resolution — the last plan execs, the rest are waited on

Of the three candidate shapes above, **refuse** is the one that had to be
rejected on evidence rather than taste: RFC-0065 D1 specifies `--all` in its own
prose ("`--all` builds every image"), F10 records *building N images at once* as
routine, and D1 gives `[system] default_images` as naming the default **set** —
so a bare `nros build` in a workspace declaring two defaults is an ordinary
invocation that refusing would break. Deleting the capability is an RFC
amendment, not a bug fix.

So the fix is the second shape, and the reason it is admissible is that **the
guarantee is "nothing is capturing it", not "there is exactly one process"**.
`builder::handoff::wait` runs a plan as a child with **inherited** stdio — no
`Stdio::piped()` anywhere in that file, still — so `isatty`, colour detection,
line buffering and the compiler's own diagnostics are untouched, the tool's exit
status is returned rather than remapped, and the child sits in this process's
foreground process group so Ctrl-C reaches it. What is lost is the fourth D1
property alone: a second process appears in `ps`.

`handover_for(index, total)` spends the `exec` on the **last** plan, so:

* a single-image `nros build <image>` — the invocation D1's prose describes — is
  unchanged in every respect, including its announcement line; and
* the final image of a multi-image run still gets the full guarantee.

Two smaller consequences, both about the silence rather than the build:

* a multi-image run prefixes each announcement `[i/N]`. That counter is the
  *only* progress reporting available, because the last plan execs and no
  epilogue can run after it; `[3/7]` as the last line is how a reader learns
  where it stopped.
* a non-final image that fails is wrapped with which image it was and that
  nothing after it was attempted, instead of exiting with a status no longer
  attributable to anything.

Verified with the same command:

```
$ nros build native_service_client native_service_server ; echo "rc=$?"
nros build: [1/2] demo_bringup:native_service_client -> board native (platform posix), driver cargo
    Finished `dev` profile [optimized + debuginfo] target(s) in 54.85s
nros build: [2/2] demo_bringup:native_service_server -> board native (platform posix), driver cargo
    Finished `dev` profile [optimized + debuginfo] target(s) in 0.46s
rc=0

$ ls target/debug/ | grep native_service.*entry$
native_service_client_entry
native_service_server_entry
```

## Coverage added

`cmd::build::drive` is the loop, split out of `run` for exactly the reason the
issue names: `plan_builds` is pure and heavily tested, `run` needs a real
workspace on disk, and the loop between them could be reached by no test. It
takes the handover as a parameter, so a test can observe every plan's handover
without a compiler and without a process that stops existing mid-assertion.

`multi_image_drive_tests` — six tests, and the load-bearing one is
`every_plan_reaches_a_handover_and_only_the_last_execs`. Measured against a
negative control that restores the pre-fix shape (`handover(hand, Exec)?;
return Ok(())` inside the loop):

```
test every_plan_reaches_a_handover_and_only_the_last_execs ... FAILED
  assertion `left == right` failed: one handover per plan, not one per
  invocation: [("true", Exec)]
    left: 1
   right: 3
test a_failing_earlier_image_stops_the_run_and_is_named ... FAILED
```

Three plans, one handover — the bug, reproduced in a unit test. With the fix,
6 passed.

The issue asks the accompanying test to "assert both artifacts exist". That form
cannot live in a test here: it needs a compiler, and *no compilation inside
tests* is a project rule. It is instead the reproduction above, run by hand, and
`a_waited_handover_actually_runs_the_command` carries the checkable half — a
real `Wait` handover whose command's effect on the filesystem is asserted.
