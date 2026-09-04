# phase-422 — provisioning has one path, and CI walks it

**Status (2026-09-05). In progress. DONE: W1 (duplicate producers retired),
W3's dispatcher half (argument passthrough), W5 (the ratchet — four gates),
W6 (the scope verb reports the system closure), W7's gate half. IN FLIGHT:
W2's mdbook/verus half, W4, W7's additive board entries. OPEN BY DECISION: W8
(the refusal, report -> warn -> error), W3's prose sweep, and W7's true
unification — the index namespace is what users type while the cmake namespace
is what the build uses, and neither is obviously the one to keep.**

Implements the provisioning half of RFC-0014 and finishes what phase-413 W5
started. Prior phases: 413 (CI workflow user parity), 398 (`package.xml`
dependency ladder), 365 (versioned SDK store).

## The problem

There is supposed to be one way to provision this tree: `nros setup` reading
`nros-sdk-index.toml`, wrapped by `just setup <scope>`. In practice there are
four, and they disagree.

**1. Two producers for the same tool.** `[tool.corrosion]` in the index and
`just workspace install-corrosion` both install corrosion, into the same
versioned prefix, each with its own stamp logic. Same for
`play_launch_parser`. Their version pins are not even spelled the same:

| tool | `just/workspace.just` | `nros-sdk-index.toml` |
| --- | --- | --- |
| corrosion | `v0.6.1` | `0.6.1-nros1` |
| play_launch_parser | git SHA `838ce948…` | `0.1.0-nros1` |

Issue 0500 is the cost of this already being paid once: the SDK store
accumulates, prefixes resolve newest-first, and **both provisioning paths print
success either way** — so a stale Corrosion shadowed the pin that had just been
installed, and `mixed` could not link. A second producer is not a convenience;
it is a second answer to "which version is installed?".

`just workspace install-sccache` is NOT in this category — it is a thin wrapper
that shells `nros setup --tool sccache`. It should still lose the wrapper, but
it is tidying, not a correctness fix.

**2. Installers that never reached the index at all.** `clang-format` (pip
wheel download, `scripts/`-side logic), `mdbook` (`scripts/setup-mdbook.sh` +
a hand-maintained `scripts/mdbook-checksums.txt`), `verus`
(`scripts/setup-verus.sh`, reached by `just verify-verus`). Each re-implements what the index already does for
16 other tools: pinned version, download, checksum, install prefix, smoke
check. None of them can be asked "are you at the pin?" the way
`nros setup --tool X --check` can.

