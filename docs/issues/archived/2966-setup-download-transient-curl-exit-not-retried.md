---
id: 2966
title: "`nros setup` did not retry a download curl reported as a broken transfer in words its list did not know, and restarted a cut-off download from zero"
status: resolved
type: bug
severity: high
area: [cli, setup, ci]
related: [2129, 2369, phase-590]
found: 2026-11-25
resolved: 2026-11-25
---

## What failed

The first release nightly at 939ac840b2 (run 45533246866, about 02:39Z) lost
three jobs (`zephyr 4.4 / rust/talker`, `nuttx make road (riscv)`,
`NuttxRiscv`) inside `nros setup`, fetching `xrce-agent-2.4.3-nros1`,
`arm-none-eabi-gcc-13.2-nros5` and `qemu-11.0.0-nros8` from the
nano-ros-sdk GitHub releases. Each failed once and stood:

    curl: (56) Connection died, tried 5 times before giving up
    [FAILED]  arm-none-eabi-gcc — install 13.2-nros5: download https://github.com/NEWSLabNTU/nano-ros-sdk/releases/download/...

The re-dispatch, 45535612375, was green on the same commit.

## Cause

Every store fetch already runs through one retrying runner,
`run_fetch` in `sdk_store.rs` (issue 2129 for a network failure, issue 2369
for a server outage). It decides whether a failure is transient by matching
curl's MESSAGE against a list. `Connection died, tried 5 times before giving
up` is HTTP/2's wording of a receive failure (exit 56), and the list knew
only HTTP/1's `Recv failure: Connection reset by peer`. So the failure read
as permanent and was not retried. `transfer closed with N bytes remaining to
read` (exit 18, a server that closes short of Content-Length) was not in the
list either; measured against a local server that does exactly that.

And a retry, when one did happen, started the download over, so a large
asset cut off near its end paid for all of it again.

## Fix

- `transient_failure` also reads curl's EXIT CODE from its `curl: (N)` line,
  which is curl's stable contract where the text is not. Network exits 5, 6,
  7, 16, 18, 28, 35, 52, 55, 56 and 92 get the network budget (3 attempts,
  5 s then 10 s). 22 stays classified by its HTTP status, as before.
- `download` resumes: every attempt passes `-C -`, so a retry asks for the
  bytes after the partial file. A partial left by an EARLIER run is removed
  first, since nothing says it came from this URL. A server that cannot
  resume (exit 33, or HTTP 416) gets one fresh download.
- Callers still verify the sha256 after the download (`verify_sha256`, at
  both store call sites and for `rustup-init`), so a resume can never install
  a spliced file.
- `rustup-init`'s download, the one store fetch with its own curl line, goes
  through `download_with` (it keeps `--proto =https --tlsv1.2`).

Not changed: the SDK index fetch (`cmd/setup.rs::fetch_index`) keeps its own
fail-fast curl (10 s to connect, 60 s in all), on purpose: it runs before any
provisioning step, and an offline host must fail in seconds there. Git
sources already go through `run_fetch` and so get the new classification.

Sweep: `git grep -n '"curl"' -- packages/cli/nros-cli-core/src`.

## Test

`fetch_retry_tests` in `sdk_store.rs`, against a local server that sends the
headers and 70,000 of 200,000 bytes and then drops:

- with a reset (curl 56): the download succeeds, byte for byte, and the
  second request carries `Range: bytes=70000-`;
- with a clean close short of the length (curl 18): the same;
- a server that ignores the Range (curl 33): one fresh download, not a
  failure and not a spliced file;
- a partial from an earlier run is not resumed;
- curl's network exits classify as transient whatever their wording; 22
  (404), 33 and 3 do not.

Negative controls: with the exit-code classification removed, 4 of the 16
tests fail; with `-C -` removed, 3 fail.
