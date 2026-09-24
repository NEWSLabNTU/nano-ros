#!/usr/bin/env python3
"""The `[python.*]` layer, for a build that has no `nros` CLI — issue 1482.

Sibling of `prereq-packages.py`, and the same argument: a consumer that needs
two package names should not have to build the CLI first (phase-413 W3). It is
not a second source of truth — it reads `nros-sdk-index.toml`, the file the CLI
reads, exactly as `prereq-packages.py` and `check-dist-runtime-deps.py` do.

WHO ASKS

The self-hosted runner's container image. `scripts/ci/runner-container.sh`
generates its Dockerfile from the index and already resolves `[prereq.*]`
through `prereq-packages.py`; the `[python.*]` layer never reached it, so the
image carried no `catkin_pkg` / `empy` / `lark` / `PyYAML` and every cyclone
`msg2idl` in tier-2 nightly died on `ModuleNotFoundError` (issue 1457).

WHICH ENTRIES — DERIVED, NOT LISTED

An entry with a `check = { cmd = … }` is an EXECUTABLE that one provisioning
verb installs into a place the container already persists: `west` into the
in-repo Zephyr venv (`scripts/build/zephyr-python.sh`, group `west` in
`check-python-deps.py`), `clang-format` into `build/clang-format/bin` as the
wheel's standalone binary, `colcon` alongside a ROS install. Baking a second
copy into the image is issue 0500's shape — two producers for one name — and
for `west` it is worse than cosmetic: `runner-doctor.sh` probes `command -v
west`, so an image-provided copy would make the `nros-sdk-zephyr` label true by
construction rather than by provisioning.

An entry with NO `check` is probed by `import`ing its `module` in the host's
own `python3` (`PythonDep::module`, and the `[python.*]` header says so). That
is a fact about the AMBIENT interpreter — the one cmake, ninja and
`msg_to_cyclone_idl.py` invoke — which in a container is the image's job and
nothing else's, because the running container is `--cap-drop ALL` with no root.

So: **the image layer is the entries with no `check.cmd`.** Derived, so a new
`[python.*]` entry joins or stays out by what it declares, with no list here to
forget to update.

APT VS PIP — MEASURED IN THE IMAGE, NOT ON THE HOST THAT GENERATES IT

Issue 1481's rule: the index DECLARES `apt = [..]` xor `apt_refused = "<why>"`,
and the resolver asks apt for a candidate before preferring it. The host that
generates the Dockerfile is the wrong host to ask — measured on the maintainer's
workstation, which has the ROS 2 apt repo, `python3-catkin-pkg` reports
candidate `1.1.0-101`; in `ubuntu:22.04` it reports `0.4.24-2`, from universe.
Encoding either at generation time makes the image's contents depend on who ran
the generator.

So the split is in two halves, and each runs where its facts are:

* **generation time** (`--emit json`) reads the index — which entries, their
  apt names, pip name and version pin. Host-independent.
* **image build time** (`--resolve <json>`) asks THAT image's apt. The same
  rule as `python_provider::resolve`: declared apt names, a candidate for every
  one of them, and the index's pin satisfied, else pip with the reason named.

The pin rule is the one thing that must stay in step with the Rust half, and it
is the same two functions by the same names (`upstream_version`,
`candidate_satisfies`) over the same cases in `--self-test`.

Usage::

    python-packages.py --emit json                  # declarations (from the index)
    python-packages.py --emit keys                  # the derived key set
    python-packages.py --resolve <json> --emit apt  # apt names, measured here
    python-packages.py --resolve <json> --emit pip  # pip specs, measured here
    python-packages.py --resolve <json> --emit plan # one line per entry, with the reason
    python-packages.py --resolve <json> --verify    # import what was installed
    python-packages.py --self-test
"""

import argparse
import json
import os
import subprocess
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(_HERE))
# In the image the two files sit side by side in the build context; in the tree
# the helper lives under `scripts/lib`. Both, so one file serves both roles.
sys.path.insert(0, _HERE)
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
import index_packages  # noqa: E402 — the one manager-field reader (phase-447 D2)