**3. Two spellings of the setup verb, one of which silently does less.**
`just setup <scope>` runs `_setup-common` then the module recipe;
`just <scope> setup` runs the module recipe alone. `_setup-common` is where
the host facts every tier asserts get provisioned. nightly's platform job used
the module spelling and carried a hand-rolled "Install cross targets" step to
compensate — a workaround for a defect one word wide (fixed, #397).

**4. CI lanes that do not walk the user's path.** phase-413 W5 converted
`build-wide`, `run-matrix`, `queue` and `host-tests`. The nightly zephyr jobs
still spell out six provisioning steps each, and cannot convert today because
they pass `just zephyr setup --skip-sdk` and the dispatcher has no argument
passthrough.

### What this cost, measured

Three consecutive host-tests runs, each ~40 minutes, each failing on a
different missing prerequisite that `just setup` did not provision:

1. cross Rust targets (`armv8r-none-eabihf`), pinned corrosion, fixture stamp
2. — same run after fixes: reached `check fast`, died on `clang-format`
3. the layer below that is what W5's gate now finds statically

Each layer only became visible once the one above it was fixed, because a
missing prerequisite fails the run at the first gate that needs it.

## Work items

### W1 — retire the duplicate producers — DONE

**Decided and landed.** Both are forwarders to `nros setup --tool <name>`. The
index path was verified working BEFORE anything was retired
(`present 0.6.1-nros1 (skip)`, `--check` -> `[OK]`).

Retiring them exposed the drift immediately: corrosion's two pins AGREED
(`CORROSION_VERSION` = the index's `upstream`), but play_launch_parser's did
NOT — a git SHA against `0.1.0-nros1` — so forwarding made `just doctor` call a
correctly installed tool MISSING. The doctor now asks the CLI rather than
comparing a second constant, and `PLAY_LAUNCH_PARSER_VERSION` is deleted.

Original text:

`install-corrosion` and `install-play-launch-parser` become forwarders to
`nros setup --tool <name>`, or are deleted and their callers changed. The
version constants `CORROSION_VERSION` / `PLAY_LAUNCH_PARSER_VERSION` in
`just/workspace.just` go with them: the index is the pin.

Care: `_setup-common` calls `just workspace install-corrosion` today (landed in
#365). Whichever spelling survives must still be reachable from every
`just setup <scope>`, or `check-preconditions-provisioned` fails — which is the
gate doing its job.

**Acceptance.** No tool has two installers. Each tool has exactly one pin.
`check-preconditions-provisioned` (#387) stays green.

### W2 — move the bespoke installers into the index

`clang-format`, `mdbook`, `verus` become `[tool.*]` entries with a pinned
version, a download, a checksum and a `smoke` command, like the other 16.
`scripts/setup-mdbook.sh`, `scripts/mdbook-checksums.txt` and
`scripts/setup-verus.sh` are deleted, not left as dead alternates.

**DECIDED (2026-09-05): split this item.**

The index ALREADY expresses per-host artifacts — `dist.linux-x86_64 = { url,
sha256 }`, as `[tool.qemu]` and `[tool.arm-none-eabi-gcc]` do. So the question
was never "can the index do this", it was "which of these three fit that
shape".

* **mdbook and verus fit it exactly** — plain release tarballs, no new
  machinery. `scripts/setup-verus.sh` additionally resolves "latest" through
  the GitHub releases API, a moving target the index replaces with a pin.
  DOING THIS NOW.
* **clang-format does not.** It is a pip wheel whose binary sits at
  `clang_format/data/bin/clang-format` inside a zip, so it needs the index to
  unzip and know an inner path — new machinery for exactly one consumer. It
  KEEPS its recipe as a documented exception until a second wheel-shaped tool
  appears. One exception with a stated reason beats index machinery serving one
  caller.

**Acceptance.** `scripts/setup-mdbook.sh` and `scripts/setup-verus.sh` no longer
exist as bespoke downloaders. Every remaining bespoke installer is listed in this doc
with a reason.

### W3 — retire the old spelling everywhere — workflow half DONE

**Landed.** `just setup <scope>` takes a variadic tail and forwards it, so
`just setup zephyr --skip-sdk` works and the exemption can go.

The subtlety, found by testing rather than assuming: `just` fills positional
parameters IN ORDER, so the flag binds to `tier`, not to the variadic tail. A
tier is never a flag, so it is re-homed — rather than making a user write
`just setup zephyr "" --skip-sdk`.

The workflow half landed with W4 below: no workflow uses the module spelling and
`check-workflow-setup-spelling`'s exemption list is empty.

Remaining: the prose sweep (book/, AGENTS.md, CLAUDE.md still teach the module
spelling). The gate reads `.github/workflows/` only, so prose is unratcheted.

Original text:

`check-workflow-setup-spelling` (#397) forbids `just <scope> setup` in
workflows and carries exactly one exemption: the nightly zephyr jobs, which pass
`--skip-sdk`. Give the `setup` dispatcher argument passthrough so the exemption
can be deleted.

Then sweep the non-workflow callers — docs, `book/src/`, `AGENTS.md`,
`CLAUDE.md` — so the documented spelling is the one that provisions correctly.
A reader who copies `just zephyr setup` out of the book gets the module recipe
and none of `_setup-common`.

**Acceptance.** The exemption list in `check-workflow-setup-spelling` is empty.
No prose in the repo teaches the module spelling.

### W4 — the nightly zephyr jobs walk the user path — DONE

**Landed.** All four module-spelling call sites in `nightly.yml` are the
dispatcher now — one in `zephyr-example-matrix`, two in
`zephyr-dual-line-summary` (3.7 and 4.4), one in `zephyr-copy-out`. Three jobs,
four sites; W3's text said "four zephyr jobs" and there are three. The
`NROS_ZEPHYR_VERSION=` prefixes are unchanged: they are shell env on the
command, and the dispatcher passes them through to the module recipe by
inheriting them.

The flag path was PROVEN, not assumed, because the re-homing rule is new. `just
--dry-run setup zephyr --skip-sdk` shows `just` binding `target=zephyr`,
`tier=--skip-sdk`, `extra=""`; running that interpolated script with a `just`
shim on `PATH` prints exactly two invocations, `[_setup-common]` then
`[zephyr] [setup] [--skip-sdk]`; and `just --dry-run zephyr setup --skip-sdk`
shows `ARGS="--skip-sdk"` reaching `./scripts/zephyr/setup.sh $ARGS`. The
no-flag and two-flag shapes were checked the same way.

`check-workflow-setup-spelling`'s `EXEMPT` is now empty, and both of its
directions were re-mutated after the change: reverting one conversion fails it,
and adding an exemption that matches nothing fails it.

**One step removed per job, and only one.** The "Provision Zephyr sources via
nros" step (`nros setup --source zenoh-pico --source cyclonedds-src --source
px4-rs`) is the first thing `just/zephyr-setup.just`'s `setup` does — with an
explicit `--index`, and before the `$WORKSPACE/zephyr` short-circuit, so it runs
on every invocation. 10/10/9 steps -> 9/9/8.

**What did NOT go, and why**, since the W5 table counts ~6 provisioning steps
per job and only one was redundant:

| step | kept because |
| --- | --- |
| `./.github/actions/setup-nros-cli` | Its own docstring records the decision: `_setup-common` does all three of its steps, but clones `play_launch` in FULL where the action uses `--depth 1`, and it is the only thing that puts the CLI on `$GITHUB_PATH`. A clone-cost change into a lane with no signal cannot be verified. |
| Register the baked Zephyr SDK for this HOME | `--skip-sdk` exists precisely so setup does NOT touch the SDK. Nothing in `_setup-common` or the module recipe registers a CMake package. |
| Reclaim disk (#0078) | Container housekeeping, not provisioning. |
| Unblock rustup clippy-preview conflict / `rustup set profile minimal` | Image workarounds that `rustup target add` needs — and `_setup-common` runs `just workspace rust-targets`, so the conversion makes them MORE load-bearing, not less. |
| Install clang + libclang for bindgen | An OS package. phase-422 W6 has `_setup-common` REPORT the system closure and never install it. |

**Acceptance.** Met for the zephyr jobs. Not yet swept for the whole
`.github/workflows/` tree — `host-tests` and `queue` are still W5's rows.

### W5 — the ratchet

Two gates landed already and are the model:

- `check-preconditions-provisioned` — every tier precondition is classified
  setup/build/manual, and `setup` ones are reachable from `_setup-common`.
- `check-workflow-setup-spelling` — workflows invoke the dispatcher form.

One more is missing, and it is what W1 needs to stay fixed: **a gate asserting
no indexed tool has a second installer.** Shape: for each `[tool.X]`, no
`install-X` / `setup-X` recipe may perform its own download or stamp
comparison; forwarding to `nros setup --tool X` is fine.

**Acceptance.** Reintroducing `install-corrosion`'s own stamp logic fails a
gate. Both directions checked — a gate row naming a tool the index dropped
fails too.

### W6 — the bootstrap actually pulls the system closure

Measured: 24 of the 46 `[prereq.*]` keys have no explicit consumer, nothing in
the repo INSTALLS via `--system`, and `just setup <scope>` never reaches it. On
a developer host that is 5 missing packages (`libgcrypt-dev`, `libpixman-dev`,
`make`, `ninja`, `openocd`) that no documented command installs.

The fix is NOT "run `--system --sudo` from setup": composing the command and
running it are deliberately separate (RFC-0062), and a provisioning verb that
sudo-installs behind the user's back is worse than the gap. What is missing is
that `just setup <scope>` never even PRINTS the closure, so the user is not
told. Make the scope verb report missing system prerequisites with the composed
command, the way `nros setup --system` already does.

**Acceptance.** `just setup <scope>` on a host missing a `role = package` or
`role = workspace` key names it and prints the install command. Nothing is
installed without an explicit `--sudo`.

### W7 — one board vocabulary

`board=` in a `package.xml` export and `[board.*]` in the index are two
namespaces that do not line up: of five boards declared across `examples/`,
only `threadx-linux` is an index key. `nros setup --workspace` works around it
by validating before printing a command, which is honest but is a workaround.

Same for `deploy=`: `threadx` is not a scope, it splits into `threadx_linux` /
`threadx_riscv64`.

**DECIDED (2026-09-05): additive first, unification deferred.** Add the missing
`[board.*]` entries so every exported board is ALSO an index key — no renames,
and it makes `nros setup <board>` work for all five instead of one. True
unification (picking one canonical spelling and renaming through 90+
package.xml files) stays open, because the index namespace is what USERS type
while the cmake namespace is what the BUILD uses, and neither is obviously the
one to keep. This is the two-vocabularies class this repo keeps paying for — the
`native`/`posix`/`linux` collapse, `[system.*]` vs `[prereq.*]`, the module vs
dispatcher setup spelling.

**Acceptance.** A gate asserts every `board=` in a package.xml export resolves
to an index board (or to a documented alias), and every `deploy=` resolves to a
scope.

### W8 — refuse a wrong-role dependency

Deferred by decision, not oversight. `<depend>qemu-system-arm</depend>` resolves
silently today; `nros setup --workspace` reports it as a category error without
failing. Turning it into a hard error breaks a working tree, so it needs a
deprecation window: report -> warn -> error.

**Acceptance.** A package.xml naming a `role = infra` key fails resolution, and
the error names the deploy target it should have come from instead.

## Non-goals

- **Not** replacing the CI image's baked tooling. The image is a cache; the
  index is the contract. `ci/docker/ci-base/Dockerfile` reading
  `config/rust-targets.txt` (#365) is the pattern — bake from the SSoT, do not
  hand-list.
- **Not** moving `setup-cli`, `setup-launch-resolve` or `setup-hooks` into the
  index. Those build in-tree code or configure the user's git; they are not
  third-party artifacts and have no version to pin.
- **Not** touching `[prereq.*]`. OS packages are RFC-0062's namespace and
  phase-413 W3 already gated workflows against restating them.
