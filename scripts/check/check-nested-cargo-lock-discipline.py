#!/usr/bin/env python3
"""A nested cargo that bypasses the `--locked` PATH shim must say what it does
to the lockfile.

THE CLASS
---------
`Cargo.lock` is a promise that someone else's build resolves what yours did
(issues 0359/0378), and `--locked` is what makes a mismatch fail instead of
silently rewriting the file. The project injects it ONCE, in the
`scripts/bin/cargo` PATH shim, precisely because a per-call-site flag would
miss the callers that matter — cmake and corrosion invoke `cargo` by NAME.

A build script is the one caller the shim cannot reach. Inside a build script
cargo exports `CARGO` as the path of the REAL binary, and the shim `exec`s that
same binary, so `Command::new(env::var_os("CARGO"))` runs cargo with the shim
skipped and no `--locked`. Issue 1307 is what that costs: the `nros-sizes-build`
size probe injects `--config patch.crates-io.libc.path=…` for NuttX targets, the
root lock pins `libc` at a version the patched copy cannot match, and so EVERY
NuttX build appended

    [[patch.unused]]
    name = "libc"
    version = "0.2.183"

to the repo's root `Cargo.lock` — a dirty tracked file after every build, for as
long as the probe had existed.

Adding `--locked` is not the fix by itself: measured, it turns the rewrite into
`error: cannot update the lock file … because --locked was passed`, and
`find_dep_rlib` has no fallback by design (issue 0464), so every NuttX build
would fail instead. The nested build has to resolve somewhere else, which is
what `resolver.lockfile-path` does.

WHAT THIS REFUSES
-----------------
A function that runs cargo through a shim-bypassing program (a `CARGO` env read,
or a `cargo` parameter handed one) and neither

  * confines itself to subcommands MEASURED not to touch the lockfile, nor
  * carries lock discipline — `--locked` / `--frozen` forwarded, or
    `resolver.lockfile-path` redirecting the file it writes.

The safe-subcommand set is measured, not assumed (2026-09-18, from
`packages/api/nros-c`, with the same unused `[patch]` that makes `build` write
the lock):

    metadata --no-deps        CLEAN
    locate-project            CLEAN
    --version                 CLEAN
    metadata (with deps)      DIRTY   <- the negative control

So `metadata` counts as safe only when `--no-deps` is on the same command.

NOT A BAN ON `Command::new("cargo")`. That spelling resolves through PATH, so
the shim already reaches it, and ~10 sites in tests and the CLI rely on that.
The rule is about the invocations that go round it.

Run: python3 scripts/check/check-nested-cargo-lock-discipline.py
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

# Where a build-script helper can live. `third-party/` is excluded by the
# tracked-file listing below (submodule contents are not this repo's files).
SCAN_ROOTS = ("packages",)

# A function whose cargo program did not come from the PATH name `cargo`.
BYPASS_SOURCE = re.compile(r"""env::var(?:_os)?\(\s*"CARGO"\s*\)|\bvar(?:_os)?\(\s*"CARGO"\s*\)""")
# ... or that was handed one by a caller.
CARGO_PARAM = re.compile(r"\bcargo\s*:\s*&?\s*(?:std::ffi::)?(?:OsStr|OsString|Path|str|String)")

# `Command::new(<simple path expression>)`. The string-literal spelling
# `Command::new("cargo")` does not match this — it resolves through PATH, so the
# shim already reaches it.
COMMAND_NEW = re.compile(r"Command::new\(\s*&?\s*([A-Za-z_][A-Za-z0-9_.:]*)\s*\)")


def names_cargo(text: str) -> bool:
    """Does any `Command::new(<ident>)` in `text` name a cargo binary?"""
    return any("cargo" in ident.lower() for ident in COMMAND_NEW.findall(text))

ARG_LITERAL = re.compile(r"""\.arg\(\s*"([^"]+)"\s*\)""")
SUBCOMMANDS = {
    "build",
    "b",
    "check",
    "c",
    "test",
    "t",
    "run",
    "r",
    "rustc",
    "bench",
    "doc",
    "metadata",
    "tree",
    "fetch",
    "package",
    "publish",
    "install",
    "nextest",
    "clippy",
    "update",
    "generate-lockfile",
}
# Measured clean — see the module docstring.
NON_RESOLVING = {"locate-project", "--version", "-V", "help", "--help"}

LOCK_DISCIPLINE = re.compile(
    r"resolver\.lockfile-path|--lockfile-path|--locked|--frozen|apply_nested_lock_discipline"
)

