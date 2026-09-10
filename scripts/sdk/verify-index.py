#!/usr/bin/env python3
"""Phase 187.4 + 196.3 gate — verify the SDK index is internally coherent and
that every prebuilt `dist` it references is live and hash-correct.

Run in nano-ros CI on any PR that touches `nros-sdk-index.toml` (or
`.gitmodules`). Two kinds of check:

1. **Structure (offline, always run):**
   - every `[board.*].packages` / `[rmw.*].packages` name resolves to a defined
     `[tool]`/`[source]`/`[gated]` entry (mirrors `SdkIndex::validate`, but
     statically — no `nros` build needed) (Phase 191.x);
   - every `[source.*]` provisioning recipe is coherent: submodule mode needs a
     `dest`; clone mode (a `git` with no `submodule`) needs a `ref`, plus a
     `dest` when it is provisioned into the WORKSPACE and no `dest` when it is
     provisioned into the STORE, whose path is derived from name + version
     (Phase 195.B; phase-440 / RFC-0095 D1+D2);
   - every `[source.*].submodule` path is a real submodule in `.gitmodules`, and
     a declared `git` URL matches that submodule's URL — so the index and
     `.gitmodules` **can't drift** (the 195.B SSOT guarantee).

2. **Prebuilt assets (network, skip with `--structure-only`):** for each
   `[tool.<name>].dist.<host>` download the asset and check sha256, so a version
   bump can only merge once its prebuilt exists on NEWSLabNTU/nano-ros-sdk
   Releases (the 187.4 guarantee). Read-only — no token, no upload. Source-only
   tools (no `dist`) are skipped here; their recipe is exercised by the
   nano-ros-sdk source-build CI.

    python3 scripts/sdk/verify-index.py nros-sdk-index.toml
    python3 scripts/sdk/verify-index.py --structure-only nros-sdk-index.toml
"""

import hashlib
import os
import sys
import urllib.request

try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:
    try:
        import tomli as tomllib  # `pip install tomli` on 3.10 and older
    except ModuleNotFoundError:
        sys.exit("verify-index: needs Python 3.11+ (tomllib) or `pip install tomli`")


def parse_gitmodules(path):
    """Map submodule path -> url from a `.gitmodules` file. Empty dict if absent
    (a checkout without the file still lets the source checks run, minus drift)."""
    mapping = {}
    if not os.path.isfile(path):
        return mapping
    cur_path = None
    with open(path, encoding="utf-8") as f:
        for raw in f:
            line = raw.strip()
            if line.startswith("[submodule"):
                cur_path = None
            elif line.startswith("path") and "=" in line:
                cur_path = line.split("=", 1)[1].strip()
            elif line.startswith("url") and "=" in line and cur_path is not None:
                mapping[cur_path] = line.split("=", 1)[1].strip()
    return mapping


