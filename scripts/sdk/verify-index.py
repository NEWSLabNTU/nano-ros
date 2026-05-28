#!/usr/bin/env python3
"""Phase 187.4 gate — verify every prebuilt `dist` referenced by the SDK index
is live and hash-correct.

Run in nano-ros CI on any PR that touches `nros-sdk-index.toml`. For each
`[tool.<name>].dist.<host>` it downloads the asset and checks sha256, so a
version bump can only merge once its prebuilt assets exist on the
NEWSLabNTU/nano-ros-sdk Releases (the 187.4 guarantee, across repos). Read-only
— no token, no upload. Source-only tools (no `dist`) are skipped here; their
recipe is exercised by the nano-ros-sdk source-build CI.

    python3 scripts/sdk/verify-index.py nros-sdk-index.toml
"""

import hashlib
import sys
import urllib.request

try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:
    try:
        import tomli as tomllib  # `pip install tomli` on 3.10 and older
    except ModuleNotFoundError:
        sys.exit("verify-index: needs Python 3.11+ (tomllib) or `pip install tomli`")


def main(path: str) -> int:
    with open(path, "rb") as f:
        index = tomllib.load(f)

    checked = 0
    failures = []
    for name, tool in index.get("tool", {}).items():
        for host, dist in tool.get("dist", {}).items():
            url = dist.get("url", "")
            want = dist.get("sha256", "")
            if not url or not want:
                failures.append(f"{name}/{host}: dist needs both url + sha256")
                continue
            checked += 1
            try:
                with urllib.request.urlopen(url, timeout=120) as resp:
                    data = resp.read()
            except Exception as e:  # noqa: BLE001 — report any download failure
                failures.append(f"{name}/{host}: download failed: {e}\n    {url}")
                continue
            got = hashlib.sha256(data).hexdigest()
            if got != want:
                failures.append(
                    f"{name}/{host}: sha256 mismatch\n    want {want}\n    got  {got}"
                )
            else:
                print(f"ok   {name}/{host}  ({len(data)} bytes)")

    if failures:
        print(f"\nFAIL: {len(failures)} problem(s):", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print(f"\nverify-index: {checked} prebuilt asset(s) verified")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: verify-index.py <nros-sdk-index.toml>")
    raise SystemExit(main(sys.argv[1]))
