---
id: 1266
title: "`nros setup` downloads, verifies and unpacks one package strictly in
  series, and the download is silent — a 1.4 GB fetch is 15 minutes of a log
  that looks hung"
status: resolved
type: tech-debt
area: cli, build
severity: medium
found: 2026-09-10
resolved: 2026-09-11
related: [issue-1267, issue-1273, issue-1274, issue-1275, issue-0374, issue-0385, rfc-0014, rfc-0099, phase-447]
---

## What this is

`orchestration/sdk_store.rs::execute_install`, the `InstallAction::Prebuilt`
arm, is three blocking steps in a row:

```rust
sh(&["curl", "-L", "--fail", "--silent", "--show-error", "-o", &archive…, url])?;
verify_sha256(&archive, sha256)?;
sh(&["tar", "-xf", &archive…, "-C", &prefix…])?;   // or the dist's own `install`
```

Network, then CPU, then disk — and nothing overlaps. While `tar` decompresses
one package the link is idle; while `curl` runs the CPU is idle. The loop in
`cmd/setup.rs:342` then starts the next package's `curl` only after this
package's `tar` has finished, so the whole provisioning run alternates between
using the network and using the machine, never both.

## The silence is the part that gets reported first

`curl` is invoked `--silent --show-error`. That is the right pair for a short
fetch and wrong for a long one: no progress, no byte count, no rate.

Measured on a contained-runner bootstrap, 2026-09-10:

```
21:49:55   nros setup --tool zephyr-sdk: prebuilt 0.16.8 (dist linux-x86_64)
                → /home/runner/src/nano-ros/scripts/zephyr/sdk
   …15 minutes with no further output at all…
```

The store was growing the whole time (~0.8 MB/s, checked with `du`), so nothing
was wrong. But the only way to learn that was to `du` the store from outside the
process. An operator watching the log sees a tool that stopped, and the honest
reading of a 15-minute gap is "this is wedged", which is how a healthy
provisioning run gets killed and restarted.

## Fix

Two independent pieces; either is worth having alone.

**Progress.** Drop `--silent` for a fetch whose declared size is large, or use
`--progress-bar` / `-w` to emit a periodic line. The constraint is that the
output must stay readable when it is not a TTY — CI logs and
`tmp/runner-bootstrap.log` are the normal case here, and a carriage-return
progress bar renders in them as one very long line. A byte-count line every N
seconds survives both.

**Overlap.** Fetch package *n+1* while package *n* verifies and unpacks. That is
a one-deep pipeline, not general concurrency: it needs no new policy about the
store layout, the lock file, or ordering, because only one package is ever being
INSTALLED. Issue 1267 is the larger change; this one is cheap and independent of
it.

`plan_install` is already pure (`Pure — does no I/O beyond reading the
provenance marker`) and issue 0374 already added a pre-pass that resolves the
whole plan before the first fetch. The separation a pipeline needs therefore
exists; what is missing is a fetch that can be started early.

## What NOT to do

Do not reach for a download accelerator here on the strength of `aria2` being in
the index. That entry is declared for `west sdk install` — Zephyr's own tool
uses it — and is not a claim about our fetch path. Adding a second downloader
beside `curl` is a second producer for one job, which is the shape
`check-one-producer-per-tool` exists to refuse.

## Where this was measured

A contained self-hosted runner bootstrap on this workstation: 24 `nros setup`
invocations across 11 distinct tools, plus the Zephyr SDK, plus source builds
for `play_launch_parser` and `espflash`. The pre-Zephyr steps ran mostly warm
(the store already held them) and the Zephyr SDK fetch was cold.

## Resolution (2026-09-11, phase-447 E3 / RFC-0099 D7)

Both halves, as the fix section asked, with the overlap taken further than the
one-deep pipeline proposed: the session plan (phase-447 E2) is executed by a
bounded worker pool, so fetch, verify and unpack of DIFFERENT packages overlap —
package *n+1* downloads while package *n* verifies and unpacks. Within one
package they stay serial, which they must: the sha256 needs the whole archive.

**Progress.** The two `curl` copies in `sdk_store.rs` (prebuilt dist, source
tarball) are one helper, `download`. curl stays `--silent --show-error` — its
own bar is a carriage-return repaint, one very long line in a CI log — and
`orchestration/step_log.rs::watch_download` watches the archive's size instead:
nothing for the first 10 s (so a short fetch prints nothing at all), then one
plain line every 30 s, `… downloading <file>: 412.3 MB in 8m35s (0.80 MB/s)`,
and a closing `fetched` line if it ever spoke. `cmd/setup.rs::fetch_index`
stays silent on purpose: it fetches a small TOML with fail-fast timeouts.

The policy is a pure function of (elapsed, last line), tested at synthetic
instants; the watcher is tested by handshake, never by sleeping.

The "What NOT to do" held: no second downloader, `aria2` untouched.
