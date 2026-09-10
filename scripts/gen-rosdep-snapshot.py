#!/usr/bin/env python3
"""Generate `nros-rosdep-snapshot.toml` — a PINNED, VENDORED rosdep snapshot.

RFC-0099 D8 / phase-447 D3. This is the DATA half of the resolution ladder in
`prereq_resolve.rs`: a user writing `<depend>libopencv-dev</depend>` in their
own package hits UNKNOWN today, where a ROS user gets it from rosdep's
community database. The gap is data, not format.

WHAT THIS IS NOT
================

It is not rosdep, and running it is not resolution. RFC-0062's amendment
rejected rosdep as a RESOLVER for three reasons — it answers for one provider
of four, it cannot carry a `check` probe, and a resolver present on one host
and not another makes one tree resolve two ways. All three are properties of a
host-installed runtime resolver. A snapshot generated HERE, committed, pinned
to a rosdistro commit and read as TOML by the same reader that reads
`nros-sdk-index.toml` is none of them:

  * it still answers for one provider of four — so it is a FALLBACK RUNG below
    `[prereq.*]`, never the resolver;
  * it still cannot carry a `check` — so every key it supplies is marked
    UNPROBED at the point of use, rather than reported as verified;
  * it cannot differ between machines, because it is a tracked file.

`nros` never invokes this script and never reaches the network. This is a
maintainer tool; its output is the artifact.

THE PROJECTION, and why it is not verbatim
==========================================

rosdep's OS vocabulary is ~16 distributions. nano-ros installs through exactly
four managers (`apt`, `dnf`, `pacman`, `brew`) — the same four `[prereq.*]`
declares — so everything else in the upstream YAML is data we could not act on
if we vendored it. Projecting to the four we have:

  apt     <- ubuntu, else debian
  dnf     <- fedora, else rhel
  pacman  <- arch
  brew    <- osx's `homebrew` installer

A key that projects to NOTHING is dropped, not emitted empty: an entry that
resolves a name and then installs nothing is the silent success this tree
files issues about (`[system.*]` is already validated the same way — "maps to
no package manager at all" is a hard error there).

An OS whose value is version-keyed (`ubuntu: {jammy: [..], noble: [..]}`)
contributes its `'*'` entry when there is one and is DROPPED otherwise. The
OS-VERSION dimension is RFC-0099 D9, a separate item; guessing a version here
would put an answer in the snapshot that the schema cannot express and the
reader cannot qualify.

THE PIN
=======

`--ref <sha>` fetches from that rosdistro commit and rewrites the snapshot.
With no `--ref`, the ref already recorded in the snapshot is reused, so a
re-run verifies rather than bumps. `--verify` re-derives at the pinned ref and
diffs against the committed file without writing — the maintainer check that
the committed bytes are what upstream says, which needs network and therefore
is not a gate.

`scripts/check-rosdep-snapshot.py` is the offline gate: provenance complete,
keys sorted and unique, no key empty, no key carrying a probe.

Run:  python3 scripts/gen-rosdep-snapshot.py [--ref <sha>] [--verify]
"""

import argparse
import hashlib
import os
import re
import sys
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SNAPSHOT = os.path.join(ROOT, "nros-rosdep-snapshot.toml")

UPSTREAM = "https://github.com/ros/rosdistro"
RAW = "https://raw.githubusercontent.com/ros/rosdistro/{ref}/{path}"

# The rosdep sources rosdistro publishes, in the order rosdep itself merges
# them. `osx-homebrew.yaml` is last on purpose: it is the brew-only overlay,
# and its entries add the `brew` column to keys `base.yaml` already carries.
SOURCES = [
    "rosdep/base.yaml",
    "rosdep/python.yaml",
    "rosdep/ruby.yaml",
    "rosdep/osx-homebrew.yaml",
]

# manager -> the upstream OS names that answer for it, in preference order.
MANAGERS = {
    "apt": ["ubuntu", "debian"],
    "dnf": ["fedora", "rhel"],
    "pacman": ["arch"],
    "brew": ["osx"],
}

# The installer key inside an OS whose value is an installer map. Only osx
# needs one here: `{osx: {homebrew: {packages: [..]}}}`.
INSTALLER = {"osx": "homebrew"}


def fetch(ref, path):
    url = RAW.format(ref=ref, path=path)
    with urllib.request.urlopen(url, timeout=60) as fh:  # noqa: S310 - pinned host
        return fh.read()


