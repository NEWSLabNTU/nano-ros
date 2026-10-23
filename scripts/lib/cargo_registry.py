"""Did a failed cargo run fail to REACH the registry? The ONE classifier.

Issue 1626 wrote this for `api-parity`'s `cargo doc`; issue 1992 found
`dep-chain`'s `cargo tree` needed the same answer, so it moved here and both
callers read it (Python by import, the shell probe through the CLI below).

Why an outer retry is needed at all, when the root `.cargo/config.toml` sets
`net.retry = 10`: both callers run cargo from the pinned NIGHTLY
(`nightly-2026-04-11`, cargo `eb94155a9`). That cargo's sparse-index fetch
wraps the curl error in `http_async::Error::Easy`, and its `maybe_spurious`
downcasts to a bare `curl::Error`, so an HTTP/2 framing error is never
classed as spurious and is never retried, whatever `net.retry` says. Cargo
fixed it upstream six days after the pin (`963787c759`, "add http_async types
to spurious error detection"); stable 1.99 has the fix. Until the nightly pin
moves past it, the retry that config promises is not there.

CLI, for shell callers:

    python3 scripts/lib/cargo_registry.py classify <stderr-file>
        exit 0 = a registry failure, 1 = not one, 2 = the classifier's own
        self-test failed (it runs on every invocation)
    python3 scripts/lib/cargo_registry.py attempts   # prints REGISTRY_ATTEMPTS
    python3 scripts/lib/cargo_registry.py backoff    # prints REGISTRY_BACKOFF_S
    python3 scripts/lib/cargo_registry.py --self-test
"""

from __future__ import annotations

import re
import sys

# How many cargo processes a caller runs before it calls a registry failure
# final, and the pause (times the attempt number) between them.
REGISTRY_ATTEMPTS = 3
REGISTRY_BACKOFF_S = 20

# What cargo prints when it could not reach a registry or download a crate, as
# opposed to rejecting the crate. Each pattern is one measured shape: the four
# sightings in issue 1626 (an HTTP/2 framing error, a connect timeout, a refused
# connection) plus a DNS failure. Matched against cargo's stderr only when the
# exit status is already non-zero, so a pattern can only change WHICH error is
# raised, never whether one is.
_REGISTRY_FAILURE = re.compile(
    r"unable to update registry"
    r"|failed to download from `https?://"
    r"|download of \S+ failed"
    r"|\[\d+\] (?:Could not resolve host|Could not connect|Error in the HTTP2"
    r"|Timeout was reached|SSL connect error|Failure when receiving|Send failure|Recv failure)"
    r"|spurious network error"
)


def is_registry_failure(stderr: str | None) -> bool:
    """True when a failed cargo run failed to REACH the registry.

    A compile error, a resolver conflict or a missing feature is not one, and
    must keep reading as the failure it is.
    """
    return bool(_REGISTRY_FAILURE.search(stderr or ""))


# Measured stderr shapes. The first is issue 1992's dep-chain run
# (118565369353) verbatim, less its dependency-path lines.
_NETWORK = [
    ("http2 framing (dep-chain, issue 1992)",
     ("    Updating crates.io index\n"
      "error: failed to get `hash32` as a dependency of package `heapless v0.8.0`\n"
      "Caused by:\n  failed to load source for dependency `hash32`\n"
      "Caused by:\n  unable to update registry `crates-io`\n"
      "Caused by:\n  download of ha/sh/hash32 failed\n"
      "Caused by:\n  curl failed\n"
      "Caused by:\n  [16] Error in the HTTP2 framing layer\n")),
    ("connect timeout",
     ("  download of pa/st/pastey failed\n"
      "  [28] Timeout was reached (Connection timed out after 30002 milliseconds)\n")),
    ("refused",
     ("error: failed to download from `https://static.crates.io/crates/minijinja/2.21.0/download`\n"
      "Caused by:\n  [7] Could not connect to server\n")),
    ("dns", "Caused by:\n  [6] Could not resolve host: index.crates.io\n"),
]
_NOT_NETWORK = [
    ("compile error", "error[E0425]: cannot find value `x` in this scope\n"),
    ("resolver", "error: failed to select a version for `serde`.\n"),
    ("missing feature", "error: Package `nros v0.6.0` does not have the feature `nope`\n"),
    ("unknown package", "error: no matching package named `nros` found\n"),
    ("empty", ""),
]


def _self_test(quiet: bool = False) -> int:
    bad = [n for n, e in _NETWORK if not is_registry_failure(e)]
    bad += [n for n, e in _NOT_NETWORK if is_registry_failure(e)]
    for n in bad:
        print(f"cargo_registry self-test: misclassified: {n}", file=sys.stderr)
    if bad:
        return 2
    if not quiet:
        print(f"cargo_registry self-test: OK ({len(_NETWORK)} network, "
              f"{len(_NOT_NETWORK)} not)")
    return 0


def main(argv: list[str]) -> int:
    if argv == ["--self-test"]:
        return _self_test()
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment. Exit 2 is neither answer `classify` gives, and the
    # probe reads `attempts` under a check that stops it on a non-zero exit.
    if _self_test(quiet=True):
        return 2
    if argv == ["attempts"]:
        print(REGISTRY_ATTEMPTS)
        return 0
    if argv == ["backoff"]:
        print(REGISTRY_BACKOFF_S)
        return 0
    if len(argv) == 2 and argv[0] == "classify":
        with open(argv[1], encoding="utf-8", errors="replace") as fh:
            return 0 if is_registry_failure(fh.read()) else 1
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