FIX = (
    "Either confine the invocation to a subcommand that does not resolve\n"
    "(`metadata --no-deps`, `locate-project`, `--version`), or give it lock\n"
    "discipline: forward `NROS_CARGO_FLAGS`' `--locked`/`--frozen` when it\n"
    "resolves the workspace lock as-is, and redirect with\n"
    "`--config resolver.lockfile-path=\"<probe dir>/Cargo.lock\"` (seeded from\n"
    "the workspace lock) when it injects a `[patch]` that lock cannot record.\n"
    "`nros_sizes_build::apply_nested_lock_discipline` is the one spelling —\n"
    "call it rather than adding a second."
)


def tracked_rust_sources() -> list[Path]:
    """Rust sources under the scanned roots that a clone would see."""
    out = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "-z", *SCAN_ROOTS],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return [
        REPO / rel
        for rel in out.split("\0")
        if rel.endswith(".rs") and "/third-party/" not in f"/{rel}"
    ]


def code_mask(text: str) -> list[bool]:
    """True at every offset that is CODE — not a comment, string, char or raw
    string.

    A naive brace count is not safe here, and the failure is the dangerous
    direction: `ws.rs`'s `extract_cargo_path_deps` carries `{` in a doc comment
    and in a `"{names:?}"` format string, so its body appeared to run 200 lines
    past its end and swallowed a sibling function — a body that absorbs its
    neighbours also absorbs their `--locked`, which is how a gate reports OK
    over a violation.
    """
    mask = [True] * len(text)
    i = 0
    n = len(text)
    while i < n:
        ch = text[i]
        two = text[i : i + 2]
        if two == "//":
            j = text.find("\n", i)
            j = n if j < 0 else j
            for k in range(i, j):
                mask[k] = False
            i = j
        elif two == "/*":
            depth = 1
            j = i + 2
            while j < n and depth:
                if text[j : j + 2] == "/*":
                    depth += 1
                    j += 2
                elif text[j : j + 2] == "*/":
                    depth -= 1
                    j += 2
                else:
                    j += 1
            for k in range(i, min(j, n)):
                mask[k] = False
            i = j
        elif ch == "r" and (raw := re.match(r'r(#*)"', text[i:])) is not None:
            # A raw string, `r"…"` or `r#"…"#` with any number of hashes. The
            # match is the branch CONDITION, so there is no `None` to unwrap:
            # an `r` that begins an ordinary identifier (`rlib`, `rustc`,
            # `return`) simply does not match, falls through to the `else`
            # below, and advances one character. That is a skip and not a
            # finding on purpose — a bare `r` in code is code, not a lexing
            # failure, and a gate that crashed here would read as
            # infrastructure noise rather than a verdict.
            hashes = raw.group(1)
            close = '"' + hashes
            j = text.find(close, i + len(raw.group(0)))
            j = n if j < 0 else j + len(close)
            for k in range(i, min(j, n)):
                mask[k] = False
            i = j
        elif ch == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            for k in range(i, min(j, n)):
                mask[k] = False
            i = j
        elif ch == "'":
            # A char literal (`'{'`) or a lifetime (`'a`). Only the literal can
            # hide a brace, and it is at most four chars wide.
            m = re.match(r"'(\\.|[^\\'])'", text[i:])
            if m:
                for k in range(i, i + m.end()):
                    mask[k] = False
                i += m.end()
            else:
                i += 1
        else:
            i += 1
    return mask


def functions(text: str) -> list[tuple[int, str, str]]:
    """(line number, signature, body) for every `fn` in `text`.

    Brace-balanced over CODE offsets only (see `code_mask`). A body that never
    balances runs to end-of-file, which errs toward including code rather than
    skipping it — but the mask is what keeps that from being the common case.
    """
    mask = code_mask(text)
    out: list[tuple[int, str, str]] = []
    for m in re.finditer(r"^[ \t]*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+\w+", text, re.M):
        if not mask[m.start()]:
            continue  # a `fn` inside a comment or a string
        start = text.find("{", m.end())
        while 0 <= start < len(text) and not mask[start]:
            start = text.find("{", start + 1)
        if start < 0:
            continue
        depth = 0
        end = len(text)
        for i in range(start, len(text)):
            if not mask[i]:
                continue
            ch = text[i]
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    end = i
                    break
        sig = text[m.start() : start]
        out.append((text.count("\n", 0, m.start()) + 1, sig, text[start:end]))
    return out


