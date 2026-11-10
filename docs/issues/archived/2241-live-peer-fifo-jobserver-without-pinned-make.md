---
id: 2241
title: "live-peer: the fixture build hands out a FIFO jobserver on a runner with no pinned make, so the system make 4.3 children stop"
status: resolved
resolved: 2026-11-10
resolved_in: "fix 2: the jobserver form is chosen from every make the build runs, cached ones included"
type: bug
area: [build, ci, testing]
severity: high
found: 2026-11-04
related: [phase-466, 2058, 2119]
---

> **Resolved (2026-11-10).** Fix 2 landed in two steps. Issue 2219's fix
> (2026-11-04) chose the form from the make on PATH for every driver, which
> closes the first two sightings. This fix closes the third: `nros build`
> also reads the `CMakeCache.txt` of every build dir its plans name (and each
> immediate subdir, for sysbuild domains), takes the makes recorded under
> `CMAKE_MAKE_PROGRAM` and `MAKE`, and creates a FIFO pool only if the oldest
> make among them is 4.4 or newer. Otherwise it creates a pipe pool and says
> which make forced that.

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — homed 2026-11-04: the scheduled `live-peer` lane stops in its fixture build, before any cell.

## Measured

The scheduled `live-peer regression` run on main, 41771578704 (2026-11-04,
head `c23ed3b659`), failed both fixture jobs at the step **Build the fixtures
those rows resolve**. Both stop on the same line:

- Job 126382246956 (rows whose board IS this runner, runner `GitHub Actions
  1000128960`): `cyclonedds-sys`'s nested CMake build, under
  `examples/workspaces/bridge-cyclonedds/target-fixtures/…`. The build script
  warns first:

      cyclonedds-sys: a FIFO jobserver is inherited (`--jobserver-auth=fifo:`) and the pinned make is not provisioned, so CMake uses the make on PATH
      gmake: *** internal error: invalid --jobserver-auth string 'fifo:/tmp/nros-jobserver-224273-0/fifo'.  Stop.

- Job 126382800895 (rows whose board is NOT this runner): the Zephyr
  native_sim link of `build-ws-rs-qos-entry-zenoh`. Zephyr runs its
  native_simulator Makefile through `/usr/bin/make`:

      make[1]: *** internal error: invalid --jobserver-auth string 'fifo:/tmp/nros-jobserver-255351-0/fifo'.  Stop.

The pool is `nros`'s own jobserver
(`packages/cli/nros-cli-core/src/builder/jobserver.rs`, which names the FIFO
`nros-jobserver-<pid>-<seq>`). GNU make 4.3, the Ubuntu 22.04 system make,
cannot join a FIFO jobserver; 4.4 can.

## Second sighting: the book's own setup flow (2026-11-04)

The same night, both book probes stopped on the same line, in a pristine
container that follows the book's steps:

- Nightly run 41790118101, job 126441270473 (`bootstrap-probe`, step `Run
  bootstrap probe`).
- Probe run 41797511182, job 126464994275 (`book probe — checkout track`,
  `just probe checkout`).

    cyclonedds-sys: a FIFO jobserver is inherited (`--jobserver-auth=fifo:`) and the pinned make is not provisioned, so CMake uses the make on PATH
    gmake: *** internal error: invalid --jobserver-auth string 'fifo:/tmp/nros-jobserver-37226-0/fifo'.  Stop.

This happens under `/tmp/probe_quickstart_rs/build/posix-cyclonedds/linux_entry`.
That is the reader's path, not a CI lane: a user who follows the quickstart
on a host whose make is 4.3 (Ubuntu 22.04) reaches the same stop. So the
second fix below is no longer only a CI convenience. The book does not tell
the reader to provision `make`, so the first fix alone leaves the reader's
path broken. The `bootstrap-probe` job passed on 2026-11-02 and 2026-11-03.

## Third sighting: the pinned make IS on PATH, and the build dir runs another (2026-11-07)

This is the same stop on a host that HAS the pinned make. `nros build` of a
scaffolded Zephyr Rust workspace on native_sim compiled the Rust lane, then
died at the native simulator link:

    cd …/rs_node_bringup__zephyr_west/zephyr && /usr/bin/make -f …/scripts/native_simulator/Makefile all …
    make: *** internal error: invalid --jobserver-auth string 'fifo:/tmp/nros-jobserver-2594758-438158719-0/fifo'.  Stop.

