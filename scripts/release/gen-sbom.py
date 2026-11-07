#!/usr/bin/env python3
"""Write a CycloneDX 1.5 SBOM for the `nros` release asset (issue 2329).

The asset ships the binaries of several cargo workspaces (the CLI, the launch
resolver, the build profiler, the runner and command packages). The SBOM lists
every crate COMPILED INTO those binaries: `cargo tree -e normal,build` per
shipped package, for the release's host target, so dev-dependencies and
other-platform crates are left out. It is a statement about what the asset
CONTAINS, which is the question a product's security review asks; the build
inputs of a firmware image are a different document (`nros-sdk.lock`).

Usage:
    gen-sbom.py --version 0.6.0-nros8 --commit <sha> --out <file> \
        --target x86_64-unknown-linux-gnu \
        <manifest-path>:<package|*>[:<features>] [...]
        (* = every workspace member; <features> = --no-default-features --features <features>)
    gen-sbom.py --self-test
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import uuid
from datetime import datetime, timezone

LINE = re.compile(r"^(?P<name>\S+) v(?P<version>\S+)(?: \((?P<src>[^)]*)\))?\|(?P<license>.*)$")
REPO = "https://github.com/NEWSLabNTU/nano-ros"


def parse_tree(text: str) -> dict[tuple[str, str], dict]:
    """`cargo tree --prefix none --format '{p}|{l}'` lines -> components."""
    out: dict[tuple[str, str], dict] = {}
    for raw in text.splitlines():
        raw = raw.strip()
        if not raw:
            continue
        # `(*)` marks a subtree already printed; `(proc-macro)` a kind.
        raw = re.sub(r" \((\*|proc-macro)\)", "", raw)
        m = LINE.match(raw)
        if not m:
            raise ValueError(f"unparsed cargo tree line: {raw!r}")
        name, version, src = m["name"], m["version"], m["src"]
        lic = m["license"].strip()
        comp = {"type": "library", "name": name, "version": version}
        if lic:
            comp["licenses"] = [{"expression": lic}]
        if src is None:
            comp["purl"] = f"pkg:cargo/{name}@{version}"
        elif src.startswith(("http://", "https://", "git+")):
            url, _, rev = src.partition("#")
            comp["purl"] = f"pkg:cargo/{name}@{version}?vcs_url={url}%40{rev}" if rev else f"pkg:cargo/{name}@{version}?vcs_url={url}"
            comp["externalReferences"] = [{"type": "vcs", "url": src}]
        else:
            # A path dependency: first-party source in this repository.
            comp["purl"] = f"pkg:cargo/{name}@{version}?vcs_url=git%2B{REPO}"
            comp["supplier"] = {"name": "nano-ros"}
        out[(name, version)] = comp
    return out


def cargo_tree(manifest: str, package: str, target: str, features: str | None = None) -> str:
    cmd = [
        "cargo", "tree", "--manifest-path", manifest, "--offline",
        "-e", "normal,build", "--target", target,
        "--prefix", "none", "--format", "{p}|{l}",
    ] + (["--workspace"] if package == "*" else ["-p", package])
    if features is not None:
        cmd += ["--no-default-features", "--features", features]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        sys.stderr.write(r.stderr)
        raise SystemExit(f"gen-sbom: cargo tree failed for {manifest}:{package} (exit {r.returncode})")
    return r.stdout


def document(version: str, commit: str, components: dict) -> dict:
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": f"urn:uuid:{uuid.uuid5(uuid.NAMESPACE_URL, f'{REPO}/releases/nros-{version}')}",
        "version": 1,
        "metadata": {
            "timestamp": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "component": {
                "type": "application",
                "name": "nros",
                "version": version,
                "purl": f"pkg:github/NEWSLabNTU/nano-ros@{commit}",
                "externalReferences": [{"type": "vcs", "url": f"{REPO}/tree/{commit}"}],
            },
        },
        "components": [components[k] for k in sorted(components)],
    }


def self_test() -> None:
    sample = "\n".join([
        "nros-cli v0.6.0 (/w/packages/cli/nros-cli)|MIT OR Apache-2.0",
        "serde v1.0.228|MIT OR Apache-2.0",
        "serde v1.0.228|MIT OR Apache-2.0 (*)",
        "serde_derive v1.0.228 (proc-macro)|MIT OR Apache-2.0",
        "ros-launch-manifest v0.3.12 (https://github.com/x/y?tag=v0.3.12#abc123)|MIT",
        "nolicense v1.0.0|",
    ])
    comps = parse_tree(sample)
    assert len(comps) == 5, comps
    assert comps[("serde", "1.0.228")]["purl"] == "pkg:cargo/serde@1.0.228"
    assert comps[("nros-cli", "0.6.0")]["supplier"] == {"name": "nano-ros"}
    git = comps[("ros-launch-manifest", "0.3.12")]
    assert git["externalReferences"][0]["url"].endswith("#abc123"), git
    assert "licenses" not in comps[("nolicense", "1.0.0")]
    try:
        parse_tree("garbage line")
    except ValueError:
        pass
    else:
        raise AssertionError("an unparsable line must refuse, not be dropped")
    doc = document("0.0.0-test", "deadbeef", comps)
    assert doc["components"] == sorted(doc["components"], key=lambda c: (c["name"], c["version"]))
    json.dumps(doc)
    print("gen-sbom.py self-test OK")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--version")
    ap.add_argument("--commit")
    ap.add_argument("--out")
    ap.add_argument("--target", default="x86_64-unknown-linux-gnu")
    ap.add_argument("roots", nargs="*", help="<manifest-path>:<package|*>[:<features>]")
    a = ap.parse_args()
    if a.self_test:
        self_test()
        return 0
    if not (a.version and a.commit and a.out and a.roots):
        ap.error("--version, --commit, --out and at least one root are required")
    comps: dict = {}
    for root in a.roots:
        parts = root.split(":")
        if len(parts) not in (2, 3) or not all(parts[:2]):
            ap.error(f"root {root!r} is not <manifest-path>:<package>[:<features>]")
        features = parts[2] if len(parts) == 3 else None
        comps.update(parse_tree(cargo_tree(parts[0], parts[1], a.target, features)))
    with open(a.out, "w", encoding="utf-8") as fh:
        json.dump(document(a.version, a.commit, comps), fh, indent=1, sort_keys=True)
        fh.write("\n")
    print(f"gen-sbom: {len(comps)} components -> {a.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
