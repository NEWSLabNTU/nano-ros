---
id: 1138
title: "`rustdoc-links` fails CLOSED when a vendored source is not provisioned, so every fresh worktree reads as a documentation defect"
status: resolved
type: bug
area: ci, tooling, docs
severity: medium
related: [1043, 1110, 1116, 0390, 0650]
found: 2026-09-06
resolved: 2026-09-07
---

# "I cannot build the docs here" is reported as "your docs are broken"

## Symptom

On the fast line — and therefore in the `pre-push` hook — in any checkout where
`packages/rmw/zenoh/zpico-sys/zenoh-pico` is not provisioned:

```
zenoh-pico source not provisioned at ".../zpico-sys/zenoh-pico". Run: nros setup
--source zenoh-pico (or git submodule update --init
packages/rmw/zenoh/zpico-sys/zenoh-pico). — #0390
error: recipe `rustdoc-links` failed
check-fast (parallel): 1 of 235 gate(s) FAILED
pre-push: REFUSING to push — the fast gate tier is red.
```

The tree is fine. The host has not provisioned an optional vendored source.

## Measured, on four independent sessions in one day

`rustdoc-links` arrived 2026-09-06 in `6359314e5` (issue 1110, "the docs deploy
was red for three days, and no lane could say so"). Within hours it blocked four
agent sessions working in `git worktree`s, none of which had touched Rust:

| session | what it was fixing | what it did |
| --- | --- | --- |
| issue 0986 | pre-push hook side effects | pushed with `NROS_SKIP_PREPUSH_CHECKS=1`, reported it |
| issue 1015 | C array extent floors | initialised the submodule; **verified the red reproduces identically on unmodified `origin/main` by stashing** |
| issue 1029 | zephyr nightly cron | pushed with `--no-verify`, recorded the reason in the commit |
| issue 1016 | tier-2 west leaves | initialised the submodule |

Two workarounds, two bypasses, zero real defects. A gate whose ordinary outcome
is a bypass has stopped being a gate.

## Cause

`just check rustdoc-links` (`just/check/docs.just:229`) is only:

```bash
source scripts/build/rustdoc-set.sh
mapfile -t pkgs < <(nros_rustdoc_package_args)
cargo doc --no-deps --quiet --features "$NROS_RUSTDOC_FEATURES" "${pkgs[@]}"
```

It asserts nothing about provisioning. The failure comes from a DEPENDENCY's
build script: `cargo doc` runs build scripts, and
`nros-zpico-build/src/runner.rs:999` **panics** when the source is absent:

```rust
if !use_side && !zenoh_pico_src.join("include").exists() {
    panic!("zenoh-pico source not provisioned at {:?}. ... — #0390", zenoh_pico_src);
}
```

That panic is correct for a BUILD — an image cannot be produced without the
source. It is wrong as the verdict of a DOC-LINK gate, which is asking a
different question, and it is the gate that reports.

## The class, and the fix that already exists for it

This is issue **1043** one gate over: *a gate that fails when it cannot evaluate
is indistinguishable from one that found a defect.* 1043 fixed exactly this for
`check-submodule-pins`, and the shape of its fix applies here:

* three outcomes — `FAIL` (documented, links broken) / `NOT VERIFIED` (source not
  provisioned here) / `OK`;
* the narrowing REPORTED, not silent — and reported through the shared
  `nros_check_skip` ledger, because `run-gates-parallel.sh` discards the stdout
  of every gate that exits 0, so a happy-path message is invisible on the push
  lane (issue 0650's shape);
* a remedy that splits by audience — local: "`nros setup --source zenoh-pico`
  turns this skip into a verdict"; CI: name the lane that was supposed to
  provision it;
* an env override (`NROS_RUSTDOC_LINKS_STRICT=1`) restoring fail-closed, wired
  to whichever lane genuinely provisions the source.

The check must be made BEFORE `cargo doc`, since the panic comes from inside it.

## Fixed — 2026-09-07

### The half that had already landed, and why it was not enough

`5673356fc` (an hour after this was filed, for issue 1110) took `rustdoc-links`
OFF the fast line via `.config/gate-lane-exempt.txt`. That removed the symptom
the four sessions actually hit — the gate no longer runs in `pre-push` — and
changed nothing about the gate. Run by hand in any unprovisioned checkout it
still reached the build script and still answered "your documentation is
broken" when the true answer was "I cannot build the docs here". A lane move
relocates a wrong verdict; it does not correct one.

### The alternative, measured rather than left open

"Not covered" asked whether the published-crate set need depend on `zpico-sys`
at all, since narrowing the features would beat reporting a skip. Measured: it
cannot.

* `packages/rmw/zenoh/nros-rmw-zenoh/Cargo.toml` declares
  `zpico-sys = { version = "0.5.0", path = "../zpico-sys", default-features =
  false }` — a plain, NON-optional dependency. No feature makes it disappear;
  features only change what zpico-sys *compiles*.
* The panic in `nros-zpico-build/src/runner.rs` sits ahead of every backend
  branch, on the `backend_count == 0` path that a plain `cargo doc` takes — the
  same path whose own comment says "reached on plain `cargo doc` … perfectly
  normal".

So the only way to sidestep the build script would be to drop
`nros-rmw-zenoh` from the documented set, and that is the crate whose dangling
`record_alloc_ceilings` is half the reason the gate exists. The source is a
genuine precondition; the fix is to REPORT it as one rather than borrow a
build's panic.

### The fix

Three outcomes where there were two, in issue 1043's shape:

    FAIL          rustdoc RAN and a link is dead.
    NOT VERIFIED  a vendored source the doc build needs is absent here, so
                  rustdoc never ran and nothing was measured either way.
    OK            rustdoc ran and the published crates document cleanly.

* The precondition is checked BEFORE `cargo doc`, because the panic is inside
  it.
* The required sources are a TABLE beside the crate list in
  `scripts/build/rustdoc-set.sh` (`NROS_RUSTDOC_SOURCE_REQS` /
  `nros_rustdoc_missing_sources`), so widening `NROS_RUSTDOC_CRATES` for issue
  1116 lands next to the note that a new provisioning-dependent build script
  needs a row.
* The narrowing goes through the SHARED ledger (`nros_check_skip`), not a
  happy-path print: `run-gates-parallel.sh` discards the stdout of every gate
  that exits 0, which is issue 0650's shape.
* The remedy splits by audience — locally `nros setup --source zenoh-pico`
  turns the skip into a verdict; under `$GITHUB_ACTIONS`/`$CI` it says the job
  did not provision it and nothing the author pushes will.
* `NROS_RUSTDOC_LINKS_STRICT=1` restores fail-closed. Unlike 1043's, it IS
  wired to a lane: `gate.yml`'s own `rustdoc-links` step runs after "Build nros
  CLI + provision compile-tier sources", so there an absent source is the
  workflow's regression and must be red. It is set on that step and nowhere
  else — 1043's rule, that setting it on a lane which provisions a subset
  re-creates the issue, applies unchanged.

### Both directions measured

Unprovisioned worktree (`…/zpico-sys/zenoh-pico/include` absent):

    $ just check rustdoc-links
    rustdoc-links: NOT VERIFIED — vendored source 'zenoh-pico' is not provisioned here.
        expected at: packages/rmw/zenoh/zpico-sys/zenoh-pico/include
        needed by:   nros-rmw-zenoh -> zpico-sys build script (nros-zpico-build, issue 0390)
      Locally: 'nros setup --source zenoh-pico' … turns this skip into a verdict.
    [SKIPPED] rustdoc-links: vendored source(s) not provisioned: zenoh-pico — rustdoc
      never ran, so the 6 published crates' doc links were NOT checked
    rc=0

    $ CI=true just check rustdoc-links          # CI remedy arm
      In CI: THIS JOB did not provision it, so the doc links were not checked.
    rc=0

    $ NROS_RUSTDOC_LINKS_STRICT=1 just check rustdoc-links
    rustdoc-links: FAILED — NROS_RUSTDOC_LINKS_STRICT=1 and the source(s) above are absent.
    rc=1

Provisioned, with a dead link injected into `packages/core/nros-rmw/src/lib.rs`
— an intra-doc link to `ClientTrait::is_server_ready`, the exact break issue
1110 was filed for —
the gate must still catch its own bug:

    $ just check rustdoc-links
    error: unresolved link to `ClientTrait::is_server_ready`
     --> packages/core/nros-rmw/src/lib.rs:3:56
    error: could not document `nros-rmw`
    rc=101

    $ NROS_RUSTDOC_LINKS_STRICT=1 just check rustdoc-links   # the CI configuration
    rc=101

Mutation reverted, same tree:

    $ NROS_RUSTDOC_LINKS_STRICT=1 just check rustdoc-links
    rustdoc-links OK — the 6 published crates document cleanly.

## Still open

* Whether other gates share the shape. `rustdoc-links` was found because four
  sessions hit it in one day; nobody has swept the rest for "fails closed on an
  unprovisioned optional source". That sweep is the real class and is still not
  attempted. Note the two ledger-reporting sites now in this family
  (`check-submodule-pins`, `rustdoc-links`) plus the pre-existing
  `zpico-config-keys`, which already skipped on this exact predicate — a gate
  in `just/check/rmw.just` had the right shape for the same source a phase
  before this gate got it wrong.
* Issue 1116 (~70 rustdoc diagnostics outside the published set) is a different
  problem: it is about WIDENING this gate, not about it failing to evaluate.