def offenders(paths: list[Path]) -> tuple[list[tuple[str, int, str, str]], int]:
    """(relative path, line, fn signature, why) plus the number of sites seen."""
    bad: list[tuple[str, int, str, str]] = []
    sites = 0
    for path in paths:
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        if "Command::new(" not in text:
            continue
        for lineno, sig, body in functions(text):
            if not names_cargo(body):
                continue
            bypasses = bool(BYPASS_SOURCE.search(body)) or bool(CARGO_PARAM.search(sig))
            if not bypasses:
                continue
            sites += 1
            args = set(ARG_LITERAL.findall(body))
            subs = args & SUBCOMMANDS
            if not subs and (args & NON_RESOLVING):
                continue
            if subs == {"metadata"} and "--no-deps" in args:
                continue
            if LOCK_DISCIPLINE.search(body):
                continue
            why = (
                f"runs {sorted(subs) or sorted(args & NON_RESOLVING) or ['(unknown)']} "
                "with no lock discipline"
            )
            try:
                shown = str(path.relative_to(REPO))
            except ValueError:  # the self-test's temp files
                shown = str(path)
            bad.append((shown, lineno, sig.strip().split("\n")[0], why))
    return bad, sites


SELF_TEST_BAD = '''
fn probe() -> Result<(), ()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(&cargo);
    cmd.arg("build").arg("-p").arg("nros");
    cmd.output().unwrap();
    Ok(())
}
'''

# The masking case, measured on `ws.rs`: a body carrying `{` in a comment and in
# a format string must END where it ends, or it absorbs the next function — and
# with it that function's `--locked`, turning a violation into an OK.
SELF_TEST_MASKING = '''
fn innocent(body: &str) -> Vec<String> {
    // Match `<name> = { path = "<rel>", ... }` form.
    let names = vec![body.to_string()];
    let _ = format!("cycle among {names:?}");
    // A raw string with an unbalanced brace, and identifiers that merely START
    // with `r` — the lexer must take neither for the other.
    let rlib = r#"fn not_a_fn() { "#;
    let rustc = r"} else {";
    let _ = (rlib, rustc);
    names
}

fn probe() -> Result<(), ()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    Command::new(&cargo).arg("build").output().unwrap();
    Ok(())
}

fn disciplined() -> Result<(), ()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    Command::new(&cargo).arg("build").arg("--locked").output().unwrap();
    Ok(())
}
'''

SELF_TEST_GOOD = '''
fn read_metadata() -> Result<(), ()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = Command::new(&cargo)
        .arg("metadata")
        .arg("--format-version=1")
        .arg("--no-deps")
        .output()
        .unwrap();
    drop(out);
    Ok(())
}

fn redirected() -> Result<(), ()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(&cargo);
    cmd.arg("build");
    cmd.arg("--config").arg("resolver.lockfile-path=\\"x\\"");
    cmd.output().unwrap();
    Ok(())
}

fn through_the_shim() -> Result<(), ()> {
    Command::new("cargo").arg("build").output().unwrap();
    Ok(())
}
'''


SELF_TEST_NO_CARGO = '''
//! A source with no cargo invocation, an unterminated raw string in a comment
//! r#"{ , and identifiers that start with `r`.
fn rustle(rlib: &Path) -> Result<(), ()> {
    let rustc = Command::new("rustc").arg("-V").output().ok();
    let _ = (rlib, rustc);
    Ok(())
}
'''


