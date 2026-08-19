---
id: 704
title: "The PlatformIO bringup test runs `pio run`, which fetches its platform package from the network and blows the 60 s per-test budget — and PlatformIO is not a supported integration right now"
status: open
type: tech-debt
area: testing, integrations
related: [issue-0700, issue-0584]
---

## Symptom

The last real failure in tier 2, on a host where `pio` is installed:

```
TERMINATING [> 60.000s] nros-tests::cli_bringup_platformio platformio_zephyr_framework_2_component_bringup_builds
    TIMEOUT [  60.002s] (1501/1654)
```

Not an assertion failure — it hits the per-test limit to the millisecond.

**Measured after the fact, and it changes the shape:** with the package already
cached, the same test PASSES in **3.4 s**
(`NROS_ENABLE_PLATFORMIO=1 cargo nextest run --test cli_bringup_platformio`
-> `1 passed`). So this is a COLD-CACHE failure, not a permanent one: the first
run on any machine (or any CI container with a fresh PlatformIO home) pays a
package download that does not fit the budget, and every run after it is fast.
That is worse than a permanent red for a gate, not better — it passes locally
for whoever already ran it and fails for everyone else, which is how it reached
tier 2 unnoticed.

## Why

`cli_bringup_platformio.rs` asserts the adapter surface (repo-root
`library.json`, the `integrations/platformio/nros_codegen.py` pre-build hook)
and then does the expensive part:

```rust
let out = Command::new(&bin).args(["run", "-e", "native"]).current_dir(&pio_app).output()
```

`pio run` resolves and DOWNLOADS its platform package on first use. The test
already knows this can fail and carries offline markers
("Could not find the package", "PackageManagerError") to skip cleanly — but the
60 s nextest budget kills the process before that handling is ever reached, so a
condition the test was written to tolerate is reported as a hard timeout.

Selection makes it worse: `test-all`'s `env_exclude` drops this suite only when
NEITHER `pio` nor `platformio` is on PATH. Installing the CLI — which a
developer may do for unrelated reasons — opts the machine into a
network-dependent 60 s test in the per-change tier.

## Decision (2026-08-20): opt-in, because PlatformIO is not supported at present

PlatformIO is not a maintained integration right now. A test whose subject is
unsupported should not be able to red the per-change gate, and the alternatives
are worse:

* **Raise the timeout.** Would work on a warm cache and still fails cold or
  offline — it makes the gate depend on whether the machine has run this before,
  which is exactly the property that made this hard to see.
* **Deselect it silently** (extend `env_exclude` to drop it unconditionally).
  Cheap, but the suite would vanish with nothing in the run saying why — the
  "test that could not fail" shape this tree keeps removing.

So the suite is **opt-in**: it `skip!`s with a reason naming this issue unless
`NROS_ENABLE_PLATFORMIO=1` is set. The skip is VISIBLE in the run
(`[SKIPPED]`, counted in the skip budget), the reason says both facts — not
supported, and network-bound — and the escape hatch keeps it one env var away
for anyone working on the adapter.

The adapter-surface assertions above the `pio run` (library.json, the pre-build
script) are cheap and offline, and they go behind the same gate rather than
being split out: what they check is only meaningful if someone is maintaining
the integration, which is precisely what this issue records nobody is.

## What would close this

Either PlatformIO becomes a supported integration again — in which case the
`pio run` needs a fixture built in the BUILD stage rather than at run time
(CLAUDE.md: "No compilation inside tests"), which is the real reason it is slow
— or the integration and its test are deleted. "Present but untested and
opt-in" is a holding position, not a destination.
