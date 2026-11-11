# Phase 551 — The starter image

**Status (2026-11-11). IN PROGRESS: option B is ruled; W1, W2 and W7 are
unblocked, the rest wait on open questions.** Work for
[RFC-0127](../design/0127-starter-image.md), whose option B (D1's structure,
D4, D5's recipe) the user ruled on 2026-11-11; its other decisions are open.
Where an unblocked item needs an answer to an open question (Q2 the ROS
edition, Q3 the contents), it takes the RFC's recommendation as a PROVISIONAL
default, marked in code as changeable. Nothing is published to a registry and
no tag moves until Q1 and Q5 are ruled.
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

Gate: none. Option B ruled 2026-11-11; Q1 to Q6, Q8 and Q9 remain. Put those to the user; rewrite RFC-0127 from shape to
decisions; mark each item below unblocked or dropped.

Acceptance: RFC-0127 status `Stable`, or a ruling recorded per question.

### W1 — The generator becomes a shared library

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

Gate: none for the target (B). Its contents use RFC-0127's recommendations as
PROVISIONAL defaults until Q2, Q3 and Q8 are ruled. `host` plus the release `nros` through `scripts/install.sh`,
a baked `nros setup` for the Linux host, `mps2-an385-baremetal` and
`mps2-an385-freertos` with zenoh, a minimal Rust (host std plus
`thumbv7m-none-eabi`), and the template crates fetched at build time.
`NROS_HOME` and `/work` fixed (RFC-0127 D5 guarantee 3).

Acceptance: in the image with networking off (`--network none`), the book's
first project (`nros new` then build) and the Tutorial's FreeRTOS C talker
build and run under QEMU; the image's compressed size is printed and under
the D7 budget.

### W3 — The `starter-zephyr` target

Gate: Q4. `starter` plus the Zephyr SDK and the allowlisted workspace.

Acceptance: built on dispatch; a Zephyr native_sim talker builds offline in
it; compressed size printed and under 5 GB.

### W4 — Release job and tags

Gate: Q1, Q5. An image job in `release-nros.yml` after the asset; immutable
`<ver>` and `<ver>-zephyr`, a moving `latest`, and a retag recipe for a
semester or LTS tag.

Acceptance: a dispatch release publishes `<ver>`; a second publish of the
same `<ver>` is refused (immutability, RFC-0127 D5 guarantee 1); `latest`
does not move until W5 passes.

### W5 — The `image` probe track gates `latest`

Gate: Q9. A new probe track: a timed pull of the released tag on the probe
runner (with a throttled link once Q9 sets its speed), then the first project
built and run offline. Over the time or size budget is red.

Acceptance: the track is `probe=`-tagged, runs in `probe.yml`, prints the
pull time and the compressed size, and `latest` moves only on its green.

### W6 — Audience recipes

Gate: Q3. Documented in the book:

- newcomer: `docker run` then `nros new`;
- course: a `devcontainer.json` that names the image by digest (the tree has
  none today), with what to do when a seat disagrees (phase-523 W2's semester
  pin);
- air-gapped: `docker save` / `docker load`, and a `nros doctor` line that
  reports the running container's image digest (new: doctor reports only
  firmware image digests today).

Acceptance: each recipe is a probe-tagged book block that a probe runs.

### W7 — The customer `FROM` recipe

Gate: none beyond W2 (the recipe is ruled by B). Until Q1 names a registry the
recipe and its probe build `FROM` a locally built tag. A documented `FROM <starter>:<ver>` recipe adding a
board and a package, and the four guarantees of RFC-0127 D5 written where a
customer reads them.

Acceptance: a probe builds the recipe against each release; guarantee 2 is
checked by comparing the image's baked store against `nros setup --dry-run`
of the same release.

### W8 — arm64

Gate: Q6, and the runner decision `release-nros.yml` already waits on. One
matrix row for the asset and one platform for the image.

Acceptance: `starter` published for `linux/arm64`; W5 runs on an arm64
runner.

## Acceptance (phase)

A seat with only a container engine pulls one released tag and, offline,
builds and runs the book's first project and the current labs; the pull fits
the budget Q9 sets; a customer image built `FROM` it is probed every release.