def flatten(os_name, value):
    """One upstream OS value -> a flat package list, or None if unusable.

    Three upstream shapes:
      [a, b]                          a flat list
      {'*': [a], 'noble': [b]}        version-keyed — only '*' is portable
      {'homebrew': {'packages': [a]}} installer-keyed
    A version-keyed value with no '*' is DROPPED: expressing it needs the
    OS-version dimension (RFC-0099 D9), and picking one version's answer for
    every version would be a guess the schema cannot qualify.
    """
    if isinstance(value, list):
        return [str(v) for v in value] or None
    if not isinstance(value, dict):
        return None
    inst = INSTALLER.get(os_name)
    if inst is not None and inst in value:
        v = value[inst]
        if isinstance(v, dict):
            v = v.get("packages")
        if isinstance(v, list):
            return [str(x) for x in v] or None
        return None
    if "*" in value:
        return flatten(os_name, value["*"])
    return None


def project(entry):
    """One upstream rosdep entry -> {manager: [packages]} over our four."""
    out = {}
    if not isinstance(entry, dict):
        return out
    for manager, os_names in MANAGERS.items():
        for os_name in os_names:
            if os_name not in entry:
                continue
            pkgs = flatten(os_name, entry[os_name])
            if pkgs:
                out[manager] = pkgs
                break
    return out


def merge(docs):
    """Merge the source documents the way rosdep does: later files add columns.

    An OS an earlier file already answered for is NOT overwritten — the overlay
    files exist to add a manager, not to contradict one.
    """
    merged = {}
    for doc in docs:
        for key, value in doc.items():
            if not isinstance(value, dict):
                continue
            slot = merged.setdefault(key, {})
            for os_name, os_value in value.items():
                slot.setdefault(os_name, os_value)
    return merged


def toml_str(s):
    return '"%s"' % s.replace("\\", "\\\\").replace('"', '\\"')


def toml_list(items):
    return "[%s]" % ", ".join(toml_str(i) for i in items)


def render(ref, files, keys, stats):
    """The snapshot file. Sorted, one table per key, one line per manager.

    The FORMAT is the review mechanism: a bump's diff is one line per changed
    package list plus the provenance lines, so "what did upstream change" is
    answerable by reading the patch. A blob nobody can read in a diff is a
    supply-chain surface; this one is 4 fields wide and alphabetical.
    """
    out = []
    out.append("# GENERATED by scripts/gen-rosdep-snapshot.py — DO NOT EDIT BY HAND.")
    out.append("#")
    out.append("# A pinned, vendored projection of ros/rosdistro's rosdep database onto the")
    out.append("# four package managers nano-ros installs through. RFC-0099 D8; phase-447 D3.")
    out.append("#")
    out.append("# This is a FALLBACK RUNG below `[prereq.*]`, for `provider = \"system\"` only.")
    out.append("# Keys here carry NO `check` probe and NO `role`, and are reported as UNPROBED")
    out.append("# wherever they resolve — RFC-0062's amendment is explicit that the probe is")
    out.append("# what makes a missing prereq diagnosable rather than a loader error, and a")
    out.append("# database that supplies packages but not probes leaves that half to us.")
    out.append("#")
    out.append("# Regenerate/bump:  python3 scripts/gen-rosdep-snapshot.py --ref <rosdistro-sha>")
    out.append("# Verify (network): python3 scripts/gen-rosdep-snapshot.py --verify")
    out.append("# Gate (offline):   python3 scripts/check-rosdep-snapshot.py")
    out.append("")
    out.append("[snapshot]")
    out.append("upstream = %s" % toml_str(UPSTREAM))
    out.append("ref = %s" % toml_str(ref))
    out.append("generator = %s" % toml_str("scripts/gen-rosdep-snapshot.py"))
    out.append("# Every upstream file this projection read, with the sha256 of the bytes")
    out.append("# fetched at `ref`. These are what a reviewer re-fetches to check a bump.")
    for path, digest in files:
        out.append("")
        out.append("[[snapshot.file]]")
        out.append("path = %s" % toml_str(path))
        out.append("sha256 = %s" % toml_str(digest))
    out.append("")
    out.append("# %d upstream key(s) read; %d projected onto at least one manager." % stats)
    out.append("# The remainder map only to distributions nano-ros does not install through,")
    out.append("# or are version-keyed with no portable answer (RFC-0099 D9).")
    for key in sorted(keys):
        out.append("")
        out.append("[key.%s]" % (key if re.fullmatch(r"[A-Za-z0-9_-]+", key) else toml_str(key)))
        for manager in ("apt", "dnf", "pacman", "brew"):
            if manager in keys[key]:
                out.append("%s = %s" % (manager, toml_list(keys[key][manager])))
    return "\n".join(out) + "\n"


def pinned_ref():
    """The ref the committed snapshot records, so a bare run re-derives it."""
    try:
        with open(SNAPSHOT, encoding="utf8") as fh:
            for line in fh:
                m = re.match(r'ref\s*=\s*"([0-9a-f]{40})"', line.strip())
                if m:
                    return m.group(1)
    except OSError:
        pass
    return None