INDEX = os.path.join(ROOT, "nros-sdk-index.toml")


def load(path=INDEX):
    try:
        import tomllib as toml
    except ModuleNotFoundError:  # py<3.11 — and the image is jammy/3.10
        import tomli as toml
    with open(path, "rb") as fh:
        return toml.load(fh)


# --------------------------------------------------------------------------
# generation time: the index


def image_layer_keys(index):
    """The `[python.*]` entries a container image owns — see the header.

    An entry with a `check.cmd` is an executable some verb installs elsewhere;
    an entry without one is an import against the ambient interpreter.
    """
    out = []
    for key, entry in sorted(index.get("python", {}).items()):
        check = entry.get("check") or {}
        if check.get("cmd"):
            continue
        out.append(key)
    return out


def declarations(index, keys=None):
    """What the index says about each key — no host measurement in here."""
    python = index.get("python", {})
    keys = image_layer_keys(index) if keys is None else list(keys)
    missing = [k for k in keys if k not in python]
    if missing:
        raise SystemExit(
            f"python-packages: no [python.{missing[0]}] in nros-sdk-index.toml"
            + (f" (and {len(missing) - 1} more)" if len(missing) > 1 else "")
            + "\n  Declare it there — the index is the SSoT (RFC-0062)."
        )
    out = []
    for key in keys:
        entry = python[key]
        pip = entry["pip"]
        out.append(
            {
                "key": key,
                "pip": pip,
                "module": entry.get("module") or pip.replace("-", "_"),
                "version": entry.get("version"),
                # The RAW field: a flat list or a per-release table. Which
                # release applies is the IMAGE's fact, resolved at build time.
                "apt": entry.get("apt"),
                "apt_refused": entry.get("apt_refused"),
                "why": entry.get("why"),
            }
        )
    return out


# --------------------------------------------------------------------------
# image build time: this host's apt
#
# The Rust half is `orchestration::python_provider`. Kept to the same names and
# the same conservatism: anything unreadable falls to pip, which is a duplicate,
# where accepting a wrong-major `empy` is broken codegen.


def upstream_version(debian):
    """`1:3.3.4-2` -> `3.3.4`."""
    after_epoch = debian
    if ":" in debian:
        epoch, rest = debian.split(":", 1)
        if epoch and epoch.isdigit():
            after_epoch = rest
    if "-" in after_epoch:
        return after_epoch[: after_epoch.rfind("-")]
    return after_epoch


def candidate_satisfies(candidate, want):
    """Does an apt candidate satisfy the index's exact pin?"""
    upstream = upstream_version(candidate)
    if not upstream.startswith(want):
        return False
    rest = upstream[len(want) :]
    if not rest:
        return True
    return not (rest[0].isdigit() or rest[0] == ".")


def parse_policy(text):
    """`apt-cache policy <pkg>` -> {"installed": .., "candidate": ..}.

    `None` when apt knows no such package at all — which is what an empty body
    means, and it is a DIFFERENT answer from `Candidate: (none)`.
    """
    installed = candidate = None
    seen = False
    for line in text.splitlines():
        line = line.strip()
        for label, key in (("Installed:", "installed"), ("Candidate:", "candidate")):
            if line.startswith(label):
                seen = True
                value = line[len(label) :].strip()
                value = None if value == "(none)" else value
                if key == "installed":
                    installed = value
                else:
                    candidate = value
    return {"installed": installed, "candidate": candidate} if seen else None


