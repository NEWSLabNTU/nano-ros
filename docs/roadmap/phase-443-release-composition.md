# Phase 443 — release composition

**Status (2026-09-10). All six work items implemented; W1/W5 and W3/W4 are in
review.** Implements
[RFC-0097](../design/0097-release-composition-and-version-axes.md). Makes the
`nano-ros` release say what it contains, so the axis that carries compatibility
can be read without reading the ones that do not.

**Prior phases:** 440 (the store is the root, RFC-0095), 431 W4 (the installer),
440 W7 (the per-project pin), 429 (RFC-0090's codegen version).

## Why this exists, in one ratio

Four things carry a version and one artifact carries all four. Measured on
`main`, 2026-09-10:

| axis | changes | over |
| --- | --- | --- |
| `packages/cli` | 710 | 60 days |
| runtime crates the CLI compiles | 248 | 60 days |
| `nros-sdk-index.toml` | 70 | 60 days |
| **`NROS_CODEGEN_VERSION`** | **2** | **all time** |

`release-nros.yml` asserts three of these are EQUAL, so a user cannot take one
without taking all — and the only one that can invalidate their existing
generated code is the one that barely moves.

`nano-ros` has **zero** releases today, so every item here is greenfield: no
installed base, no migration.

## Work items

### W1 — the index leaves the binary

`shipped_index()` reads `<prefix>/share/nros/nros-sdk-index.toml` out of the
release asset, so all 70 index commits per 60 days would each be a CLI release.
The index is a manifest of pointers into `nano-ros-sdk`'s per-tool releases; it
has no ABI, so a newer index is usable by an older CLI.

*Acceptance:* the index is fetched and cached in the store, not read from the
asset; a CLI resolves an index it did not ship with; the offline path is named
in the failure text rather than assumed (`NROS_INSTALL_URL` has a sibling here);
`nros setup --check` still works with no network when the cache is warm.

### W2 — the release DECLARES its components (RFC-0097 D7)

```toml
# share/nros/manifest.toml
version  = "0.7.9"      # the toolchain
codegen  = 7            # the only field that can invalidate existing output
index    = "2026-09-10"
nano_ros = "abc1234"
```

`release-nros.yml` stops asserting the three versions equal and starts recording
them.

*Acceptance:* two releases with the same `codegen` are shown to require no
re-emit, and one with a different `codegen` warns BEFORE doing anything;
`nros pin` reports the codegen delta across a bump; the manifest is read by the
CLI rather than parsed out of a filename. A gate asserts the manifest's
`codegen` equals the tree's `NROS_CODEGEN_VERSION` at release time — the
equality check that survives, because it is the one that is true.

### W3 — the launcher becomes its own binary (RFC-0097 D4)

phase-440 W7 put dispatch in `packages/cli/nros-cli/src/main.rs`, so the
fronted `nros` IS the full CLI and the launcher's lifetime is the CLI's. A
proxy that outlives what it proxies cannot share a release with it.

*Acceptance:* a second bin target containing only W7's `dispatch.rs` logic —
read the pin, ensure the toolchain, `exec`; W7's existing tests still pass
against it, including "an older launcher dispatches to a newer toolchain"; the
launcher does not link the CLI's dependency graph (measured, not asserted); a
contributor inside a checkout is unaffected, because dispatch declines at its
own seam.

**Landed.** `packages/cli/nros-launcher` — `dispatch.rs` and `pin.rs` MOVED out
of `nros-cli-core` (which re-exports them at their old paths, so there is one
parser of `nros-toolchain.toml`), plus `launch.rs` for the three states a
launcher meets and a fronted toolchain cannot: an empty store, an unpinned
project with no CLI to fall back to, and a cwd inside a checkout. The crate
carries its OWN `version`, which is the point — the workspace version is the
toolchain's, and sharing it is the coupling the split exists to break.

Measured, not asserted: `tests/launcher_dependency_closure.rs` walks the
launcher's link closure out of `packages/cli/Cargo.lock` — **27 crates against
the toolchain's 192**, with `nros-cli-core`, `clap`, `minijinja`,
`nros-pkg-index`, `nros-launch-parser` and `nros-entry-lower` named as forbidden
and a ceiling that a new direct dependency trips.

**Not done here, and it is the half a user meets:** `scripts/install.sh` still
fronts `<store>/bin/nros` at the newest TOOLCHAIN's `bin/nros` through
`sdk-front`, so nothing installs the launcher yet. Fronting it instead means the
release asset must carry both binaries and `sdk-front` must learn which one it
fronts — a change to `release-nros.yml`, which is W2's file. It belongs with
W2's manifest, not ahead of it. Until then the launcher is a built, tested
artifact that no install path delivers, and the dispatch a user actually gets is
still W7's in-toolchain one (`nros-cli/tests/toolchain_dispatch.rs`, which
still passes).

