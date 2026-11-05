---
id: 2284
title: "An sccache server started under a make jobserver keeps its tokens, and the make hangs at exit with zero children"
status: resolved
type: bug
area: [build, ci]
severity: high
related: [1403, 1773, phase-535]
found: 2026-11-05
resolved_in: "phase-535 W3 (the workspace-fixture fan-out PR): every sccache launch drops the jobserver"
---

**Home: [phase-535](../../roadmap/phase-535-example-reorg-and-cadence-ci.md)** — found while
measuring W3's fan-out of the workspace fixture builder.

## Measured

`just freertos build-fixtures` with the workspace builder fanned out by build
unit. The `freertos-posix` stage (two CycloneDDS units) built both rows, and
then its make sat in `pipe_read` for minutes with no child:

- `/proc/<make>/wchan` = `pipe_read`; utime and stime 0.
- The jobserver FIFO held **0** tokens (FIONREAD) of the 19 it started with.
- The only other process holding the FIFO open was the user's **sccache
  server**, started 5 minutes earlier, whose environment carried
  `MAKEFLAGS=… --jobserver-auth=fifo:/tmp/GMfifo<make pid>`.

Who started it: `ProvideCycloneDDS.cmake` routes Cyclone's C/C++ compiles
through sccache as `CMAKE_<LANG>_COMPILER_LAUNCHER`, and does so even with
`NROS_SCCACHE_REAL` cleared. The server had idled out since the previous
stage, so the first Cyclone compile in this stage started a new one, as a
daemon, with the compile's environment, and the server joined the pool.

## Reproduced in isolation

On a private port, so no running server is touched (make 4.4.1, sccache
0.8.2):

```make
all: a b
a:
	+@SCCACHE_SERVER_PORT=4299 sccache --start-server >/dev/null 2>&1; echo a-done
b:
	+@echo b-done
```

`make -j4 --jobserver-style=fifo` prints both lines and then never exits
(`timeout 30` → rc 124). The same file with
`env -u MAKEFLAGS -u MFLAGS -u CARGO_MAKEFLAGS` in front of `sccache` exits 0.
make collects every token back before it exits, and a daemon that took tokens
never returns them.

## Why it matters beyond this build

This is very likely issue 1403's cause: "make 4.4.1 blocked in `pipe_read` on
its own FIFO with zero children", seen in `threadx_linux build-examples` while
another build was running. That build used the same sccache, and its server
can idle out between stages like this one did. 1403 closed as *detected* (the
stall watchdog turns the hang into NO VERDICT after 600 s), not *fixed*. So
every fixture make had a chance to lose ten minutes and its verdict whenever
the shared server had to restart inside it.

It was rare only because the server usually already existed. The phase-535 W3
fan-out made it routine: it put the CycloneDDS rows of `freertos` and
`freertos-posix` under a make pool for the first time (before, each language
ran in a serial walk with no make).

## Fix

Every sccache launch drops the jobserver, by road:

- **C/C++ launchers.** `ProvideCycloneDDS.cmake` launches
  `env -u MAKEFLAGS -u MFLAGS -u CARGO_MAKEFLAGS <sccache>`. The Zephyr fixture
  leaves launch `scripts/bin/sccache-no-jobserver`, which does the same; a
  shell string carries `-D…` there, so the launcher has to be one word. A C
  compile has no use for the pool.
- **rustc (`scripts/bin/rustc-wrapper/sccache`).** rustc does use the pool, so
  the compile keeps it. Before the compile, the wrapper starts the server
  outside the pool if nothing is listening on the sccache port. The check is a
  bash socket open, so a running server costs one fork.

Verified with the reproduction above through both entry points (rc 0), and by
re-running the FreeRTOS fixture build that hung.

## Not done

- No gate. The class is "a daemon started inside a jobserver". sccache is the
  only daemon these builds start, and every launch of it now goes through one
  of the two spellings above. A gate would need sccache on the host to show
  anything, and the reproduction above is the cheap one.
- `--no-sccache` in `scripts/dev/measure-fixture-build.sh` clears the rustc
  wrapper only, and the CycloneDDS launcher still caches. The phase-535 W3
  numbers say so.