def self_test() -> None:
    """A detector that can never fail is not a detector (issue 1040's shape)."""
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        bad_file = Path(tmp) / "bad.rs"
        bad_file.write_text(SELF_TEST_BAD, encoding="utf-8")
        good_file = Path(tmp) / "good.rs"
        good_file.write_text(SELF_TEST_GOOD, encoding="utf-8")

        found, seen = offenders([bad_file])
        if len(found) != 1 or seen != 1:
            raise SystemExit(
                "check-nested-cargo-lock-discipline: SELF-TEST FAILED — the "
                f"detector missed issue 1307's own shape (found {found}, "
                f"{seen} site(s))"
            )
        found, seen = offenders([good_file])
        if found:
            raise SystemExit(
                "check-nested-cargo-lock-discipline: SELF-TEST FAILED — the "
                f"detector flags a sanctioned spelling: {found}"
            )
        if seen != 2:
            raise SystemExit(
                "check-nested-cargo-lock-discipline: SELF-TEST FAILED — "
                f"expected 2 shim-bypassing sites in the good sample, saw {seen} "
                "(the PATH-name spelling must not count as one)"
            )

        # Inputs the detector must survive without finding anything: a source
        # with no cargo invocation at all, and an empty file. A gate that
        # raises on these is noise rather than a verdict.
        for name, body in (("quiet.rs", SELF_TEST_NO_CARGO), ("empty.rs", "")):
            quiet = Path(tmp) / name
            quiet.write_text(body, encoding="utf-8")
            found, seen = offenders([quiet])
            if found or seen:
                raise SystemExit(
                    "check-nested-cargo-lock-discipline: SELF-TEST FAILED — "
                    f"{name} has no shim-bypassing cargo invocation but the "
                    f"detector reported {seen} site(s) / {found}"
                )
            if inventory([quiet]):
                raise SystemExit(
                    "check-nested-cargo-lock-discipline: SELF-TEST FAILED — "
                    f"--list invented a row for {name}"
                )

        mask_file = Path(tmp) / "masking.rs"
        mask_file.write_text(SELF_TEST_MASKING, encoding="utf-8")
        found, seen = offenders([mask_file])
        names = [sig for _, _, sig, _ in found]
        if seen != 2 or len(found) != 1 or "fn probe" not in names[0]:
            raise SystemExit(
                "check-nested-cargo-lock-discipline: SELF-TEST FAILED — brace "
                "balancing ran through a comment or a format string, so a "
                f"function absorbed its neighbour (sites={seen}, found={names})"
            )


def inventory(paths: list[Path]) -> list[tuple[str, int, str, str, str]]:
    """Every shim-bypassing site and how it is accounted for — the sweep,
    printed, so `rg -n 'var_os("CARGO")'` does not have to be re-derived by
    hand next time."""
    rows: list[tuple[str, int, str, str, str]] = []
    for path in paths:
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        if "Command::new(" not in text:
            continue
        for lineno, sig, body in functions(text):
            if not names_cargo(body):
                continue
            if not (BYPASS_SOURCE.search(body) or CARGO_PARAM.search(sig)):
                continue
            args = set(ARG_LITERAL.findall(body))
            subs = sorted(args & SUBCOMMANDS) or sorted(args & NON_RESOLVING)
            if LOCK_DISCIPLINE.search(body):
                verdict = "lock discipline"
            elif (args & SUBCOMMANDS) == {"metadata"} and "--no-deps" in args:
                verdict = "non-resolving (--no-deps)"
            elif not (args & SUBCOMMANDS) and (args & NON_RESOLVING):
                verdict = "non-resolving"
            else:
                verdict = "UNACCOUNTED"
            try:
                shown = str(path.relative_to(REPO))
            except ValueError:
                shown = str(path)
            name = re.search(r"fn\s+(\w+)", sig)
            rows.append(
                (shown, lineno, name.group(1) if name else "?", " ".join(subs) or "?", verdict)
            )
    return rows


def main() -> int:
    self_test()
    if "--list" in sys.argv[1:]:
        for rel, lineno, name, subs, verdict in inventory(tracked_rust_sources()):
            print(f"{rel}:{lineno}\t{name}\t[{subs}]\t{verdict}")
        return 0
    paths = tracked_rust_sources()
    if not paths:
        print(
            "check-nested-cargo-lock-discipline: FAIL — no tracked Rust sources "
            "under packages/; the scan would pass vacuously."
        )
        return 1
    bad, sites = offenders(paths)
    if not sites:
        print(
            "check-nested-cargo-lock-discipline: FAIL — no shim-bypassing cargo "
            "invocation found at all. The size probe is one, so the harvest is "
            "broken, not the tree."
        )
        return 1
    if bad:
        print("check-nested-cargo-lock-discipline: FAIL (issue 1307)")
        print("  A nested cargo bypasses the `scripts/bin/cargo` --locked shim")
        print("  ($CARGO is the real binary) and says nothing about the lockfile")
        print("  it may rewrite:")
        print("")
        for rel, lineno, sig, why in bad:
            print(f"  {rel}:{lineno}: {sig}")
            print(f"      {why}")
        print("")
        for line in FIX.splitlines():
            print(f"  {line}")
        return 1
    print(
        f"check-nested-cargo-lock-discipline: OK ({sites} shim-bypassing cargo "
        f"invocation(s) across {len(paths)} tracked Rust source(s); each is "
        "non-resolving or carries lock discipline)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
