#!/usr/bin/env python3
"""The release the book's "pin an older release" example pins.

`book/src/guide/build/install-options.md` shows a project pinned to an OLDER
release than the one installed, and its later steps (`nros build` with no
separate sync) need what that release ships. A literal pin rots: the example
pinned 0.6.0-nros4 long after the book's steps needed `nros configure`, which
first shipped in nros7, and its probe tracks sat red behind an `awaits`.

So the older pin is DERIVED, in one place: the release just before the index's
`[tool.nros] version` (0.6.0-nrosN -> 0.6.0-nros(N-1)). It moves with every
index bump, so it is never more than one release behind what the book is
written for.

  pinned_example.py           print the version
  pinned_example.py -         print it for the index text on stdin (the probe
                              reads the COMMIT under test's index, never the
                              working tree's -- issue 2604)
  pinned_example.py --check   the page's two literals equal it (the gate)
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
INDEX = ROOT / "nros-sdk-index.toml"
PAGE = ROOT / "book/src/guide/build/install-options.md"
SHAPE = re.compile(r"^(?P<base>\d+\.\d+\.\d+-nros)(?P<n>\d+)$")


class Underivable(Exception):
    pass


def index_version(index_text: str) -> str:
    m = re.search(r'^\[tool\.nros\]\n(?:(?!^\[).*\n)*?version = "([^"]+)"', index_text, re.M)
    if not m:
        raise Underivable("no `[tool.nros] version`")
    return m.group(1)


def derive(index_text: str) -> str:
    v = index_version(index_text)
    m = SHAPE.match(v)
    if not m or int(m.group("n")) < 2:
        raise Underivable(f"`[tool.nros] version = \"{v}\"` is not `<x.y.z>-nros<N>` with N >= 2, "
                          "so the release before it cannot be derived; extend scripts/lib/pinned_example.py")
    return f"{m.group('base')}{int(m.group('n')) - 1}"


def problems(page_text: str, want: str) -> list:
    found = {
        "the pin step": re.findall(r'\[toolchain\]\\nversion = "([^"]+)"', page_text),
        "the expected manifest path": re.findall(r"sdk/nros/([^/\s]+)/share/nros/manifest\.toml", page_text),
    }
    bad = []
    for what, vals in found.items():
        if not vals:
            bad.append(f"{what}: not found")
        bad += [f"{what}: {v}" for v in vals if v != want]
    return bad


def self_test(quiet: bool = False) -> None:
    """Negative controls: each case must give the stated answer, or the gate is not a gate."""
    idx = '[tool.nros]\nversion = "0.6.0-nros9"\n'
    good = 'printf \'[toolchain]\\nversion = "0.6.0-nros8"\\n\'\n.../sdk/nros/0.6.0-nros8/share/nros/manifest.toml\n'
    cases = [
        ("derives N-1", derive(idx) == "0.6.0-nros8"),
        ("a matching page passes", problems(good, "0.6.0-nros8") == []),
        ("a stale pin step fails", problems(good.replace('version = "0.6.0-nros8"', 'version = "0.6.0-nros4"'), "0.6.0-nros8") != []),
        ("a stale manifest path fails", problems(good.replace("nros/0.6.0-nros8/", "nros/0.6.0-nros4/"), "0.6.0-nros8") != []),
        ("a page without the pin fails", problems("nothing here\n", "0.6.0-nros8") != []),
    ]
    for shape in ('[tool.nros]\nversion = "0.6.0-nros1"\n', '[tool.nros]\nversion = "0.6.0"\n', "[other]\nversion = \"1\"\n"):
        try:
            derive(shape)
            cases.append((f"refuses {shape!r}", False))
        except Underivable:
            cases.append((f"refuses {shape!r}", True))
    failed = [name for name, ok in cases if not ok]
    if failed:
        sys.exit("pinned_example.py: SELFTEST FAILED -- " + "; ".join(failed))
    if not quiet:
        print(f"pinned_example.py: selftest OK ({len(cases)} cases)")


def derived(index_text: str = None, origin: str = None) -> str:
    try:
        return derive(INDEX.read_text() if index_text is None else index_text)
    except Underivable as e:
        sys.exit(f"{origin or INDEX}: {e}")


def check() -> int:
    self_test(quiet=True)
    want = derived()
    bad = problems(PAGE.read_text(), want)
    have = index_version(INDEX.read_text())
    if bad:
        print(f"check-book-pinned-example: FAILED -- {PAGE.relative_to(ROOT)} must pin {want}, "
              f"the release before the index's {have}:")
        for b in bad:
            print(f"  {b}")
        print("  Move both literals on the page to it (it follows every index bump).")
        return 1
    print(f"check-book-pinned-example: OK ({want}, the release before {have})")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--check"]:
        sys.exit(check())
    if sys.argv[1:] == ["-"]:
        print(derived(sys.stdin.read(), "<stdin>"))
        sys.exit(0)
    if sys.argv[1:]:
        sys.exit("usage: pinned_example.py [--check | -]")
    print(derived())
