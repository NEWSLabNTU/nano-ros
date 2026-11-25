---
id: 2974
title: "The download-reset test asserted the resume offset equals the reset point, which a RST does not guarantee"
status: resolved
type: bug
severity: low
area: [cli, setup, tests]
related: [2966]
found: 2026-11-25
resolved: 2026-11-25
---

## What failed

Hosted CI on PR #4862 (a params change that touches no download code) went
red in the `cli-tests + node-std-tests + launch-resolve-fresh` lane:

```
orchestration::sdk_store::fetch_retry_tests::a_download_reset_mid_stream_resumes_where_it_stopped
  assertion `left == right` failed
  left: [None, Some(65536)]
 right: [None, Some(70000)]
```

The test came in with issue 2966's fix (d78edc7ca5).

## Why

The test's server sends 70,000 bytes of a 200,000-byte body, then closes the
socket with the request still unread, which Linux turns into a RST (curl exit
56). A RST discards whatever the client had not yet read off the socket, so
the partial file holds however much curl had drained when the reset landed:
anywhere up to 70,000 bytes. `curl -C -` then resumes from the partial's
length. The test asserted that the resume Range was exactly 70,000. On CI curl
had 65,536 on #4862 and 65,513 on #4846: not a whole buffer, so any count can
be lost. The code was right and the assertion was about timing.

Reproduced here: the unfixed test, 8 copies at a time, failed 4 of 520 runs,
each with exactly `[None, Some(65536)]`.

## Fix

The precondition is made deterministic, and the assertion states what holds
whatever the timing.

- **Precondition.** The server sends its 70,000 bytes, then holds the reset
  until the client's partial file holds data. That is an observed condition
  on curl's own write, not a sleep; a 30 s deadline only bounds a hang. What
  curl has written it has read, so the reset can no longer discard every byte,
  and there is always something to resume.
- **Assertion.**
  - One reset, one retry, and the first request asks for the whole body.
  - The retry sends a Range (it resumed, rather than restarting).
  - That Range is in `1..=70_000`: past zero, and no further than was sent.
  - The finished file is the body byte for byte. The server answers
    `bytes=from-` with `body[from..]`, so an identical file proves `from` was
    exactly the partial's length, i.e. the bytes actually written.

The siblings were checked for the same assumption with
`git grep -n '70_000' -- packages/cli/nros-cli-core/src/orchestration/sdk_store.rs`.
`a_download_closed_short_resumes_where_it_stopped` and
`a_server_that_cannot_resume_gets_a_fresh_download` keep their exact
`Some(70_000)`. Their server consumes the request before closing, so the close
is a FIN, and TCP delivers every byte ahead of a FIN. That offset is exact by
construction, and the clean-close test pins the resume point.

Measured under the same 8-way load after the fix: the reset test passed
320/320, and all 16 fetch-retry tests together passed 320/320.
