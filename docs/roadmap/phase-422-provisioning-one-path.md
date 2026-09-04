# phase-422 — provisioning has one path, and CI walks it

**Status (2026-09-04). Open. W1–W3 are mechanical and independent; W4 needs a
small dispatcher change first; W5 is the ratchet that keeps the rest from
regrowing. Two gates already landed as part of phase-413 and are listed here as
prior art, not as work: `check-preconditions-provisioned` (#387) and
`check-workflow-setup-spelling` (#397).**

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

### W1 — retire the duplicate producers

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

Care: `clang-format` is provisioned from a **pip wheel** chosen for the host
platform, which is not the tarball shape the index uses. Either the index grows
a wheel source kind, or clang-format keeps its recipe and is documented as a
deliberate exception with the reason. Decide before implementing — do not force
a shape that then needs a second exception.

**Acceptance.** `scripts/setup-mdbook.sh` and `scripts/setup-verus.sh` no longer
exist as bespoke downloaders. Every remaining bespoke installer is listed in this doc
with a reason.

### W3 — retire the old spelling everywhere

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

### W4 — the nightly zephyr jobs walk the user path

Blocked on W3's dispatcher change. Once `just setup zephyr --skip-sdk` exists,
the four zephyr jobs collapse from ~6 provisioning steps each to the scope verb
plus one command, matching every other lane.

**Acceptance.** Every job body in `.github/workflows/` is
`just setup <scope>` plus one command, or is listed here with a reason.

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

Decide which namespace is canonical, map the other onto it, and gate that they
agree. This is the two-vocabularies class this repo keeps paying for — the
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
