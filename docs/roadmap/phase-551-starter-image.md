# Phase 551 — The starter image

**Status (2026-11-12). IN PROGRESS: every RFC-0127 question is ruled, so
every item is unblocked.** Work for
[RFC-0127](../design/0127-starter-image.md): option B (D1's structure, D4,
D5's recipe) ruled 2026-11-11, and every remaining question on 2026-11-12
("Take your advice on RFC-0127"), each recommendation taken. W0, W1, W2 and W7
are done. The first push to `ghcr.io/newslabntu/nano-ros-starter` and the
first tag go through the release flow (W4), never by hand from a session.
Supersedes [phase-523](phase-523-education-labs-adoption-probes.md) W2 (the
instructor image and the semester pin): the instructor image is now one
audience of a general starter image. Each item names the RFC-0127 question
that gates it ("Gate:"); an item gated on an unruled question does not start.
Each item is claimed with `just claim phase-551-wN` before it starts.

---

## Why

Phase-523 W2 asked for an instructor image so a lab seat installs by a pull.
The user widened it to newcomers, people who want a working setup, educators
and customers, with one base and only one or a few images. The tree has three
image sources and none for a user (RFC-0127 Problem): a generated runner
image that deliberately bakes no store, and two hand-written CI images.

## Work items

### W0 — Rule RFC-0127

Status: DONE (2026-11-12). Option B ruled 2026-11-11, the rest 2026-11-12;
RFC-0127 rewritten from shape to decisions with a ruling recorded per
question, D7's budget derived from Q9's link, and the PROVISIONAL marks
dropped from the generator.

Gate: none.

Acceptance: RFC-0127 status `Stable`, or a ruling recorded per question.

### W1 — The generator becomes a shared library

Status: DONE (349dd028da). `scripts/ci/lib/image-context.sh`; the runner's
context is held by `check-runner-context-golden`.

Gate: none (Q7 ruled by B). Split `scripts/ci/runner-container.sh` into a library that writes
one multi-stage Dockerfile from `nros-sdk-index.toml` (targets `host`,
`starter`, `starter-zephyr`) and the runner's own front end, which keeps its
`--check` / `--build` / `--run` and its no-baked-store rule.

Acceptance: the runner's build context (its Dockerfile, `entrypoint.sh` and
every copied file) is byte-identical to what `runner-container.sh` generates
today, held by a gate that diffs it against a committed golden; the
`--build/--run/--attach/--sweep` flags, the volume list, the user and
capabilities and the layer order are unchanged; `runner-doctor.sh` passes in
the image.

### W2 — The `starter` target

Status: built and accepted LOCALLY (2026-11-11) against release `0.6.0-nros8`,
with `scripts/ci/starter-image.sh`: 1.22 GB compressed (budget 2 GB), 4.3 GB
raw. The image's last step rebuilds the Linux Rust and FreeRTOS C first
projects with networking removed, so a finished build has shown the offline
claim. Under `--network none` the FreeRTOS talker publishes in QEMU with a
router started by hand. On that release `nros run`'s automatic router starts
nothing for it; on main the run plan carries the link, so this waits only on
the next release.

Gate: none (B; contents Q2, Q3 and Q8, ruled 2026-11-12). `host` plus the release `nros` through `scripts/install.sh`,
a baked `nros setup` for the Linux host, `mps2-an385-baremetal` and
`mps2-an385-freertos` with zenoh, a minimal Rust (host std plus
`thumbv7m-none-eabi`), and the template crates fetched at build time.
`NROS_HOME` and `/work` fixed (RFC-0127 D5 guarantee 3).

Acceptance: in the image with networking off (`--network none`), the book's
first project (`nros new` then build) and the Tutorial's FreeRTOS C talker
build and run under QEMU; the image's compressed size is printed and under
the D7 budget.

Found on the way: [issue 2444](../issues/2444-build-offline-flag-misses-metadata-probe.md)
— offline, the first project builds only with `NROS_OFFLINE=1`; `nros build
--offline` still resolves crates.io in the metadata probe.
[Issue 2445](../issues/archived/2445-installed-release-tree-written-by-builds.md)
— builds wrote ~4.3 GB of probe caches into the installed release tree, and a
read-only install failed to build; resolved after `0.6.0-nros8`, so the starter
still removes the caches by hand until it is built from a release with the fix.