### W4 — CI refuses to write a pin (RFC-0097 D11)

Pin-on-first-build is right interactively and wrong in CI: silently pinning to
latest yields a green build against an unrecorded toolchain, which is the bug
the pin exists to prevent.

*Acceptance:* non-interactive + no pin ⇒ refuse, naming `nros pin <version>`;
interactive + no pin ⇒ write and say so; a test drives BOTH, since a guard with
one arm exercised is half a guard.

**Landed, with one correction to the acceptance above.** The refusal does NOT
name `nros pin <version>`: at the time W4 landed there was no such verb in this
tree, and a blocked CI job aimed at a command that does not run is a worse
refusal than none. What always exists is the FILE, so the diagnostic prints
exactly the `nros-toolchain.toml` that `nros build` would have written, and the
fix is a copy-paste. Escape hatch: `NROS_ALLOW_PIN_WRITE_IN_CI=1`, named in
every refusal.

W2 (PR #859) adds `nros pin <version>` and `pin::set`. When it lands the refusal
may name the verb BESIDE the file, never instead of it — the reader is on a
runner, where they cannot run a verb either, and the file is what they have to
commit. W2's `pin::set` belongs in `nros-launcher/src/pin.rs` now; that is the
one conflict between the two work items, and it is mechanical.

The rule is split across two places on purpose, because they know different
things:

* **`nros build` REFUSES** (`PinOutcome::RefusedInCi` → an `Err`). It knows it
  is about to write into the user's source tree.
* **the launcher WARNS** (`Source::DefaultInCi`), naming the version it took.
  It parses no `argv` (RFC-0095 D8), so refusing there would refuse
  `nros --version`, `nros setup --list` and `nros doctor` on every runner —
  commands that write nothing and have no reproducibility to protect.

The refusal is asked AFTER the three arms that write nothing anyway (a
contributor's checkout, an already-pinned project, a binary with no store
version). Ordering it first would fail every build in this repository's own CI,
which builds inside a checkout; `a_checkout_in_ci_is_not_a_pin_refusal` and
`an_existing_pin_is_read_identically_in_ci` are what hold it there.

### W5 — `setup --check` covers the build stage; `doctor` covers the install (D12)

`rosidl` is pulled in by a target build rather than a board, so
`nros setup <board>` cannot pre-empt it and a no-ROS host meets it as a build
failure. The message is already right; the timing is not.

*Acceptance:* `nros setup --check` on a host with no ROS reports rosidl and
names `nros setup --source rosidl`; `nros doctor` with NO workspace verifies the
install — launcher, store, pin resolution — instead of requiring one.

### W6 — document the product shape (RFC-0097 D8)

There is no `nros run` and there will not be one: flashing and starting an image
are properties of the board and the user's bench. That makes the documentation
load-bearing rather than optional — **a missing verb is a design decision; an
undocumented artifact is a defect.**

*Acceptance:* for each supported board, the book states what `nros build`
produces, where it lands, and the common way to run it. The templates stop
teaching the checkout model — `zephyr-byo/README.md` currently tells a user to
`git submodule update --init packages/cli && just setup-cli`, which is the
opposite of the installed path. Gated the way other doc claims are, so the list
cannot silently fall behind the board table.

## Order

W1 → W2 are the release change and stand alone: after them a user can tell
whether an upgrade costs them a re-emit, which is the whole user-visible point.
W3 is independent. W4 and W5 are small correctness fixes to phase-440's own
work. W6 is docs and can land any time, but it is what makes the product usable
by someone who has not read this repository.

## What landed, and where the plan was wrong

| item | PR | note |
| --- | --- | --- |
| RFC-0097 | #841 | merged |
| W6 — product shape | #852 | merged |
| W2 — the release declares its components | #859 | merged |
| W1 + W5 — index leaves the binary; `--check` covers the build stage | #854 | in review |
| W3 + W4 — the launcher is its own crate; CI does not choose a toolchain | #860 | in review |

W1 and W5 shipped together because both edit `cmd/setup.rs`; W3 and W4 because
both live in the launcher. That was a scheduling choice, not a design one.

### D9 is a decision, not a description — `nros sync` is NOT merged into `nros build`

Measured while writing W6's documentation, on a fresh copy of
`examples/templates/multi-node-workspace` with `generated/` and `build/` removed:

```
$ nros build --dry-run
Error: missing prerequisites for this build:
  - generated message bindings (this workspace has never been synced)
      run: nros sync
```

raised by `builder::preflight::check`. So the docs claiming "`nros build` runs
`nros sync` for you" were the false ones and were corrected; the ones telling a
user to sync first are currently right. **D9 remains unimplemented**, and
nothing in W1–W6 assumes otherwise. Whoever implements it should expect the doc
edits to move back.

### W4's refusal is at the BUILD seam, not the launcher

W4's acceptance said "non-interactive + no pin ⇒ refuse". The implementation
splits it, and the split is right: **the launcher parses no `argv`** (D8), so a
refusal there would also refuse `nros --version`, `nros setup --list` and
`nros doctor` on every runner — commands that write nothing and have no
reproducibility to protect. The launcher therefore emits a loud line naming the
version it took and the pin that would fix it (`Source::DefaultInCi`), and the
refusal lives where the verb is known.

### A build-stage source needs a RESOLVABLE location, not a `dest`

W5's first rule was `build_stage && dest.is_none() ⇒ refuse`, because the report
asks whether `dest` is populated. phase-440 (#830) then gave `[source.rosidl]`
`location = "store"` and REMOVED its `dest` — deliberately, since RFC-0095 D2
derives a store path so nobody can spell it twice. `rosidl` is simultaneously
the only build-stage source, so the two correct rules meeting refused the
SHIPPED index. Measured by restoring the old rule: **9 tests fail, not 2** —
every test that loads `nros-sdk-index.toml`, i.e. an `nros` that cannot read its
own manifest. The rule is now "can anything name where this lives".

### The ETXTBSY helper has ONE home, and it is the launcher

Two work items independently hit issue 0476 — a test writes or copies an
executable and then execs it, and `O_CLOEXEC` closes at exec, not fork, so a
sibling thread's fork holds a write handle. Measured at 4/60 failures on a
loaded machine, 0/60 with the fix. Both fixes were correct; the placement is
decided by layering. `nros-cli-core` DEPENDS on `nros-launcher` after W3, so the
helper lives in `nros-launcher` and cli-core re-exports it — a helper about a
race is the worst kind to keep two copies of.

## Non-goals

* **Extracting `nros-codegen` as a binary.** RFC-0097 D7: the CLI↔codegen seam
  changed 13 times in 60 days (of 23 all time), so the two would ship together
  anyway and the split would buy nothing. Revisit only if that seam stabilises.
* **An acceptance range for `NROS_CODEGEN_VERSION`.** D6 — exact match stays;
  codegen and the runtime are one unit.
* **`nros run` / `nros flash`.** D8.
* **Versioning `nros-sdk-index.toml` as an artifact.** It is a manifest of
  pointers; `nano-ros-sdk` already publishes per-tool, which is finer.