class AptCache:
    """`apt-cache policy` — read-only, and the only thing that can answer
    "does THIS image have a candidate" without being told."""

    @staticmethod
    def detect():
        try:
            rc = subprocess.run(
                ["apt-cache", "--version"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            ).returncode
        except OSError:
            return None
        return AptCache() if rc == 0 else None

    def policy(self, package):
        try:
            out = subprocess.run(
                ["apt-cache", "policy", package],
                capture_output=True,
                text=True,
                check=False,
            ).stdout
        except OSError:
            return None
        return parse_policy(out)


def resolve(decl, release=None, apt=None):
    """Which provider this host should use for one declaration.

    Returns `("apt", [packages], reason)` or `("pip", [spec], reason)`.
    """
    spec = decl["pip"] + ("==" + decl["version"] if decl.get("version") else "")
    declared = index_packages.for_release(decl.get("apt"), release)
    if not declared:
        reason = decl.get("apt_refused") or (
            "the index maps apt, but not for this release"
        )
        return "pip", [spec], reason
    if apt is None:
        return "pip", [spec], "this host has no apt"
    for pkg in declared:
        policy = apt.policy(pkg)
        if policy is None or policy.get("candidate") is None:
            return "pip", [spec], f"apt has no candidate for {pkg} here"
        want = decl.get("version")
        if want and not candidate_satisfies(policy["candidate"], want):
            return (
                "pip",
                [spec],
                f"{pkg} candidate {policy['candidate']} does not satisfy the pin {want}",
            )
    return "apt", list(declared), "apt has a candidate for every declared package"


def resolve_all(decls, release=None, apt=None):
    return [(d, resolve(d, release, apt)) for d in decls]


# --------------------------------------------------------------------------


def do_verify(decls):
    """Import every module, in THIS interpreter — the build-time assertion.

    A package that installs and cannot be imported must fail while the image is
    being built, not eleven minutes into a Zephyr build in a nightly lane.
    """
    bad = []
    for decl in decls:
        try:
            __import__(decl["module"])
        except Exception as exc:  # noqa: BLE001 — any import failure is the answer
            bad.append(f"{decl['key']}: import {decl['module']} -> {exc}")
    if bad:
        print("python-packages: the installed layer does not import:", file=sys.stderr)
        for line in bad:
            print(f"  {line}", file=sys.stderr)
        return 1
    print(
        "python-packages: verified "
        + " ".join(d["module"] for d in decls)
        + " import in "
        + sys.executable
    )
    return 0


def self_test():
    """Exercised on the NORMAL path: `runner-container.sh` runs it before it
    generates a Dockerfile, so these cases are re-measured rather than recalled.
    """
    failures = 0

    def fail(msg):
        nonlocal failures
        print(f"  FAIL: {msg}")
        failures += 1

    # --- the derivation -------------------------------------------------
    index = {
        "python": {
            "lark": {"pip": "lark", "apt": ["python3-lark"]},
            "pyyaml": {"pip": "PyYAML", "module": "yaml", "apt": ["python3-yaml"]},
            "empy": {"pip": "empy", "module": "em", "version": "3.3.4",
                     "apt": ["python3-empy"]},
            "west": {"pip": "west", "apt_refused": "PyPI only",
                     "check": {"cmd": "west"}},
        }
    }
    if image_layer_keys(index) != ["empy", "lark", "pyyaml"]:
        fail(f"derivation: {image_layer_keys(index)}")
    decls = declarations(index)
    if [d["module"] for d in decls] != ["em", "lark", "yaml"]:
        fail("module name not derived from pip where unauthored")

    # --- the split, against a fake apt ----------------------------------
    class FakeApt:
        def __init__(self, table):
            self.table = table

        def policy(self, package):
            return self.table.get(package)

    jammy = FakeApt(
        {
            "python3-lark": {"installed": None, "candidate": "1.1.1-1"},
            "python3-yaml": {"installed": None, "candidate": "5.4.1-1ubuntu1"},
            "python3-empy": {"installed": None, "candidate": "3.3.4-2"},
        }
    )
    got = {d["key"]: r for d, r in resolve_all(decls, "jammy", jammy)}
    for key in ("lark", "pyyaml", "empy"):
        if got[key][0] != "apt":
            fail(f"{key} should resolve to apt on jammy: {got[key]}")

    # No candidate -> pip, with the package NAMED.
    empty = FakeApt({})
    kind, spec, reason = resolve(decls[1], "jammy", empty)
    if kind != "pip" or spec != ["lark"] or "python3-lark" not in reason:
        fail(f"no candidate did not fall to pip naming the package: {kind} {reason}")

    # No apt at all (a non-Debian host) -> pip.
    if resolve(decls[1], "jammy", None)[0] != "pip":
        fail("a host with no apt did not fall to pip")

    # A refusal is the authored answer, and its reason is carried through.
    west = declarations(index, ["west"])[0]
    kind, spec, reason = resolve(west, "jammy", jammy)
    if kind != "pip" or reason != "PyPI only":
        fail(f"apt_refused not honoured: {kind} {reason}")

    # An empty per-release override is "not packaged there" — a different fact.
    perrelease = declarations(
        {"python": {"x": {"pip": "x", "apt": {"default": ["python3-x"], "noble": []}}}}
    )[0]
    if resolve(perrelease, "noble", jammy)[0] != "pip":
        fail("an empty release override did not fall to pip")

    # --- the pin ---------------------------------------------------------
    for candidate, want, expect in (
        ("3.3.4-2", "3.3.4", True),
        ("1:3.3.4-2", "3.3.4", True),
        ("3.3.4+ds1-1", "3.3.4", True),
        ("4.0.1-1", "3.3.4", False),
        ("3.3.40-1", "3.3.4", False),
        ("3.3.4.1-1", "3.3.4", False),
    ):
        if candidate_satisfies(candidate, want) is not expect:
            fail(f"pin: {candidate} vs {want} != {expect}")
    pinned = FakeApt({"python3-empy": {"installed": None, "candidate": "4.0.1-1"}})
    kind, _, reason = resolve(decls[0], "jammy", pinned)
    if kind != "pip" or "3.3.4" not in reason:
        fail(f"an unmet pin did not fall to pip: {kind} {reason}")

    # --- the policy parser ------------------------------------------------
    if parse_policy("") is not None:
        fail("an empty apt-cache body is 'no such package', not 'no candidate'")
    got = parse_policy("python3-lark:\n  Installed: (none)\n  Candidate: 1.1.1-1\n")
    if got != {"installed": None, "candidate": "1.1.1-1"}:
        fail(f"policy parse: {got}")

    # An unknown key is an ERROR naming the key — the RFC-0062 rung.
    try:
        declarations(index, ["nope"])
    except SystemExit:
        pass
    else:
        fail("an unknown key did not raise")

    index_packages.self_test()

    if failures:
        print(f"python-packages self-test: {failures} case(s) FAILED")
        return 1
    print("python-packages self-test: OK")
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--emit", choices=("json", "keys", "apt", "pip", "plan"))
    ap.add_argument("--resolve", metavar="JSON", help="declarations from --emit json")
    ap.add_argument("--verify", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("keys", nargs="*")
    args = ap.parse_args()

    if args.self_test:
        return self_test()

    if args.resolve:
        with open(args.resolve) as fh:
            decls = json.load(fh)
        if args.verify:
            return do_verify(decls)
        release = index_packages.host_release()
        rows = resolve_all(decls, release, AptCache.detect())
        if args.emit == "apt":
            print(" ".join(p for _, (k, ps, _) in rows if k == "apt" for p in ps))
        elif args.emit == "pip":
            print(" ".join(p for _, (k, ps, _) in rows if k == "pip" for p in ps))
        elif args.emit == "plan":
            print(f"nros [python.*] layer — release {release or 'unknown'}")
            for decl, (kind, pkgs, reason) in rows:
                print(f"  {decl['key']:<14} {kind:<3} {' '.join(pkgs):<34} ({reason})")
        else:
            ap.error("--resolve needs --emit apt|pip|plan or --verify")
        return 0

    index = load()
    keys = args.keys or None
    if args.emit == "keys":
        print(" ".join(image_layer_keys(index) if keys is None else keys))
    elif args.emit == "json":
        print(json.dumps(declarations(index, keys), indent=2, sort_keys=True))
    else:
        ap.error("need --emit json|keys, or --resolve <json>")
    return 0


if __name__ == "__main__":
    sys.exit(main())