def verify_structure(index, index_path):
    """Offline coherence checks. Returns a list of failure strings."""
    failures = []
    tools = index.get("tool", {})
    sources = index.get("source", {})
    gated = index.get("gated", {})
    known = set(tools) | set(sources) | set(gated)

    # (1) board / rmw package references resolve.
    for kind in ("board", "rmw"):
        for name, entry in index.get(kind, {}).items():
            for pkg in entry.get("packages", []):
                if pkg not in known:
                    failures.append(
                        f"{kind} '{name}' references undefined package '{pkg}' "
                        f"(not a [tool]/[source]/[gated] entry)"
                    )
            # Phase 197.4 — build sources were folded into `packages` (validated
            # above). `dev_sources` (opt-in, fetched by tools/setup.sh --with-dev)
            # must still resolve to a [source.*] specifically.
            for s in entry.get("dev_sources", []):
                if s not in sources:
                    failures.append(
                        f"{kind} '{name}' dev_sources references '{s}' "
                        f"(not a defined [source.*] entry)"
                    )

    # (1b) [reference.*].sources resolve to [source.*].
    for name, entry in index.get("reference", {}).items():
        for s in entry.get("sources", []):
            if s not in sources:
                failures.append(
                    f"reference '{name}' references '{s}' "
                    f"(not a defined [source.*] entry)"
                )

    # (2) + (3) source coherence + .gitmodules drift guard.
    gitmodules = parse_gitmodules(
        os.path.join(os.path.dirname(os.path.abspath(index_path)) or ".", ".gitmodules")
    )
    for name, src in sources.items():
        git = src.get("git")
        ref = src.get("ref")
        dest = src.get("dest")
        submodule = src.get("submodule")
        if submodule is not None:
            if dest is None:
                failures.append(f"source '{name}' has `submodule` but no `dest`")
            if gitmodules and submodule not in gitmodules:
                failures.append(
                    f"source '{name}' submodule path '{submodule}' is not a "
                    f"submodule in .gitmodules (index↔.gitmodules drift)"
                )
            elif git is not None and submodule in gitmodules:
                want = gitmodules[submodule]
                if want != git:
                    failures.append(
                        f"source '{name}' git URL drifts from .gitmodules:\n"
                        f"      index      {git}\n"
                        f"      .gitmodules {want}"
                    )
        elif git is not None:  # clone mode
            if ref is None:
                failures.append(f"source '{name}' has `git` but no `ref` (clone needs a pinned ref)")
            # phase-440 / RFC-0095 D1+D2 — WHERE decides whether `dest` is
            # authored at all, and BOTH directions are refused so the index and
            # the store can never disagree about one source's path. Mirrors
            # `SdkIndex::validate` in
            # packages/cli/nros-cli-core/src/orchestration/sdk_index.rs; the two
            # must move together or the gate passes what the CLI refuses.
            location = src.get("location", "workspace")
            if location not in ("workspace", "store"):
                failures.append(
                    f"source '{name}' has location = {location!r} "
                    f"(expected \"workspace\" or \"store\")"
                )
            elif location == "workspace" and dest is None:
                failures.append(
                    f"source '{name}' has `git` but no `dest` (where to provision "
                    f"it). A store source needs no `dest` — set location = \"store\"."
                )
            elif location == "store" and dest is not None:
                failures.append(
                    f"source '{name}' has location = \"store\" AND a `dest`. The "
                    f"store path is DERIVED from name + version; drop the `dest`."
                )
    return failures


def verify_dist(index):
    """Network: download + hash each prebuilt `dist`. (failures, checked_count)."""
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
    return failures, checked


def selftest() -> None:
    """Prove the `location` rule fires in BOTH directions, on the normal path.

    This exists because the rule already drifted once: phase-440 moved
    `[source.rosidl]` into the store — dropping its `dest`, which the CLI's own
    validator accepts — and this gate, which claims to mirror that validator,
    still demanded a `dest` and failed the PR. A gate whose rule lives in two
    languages needs a case in each, or one of them rots silently.

    Runs on every invocation (it is five dict literals), so there is no lane in
    which the gate ships unexercised.
    """
    import tempfile

    def problems(src):
        # An empty dir: clone-mode sources never consult `.gitmodules`, and a
        # missing one is the read `parse_gitmodules` already tolerates.
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "nros-sdk-index.toml")
            return verify_structure({"source": {"probe": src}}, path)

    git = "https://example.invalid/x"
    ref = "0" * 40
    cases = [
        # (source table, must a `location`/`dest` problem be reported?)
        ({"git": git, "ref": ref, "dest": "third-party/x"}, False),
        ({"git": git, "ref": ref}, True),
        ({"git": git, "ref": ref, "location": "store"}, False),
        ({"git": git, "ref": ref, "location": "store", "dest": "third-party/x"}, True),
        ({"git": git, "ref": ref, "location": "elsewhere"}, True),
    ]
    for src, want_fail in cases:
        got = problems(src)
        if bool(got) != want_fail:
            raise SystemExit(
                f"verify-index selftest: source {src!r} expected "
                f"{'a failure' if want_fail else 'no failure'}, got {got!r}"
            )


def main(path: str, structure_only: bool) -> int:
    selftest()
    with open(path, "rb") as f:
        index = tomllib.load(f)

    failures = verify_structure(index, path)
    if failures:
        print(f"structure: {len(failures)} problem(s)", file=sys.stderr)
    else:
        print("structure: ok (board/rmw refs + source coherence + .gitmodules)")

    checked = 0
    if not structure_only:
        dist_failures, checked = verify_dist(index)
        failures += dist_failures

    if failures:
        print(f"\nFAIL: {len(failures)} problem(s):", file=sys.stderr)
        for fail in failures:
            print(f"  - {fail}", file=sys.stderr)
        return 1
    suffix = "" if structure_only else f"; {checked} prebuilt asset(s) verified"
    print(f"\nverify-index: index coherent{suffix}")
    return 0


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if a != "--structure-only"]
    structure_only = "--structure-only" in sys.argv[1:]
    if len(args) != 1:
        sys.exit("usage: verify-index.py [--structure-only] <nros-sdk-index.toml>")
    raise SystemExit(main(args[0], structure_only))