def build(ref):
    import yaml  # deferred: only the network path needs it

    docs = []
    files = []
    for path in SOURCES:
        raw = fetch(ref, path)
        files.append((path, hashlib.sha256(raw).hexdigest()))
        docs.append(yaml.safe_load(raw.decode("utf8")))
    merged = merge(docs)
    keys = {}
    for key, entry in merged.items():
        pkgs = project(entry)
        if pkgs:
            keys[key] = pkgs
    return render(ref, files, keys, (len(merged), len(keys)))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ref", help="rosdistro commit to pin to (40-hex)")
    ap.add_argument("--verify", action="store_true", help="re-derive and diff, do not write")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()

    if args.self_test:
        self_test()
        return 0
    self_test()

    ref = args.ref or pinned_ref()
    if not ref:
        sys.stderr.write(
            "error: no ref. Pass --ref <rosdistro-sha> for the first generation;\n"
            "afterwards the ref is read back from %s.\n" % SNAPSHOT
        )
        return 2
    if not re.fullmatch(r"[0-9a-f]{40}", ref):
        sys.stderr.write("error: --ref must be a full 40-hex commit, got %r\n" % ref)
        return 2

    text = build(ref)
    if args.verify:
        try:
            with open(SNAPSHOT, encoding="utf8") as fh:
                have = fh.read()
        except OSError as e:
            sys.stderr.write("error: cannot read %s: %s\n" % (SNAPSHOT, e))
            return 1
        if have != text:
            sys.stderr.write(
                "gen-rosdep-snapshot --verify: the committed snapshot is NOT what\n"
                "rosdistro %s projects to. Re-run without --verify and review the diff.\n" % ref
            )
            return 1
        sys.stdout.write("gen-rosdep-snapshot --verify: OK — committed bytes match %s\n" % ref)
        return 0

    with open(SNAPSHOT, "w", encoding="utf8") as fh:
        fh.write(text)
    sys.stdout.write("wrote %s (%d bytes) from rosdistro %s\n" % (SNAPSHOT, len(text), ref))
    return 0


def self_test():
    # A flat list is taken as-is.
    assert flatten("ubuntu", ["libopencv-dev"]) == ["libopencv-dev"]
    # Version-keyed: '*' is the only portable answer …
    assert flatten("ubuntu", {"*": ["ack"], "focal": ["ack-grep"]}) == ["ack"]
    # … and without one, the key contributes nothing rather than a guess.
    assert flatten("ubuntu", {"focal": ["a"], "noble": ["b"]}) is None
    # A null version value must not become a package named "None".
    assert flatten("ubuntu", {"*": None, "bionic": None}) is None
    # Installer-keyed osx, both spellings upstream uses.
    assert flatten("osx", {"homebrew": {"packages": ["asio"]}}) == ["asio"]
    assert flatten("osx", {"homebrew": ["asio"]}) == ["asio"]
    # A non-homebrew installer is not ours to read.
    assert flatten("slackware", {"slackpkg": {"packages": ["boost"]}}) is None
    # Preference order: ubuntu wins over debian for apt, fedora over rhel.
    got = project(
        {
            "ubuntu": ["u"],
            "debian": ["d"],
            "fedora": ["f"],
            "rhel": ["r"],
            "arch": ["a"],
            "osx": {"homebrew": {"packages": ["b"]}},
            "gentoo": ["g"],
        }
    )
    assert got == {"apt": ["u"], "dnf": ["f"], "pacman": ["a"], "brew": ["b"]}, got
    # The fallback fires when the preferred OS is missing or unusable.
    assert project({"debian": ["d"]}) == {"apt": ["d"]}
    assert project({"ubuntu": {"noble": ["u"]}, "debian": ["d"]}) == {"apt": ["d"]}
    # A key that maps only to distributions we do not install through projects
    # to nothing, and the caller drops it.
    assert project({"gentoo": ["x"], "nixos": ["y"]}) == {}
    # The overlay adds a column without overwriting one.
    m = merge([{"k": {"ubuntu": ["u"]}}, {"k": {"osx": {"homebrew": {"packages": ["b"]}}}}])
    assert project(m["k"]) == {"apt": ["u"], "brew": ["b"]}, m
    m2 = merge([{"k": {"ubuntu": ["first"]}}, {"k": {"ubuntu": ["second"]}}])
    assert project(m2["k"]) == {"apt": ["first"]}, m2
    # Rendering is deterministic, sorted, and quotes what it must.
    text = render(
        "0" * 40,
        [("rosdep/base.yaml", "a" * 64)],
        {"zzz": {"apt": ["z"]}, "aaa": {"apt": ["a"], "brew": ["a"]}},
        (2, 2),
    )
    assert text.index("[key.aaa]") < text.index("[key.zzz]"), text
    assert 'apt = ["a"]\nbrew = ["a"]' in text, text
    assert 'ref = "%s"' % ("0" * 40) in text
    sys.stdout.write("gen-rosdep-snapshot self-test: OK\n")


if __name__ == "__main__":
    sys.exit(main())