### W3 — The `starter-zephyr` target

Gate: Q4, ruled 2026-11-12: on dispatch. `starter` plus the Zephyr SDK and the allowlisted workspace.

Acceptance: built on dispatch; a Zephyr native_sim talker builds offline in
it; compressed size printed and under 5 GB.

### W4 — Release job and tags

Gate: Q1, Q5, ruled 2026-11-12 (`ghcr.io/newslabntu/nano-ros-starter`,
public, every release). An image job in `release-nros.yml` after the asset; immutable
`<ver>` and `<ver>-zephyr`, a moving `latest`, and a retag recipe for a
semester or LTS tag.

Acceptance: a dispatch release publishes `<ver>`; a second publish of the
same `<ver>` is refused (immutability, RFC-0127 D5 guarantee 1); `latest`
does not move until W5 passes.

### W5 — The `image` probe track gates `latest`

Gate: Q9, ruled 2026-11-12 (100 Mbit/s shared by 30 seats; the size is the
gate and the room's pull time is printed, RFC-0127 D7). A new probe track: a timed pull of the released tag on the probe
runner (with a throttled link once Q9 sets its speed), then the first project
built and run offline. Over the time or size budget is red.

Acceptance: the track is `probe=`-tagged, runs in `probe.yml`, prints the
pull time and the compressed size, and `latest` moves only on its green.

### W6 — Audience recipes

Gate: Q3, ruled 2026-11-12. Documented in the book:

- newcomer: `docker run` then `nros new`;
- course: a `devcontainer.json` that names the image by digest (the tree has
  none today), with what to do when a seat disagrees (phase-523 W2's semester
  pin);
- air-gapped: `docker save` / `docker load`, and a `nros doctor` line that
  reports the running container's image digest (new: doctor reports only
  firmware image digests today).

Acceptance: each recipe is a probe-tagged book block that a probe runs.

### W7 — The customer `FROM` recipe

Status: the recipe is `docker/starter-recipe/Dockerfile`, which adds a board
and a ROS 2 package (`nav_msgs`, which the starter does not ship; the probe
refuses a package the starter already has, since installing it proves
nothing). The recipe lives outside `ci/docker/`, because it is a customer
image, not a CI image under the shared apt closure. Its probe is `just probe starter-recipe`
(`scripts/probe/verify-starter-recipe.sh`), run locally FROM a locally built
starter. Guarantee 2 is checked with a REAL offline `nros setup` per board, not
with `--dry-run`: a dry run on releases up to `0.6.0-nros8` calls present
sources absent ([issue 2447](../issues/archived/2447-setup-dry-run-reports-present-sources-as-planned.md)).
The boards come from the image's own labels. The four guarantees are written
in the recipe's header. Their book page is W6's, and names the image Q1 ruled
(`ghcr.io/newslabntu/nano-ros-starter`); its one-command run needs the first
release that publishes it.

Gate: none beyond W2 (the recipe is ruled by B). Until the first release
publishes the image, the recipe and its probe build `FROM` a locally built tag. A documented `FROM <starter>:<ver>` recipe adding a
board and a package, and the four guarantees of RFC-0127 D5 written where a
customer reads them.

Acceptance: a probe builds the recipe against each release; guarantee 2 is
checked by comparing the image's baked store against `nros setup --dry-run`
of the same release.

### W8 — arm64

Gate: Q6, ruled 2026-11-12 (amd64 first, arm64 next), and the runner decision `release-nros.yml` already waits on. One
matrix row for the asset and one platform for the image.

Acceptance: `starter` published for `linux/arm64`; W5 runs on an arm64
runner.

## Acceptance (phase)

A seat with only a container engine pulls one released tag and, offline,
builds and runs the book's first project and the current labs; the pull fits
the budget Q9 sets; a customer image built `FROM` it is probed every release.