`make` on PATH was `~/.nros/sdk/make/4.4.1/bin/make`, so `jobserver.rs`
chose the FIFO form. The build dir, however, had been configured once before
that make was first on PATH, and its `CMakeCache.txt` held
`MAKE:FILEPATH=/usr/bin/make` (GNU Make 4.3). That cached make is the one
Zephyr's native simulator step runs. A fresh build dir of the same image
cached `MAKE:FILEPATH=/home/aeon/.nros/sdk/make/4.4.1/bin/make` and linked;
`nros run` then matched.

The class: `jobserver.rs` asks the make on PATH which form to hand out, but
a configured build dir runs the make its cache recorded
(`CMAKE_MAKE_PROGRAM`, or a `find_program` result such as `MAKE`). This
fits fix 2 below, keyed on the make the build will actually run: read the
cache of a build dir that has one, and use the pipe form if any make it
names is older than 4.4. Fix 1 does not cover it, because the pin was
provisioned here.

Evidence, kept unmodified: the stale build dir at
`/tmp/claude-1000005/-home-aeon-repos-nano-ros/dc771fa6-8a06-42be-acf2-5de5cbc99c73/scratchpad/agent-a2000a-repro/agent-a2000a-stale-make-west-dir`
(host newslab-241; a scratch path, so it does not outlive the session).

## What it is not

- **Not issue 2058's build-script half.** That fix is working:
  `cyclonedds-sys` names the pinned make when one exists, and here it says
  plainly that none is provisioned. The second failure is not a build script
  at all. It is Zephyr's own `make` call, which nothing in our tree chooses.
- **Not issue 2119's PATH half.** No pinned make exists on this runner to put
  first on `PATH`.
- **Not new code in the failing rows.** The lane was green on 2026-11-01. On
  2026-11-02 and 2026-11-03 it stopped earlier on the board-facts panic
  (issues 2224/2229), so this step was not reached. Whether it was also broken
  on those days is unknown.

## What would close it

Either half is defensible, and that choice is why this is filed rather than
patched:

1. **Provision the tool:** `nros setup --tool make` in `live-peer.yml`
   before the fixture build, for both jobs. This matches what the fixture
   drivers assume elsewhere.
2. **Make the pool safe without it:** `builder/jobserver.rs` hands out a FIFO
   jobserver only when every make the build can reach is new enough. With no
   pinned make, it falls back to the pipe style, which make 4.3 understands, or
   to no pool. That would close the class for any lane missing the pin, not
   just this one.

Acceptance: a `live-peer regression` run gets past "Build the fixtures those
rows resolve" in both jobs.

## Fix (2026-11-10)

- `builder/jobserver.rs`: `makes_in_cache` parses a cache's
  `CMAKE_MAKE_PROGRAM` / `MAKE` entries and skips Ninja and `*-NOTFOUND`.
  `cached_makes(dir)` reads `dir/CMakeCache.txt` and each immediate subdir's.
- `cmd/build.rs`: `oldest_reachable_make` collects each plan's record build
  dirs, its cmake build dir and its output build dir (cmake or west). The
  oldest of those makes and the make on PATH goes to `choose_style`.

Where live-peer stands: run 42754129258 (2026-11-10) has no
`invalid --jobserver-auth` line in either job. Its remaining red,
`build-ws-rs-qos-entry-zenoh`, is issue 2415, not this one.

Known reach not covered, filed as [issue 2421](../2421-pool-fan-out-cached-make-below-fifo.md): `scripts/build/jobserver-pool.sh`, the `just`
fan-outs' pool, is created by the pinned make 4.4 itself. A child there
running `cmake --build` on a dir whose cache recorded make 4.3 would hit the
same stop. No sighting so far. Re-configuring such a dir (`cmake <dir>`)
does not refresh a cached `MAKE:FILEPATH`; deleting that one cache entry
does.

Sweep: `git grep -n 'choose_style\|tool_version("make")\|jobserver-auth=fifo' -- packages scripts just`
