---
id: 1206
title: "`nros build --all` plans N images, announces N images, then `execvp`s the FIRST one and never returns — images 2..N are silently never built"
status: open
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
