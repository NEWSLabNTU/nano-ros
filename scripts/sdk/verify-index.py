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
   `[tool.<name>].dist.<host>` check that the asset exists and its sha256 is the
   index's, so a version bump can only merge once its prebuilt exists on
   NEWSLabNTU/nano-ros-sdk Releases (the 187.4 guarantee). Read-only, no upload.
   Source-only tools (no `dist`) are skipped here; their recipe is exercised by
   the nano-ros-sdk source-build CI.

   A GitHub release asset is checked against the `digest` the releases API
   reports (GitHub's own sha256 of the stored bytes), one API call per release
   tag, with no download. An asset uploaded before GitHub computed digests is
   checked against a checksum file the same release publishes (`<asset>.sha256`,
   `sha256.sum`, `SHA256SUMS`) — the publisher's claim, labelled as such in the
   output. Everything else — another host, no checksum file, an API call that
   failed (rate limit) — is DOWNLOADED and hashed, streamed and in parallel.
   Downloading everything was the old behaviour, and it cost 46 assets /
   6.6 GiB, each read whole into memory, one at a time; `--download` downloads
   every asset again. `GITHUB_TOKEN`/`GH_TOKEN`, when set,
   authenticates the API calls (60/h unauthenticated).

    python3 scripts/sdk/verify-index.py nros-sdk-index.toml
    python3 scripts/sdk/verify-index.py --structure-only nros-sdk-index.toml
    python3 scripts/sdk/verify-index.py --download nros-sdk-index.toml
"""

import concurrent.futures
import hashlib
import json
import os
import re
import sys
import urllib.parse
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
            # RFC-0103 D5 (phase-484 W3) — mirrors `SdkIndex::validate`: the pin
            # is readable without a checkout. `check-source-refs` holds the
            # values equal to git; this only requires them.
            if git is None or ref is None:
                failures.append(
                    f"source '{name}' has `submodule` but no `git`/`ref` "
                    f"(run python3 scripts/check/check-source-refs.py --write)"
                )
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
                failures.append(
                    f"source '{name}' has `git` but no `ref` (clone needs a pinned ref)"
                )
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
                    f'(expected "workspace" or "store")'
                )
            elif location == "workspace" and dest is None:
                failures.append(
                    f"source '{name}' has `git` but no `dest` (where to provision "
                    f'it). A store source needs no `dest` — set location = "store".'
                )
            elif location == "store" and dest is not None:
                failures.append(
                    f"source '{name}' has location = \"store\" AND a `dest`. The "
                    f"store path is DERIVED from name + version; drop the `dest`."
                )
    return failures


# The tag is everything between `download/` and the asset name, so a tag with a
# slash in it (verus's `release/0.2026...`) still parses.
GITHUB_RELEASE_ASSET = re.compile(
    r"^https://github\.com/([^/]+)/([^/]+)/releases/download/(.+)/([^/?#]+)$"
)

# Checksum files a release may publish beside its assets, most specific first.
# `{name}` is the asset's own name.
SUM_FILES = ("{name}.sha256", "sha256.sum", "SHA256SUMS", "sha256sums.txt")


def github_release_asset(url):
    """`(owner, repo, tag, asset name)` for a GitHub release download URL, else
    None. The tag and name are URL-decoded, as the releases API spells them."""
    m = GITHUB_RELEASE_ASSET.match(url)
    if not m:
        return None
    owner, repo, tag, name = m.groups()
    return owner, repo, urllib.parse.unquote(tag), urllib.parse.unquote(name)


def release_assets(owner, repo, tag):
    """`{asset name: (digest or None, download url)}` for one release."""
    api = (
        f"https://api.github.com/repos/{owner}/{repo}/releases/tags/"
        f"{urllib.parse.quote(tag, safe='')}"
    )
    req = urllib.request.Request(api, headers={"Accept": "application/vnd.github+json"})
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(req, timeout=30) as resp:
        release = json.load(resp)
    return {
        a["name"]: (a.get("digest"), a["browser_download_url"])
        for a in release.get("assets", [])
    }


def sum_file_entries(text):
    """`{file name: sha256 hex}` from `sha256sum`-format text. A bare hash on
    its own (a `<asset>.sha256` holding only the digest) maps to None."""
    out = {}
    for line in text.splitlines():
        parts = line.split()
        if not parts or not re.fullmatch(r"[0-9a-fA-F]{64}", parts[0]):
            continue
        name = parts[1].lstrip("*") if len(parts) > 1 else None
        if name and name.startswith("./"):
            name = name[2:]
        out[name] = parts[0].lower()
    return out


def download_sha256(url):
    """`(sha256 hex, byte count)` of `url`, streamed in 1 MiB chunks."""
    h = hashlib.sha256()
    size = 0
    with urllib.request.urlopen(url, timeout=120) as resp:
        while chunk := resp.read(1 << 20):
            h.update(chunk)
            size += len(chunk)
    return h.hexdigest(), size


def verify_dist(index, force_download=False):
    """Network: check each prebuilt `dist` exists and hashes to the index's
    sha256. Returns (failures, checked_count)."""
    failures = []
    rows = []  # (label, url, want)
    for name, tool in index.get("tool", {}).items():
        for host, dist in tool.get("dist", {}).items():
            url = dist.get("url", "")
            want = dist.get("sha256", "")
            if not url or not want:
                failures.append(f"{name}/{host}: dist needs both url + sha256")
                continue
            rows.append((f"{name}/{host}", url, want))

    # One API call per release tag answers every asset in it.
    releases = {}  # (owner, repo, tag) -> {name: (digest, url)} | None (API failed)
    if not force_download:
        tags = {gh[:3] for _, url, _ in rows if (gh := github_release_asset(url))}

        def fetch(key):
            try:
                return key, release_assets(*key), None
            except Exception as e:  # noqa: BLE001 — fall back to a download
                return key, None, e

        with concurrent.futures.ThreadPoolExecutor(8) as pool:
            for key, found, err in pool.map(fetch, sorted(tags)):
                releases[key] = found
                if err is not None:
                    print(
                        f"note {'/'.join(key)}: releases API failed ({err}); downloading its assets"
                    )

    def check(row):
        """`(label, verdict line)`; a verdict starting `FAIL ` is a failure."""
        label, url, want = row
        gh = None if force_download else github_release_asset(url)
        assets = releases.get(gh[:3]) if gh else None
        if assets is not None:
            asset = gh[3]
            if asset not in assets:
                return (
                    label,
                    f"FAIL {label}: no asset {asset!r} in release {gh[2]}\n    {url}",
                )
            digest = assets[asset][0]
            if digest:
                if digest != f"sha256:{want}":
                    return label, (
                        f"FAIL {label}: sha256 mismatch (releases API digest)\n"
                        f"    want sha256:{want}\n    got  {digest}"
                    )
                return label, f"ok   {label}  (releases API digest)"
            # Uploaded before GitHub computed digests: the asset is listed, so
            # it is live; take the hash from a checksum file the release
            # publishes. That is the publisher's claim about the bytes rather
            # than GitHub's own; `--download` measures them.
            for pattern in SUM_FILES:
                sums = assets.get(pattern.format(name=asset))
                if sums is None:
                    continue
                try:
                    with urllib.request.urlopen(sums[1], timeout=60) as resp:
                        entries = sum_file_entries(
                            resp.read().decode("utf-8", "replace")
                        )
                except Exception:  # noqa: BLE001 — fall through to a download
                    break
                got = entries.get(asset, entries.get(None))
                if got is None:
                    continue
                sum_name = pattern.format(name=asset)
                if got != want:
                    return label, (
                        f"FAIL {label}: sha256 mismatch (release's {sum_name})\n"
                        f"    want {want}\n    got  {got}"
                    )
                return label, f"ok   {label}  (release's {sum_name})"
        try:
            got, size = download_sha256(url)
        except Exception as e:  # noqa: BLE001 — report any download failure
            return label, f"FAIL {label}: download failed: {e}\n    {url}"
        if got != want:
            return (
                label,
                f"FAIL {label}: sha256 mismatch\n    want {want}\n    got  {got}",
            )
        return label, f"ok   {label}  (downloaded, {size} bytes)"

    with concurrent.futures.ThreadPoolExecutor(4) as pool:
        verdicts = dict(pool.map(check, rows))

    for label, _, _ in rows:
        line = verdicts[label]
        if line.startswith("FAIL "):
            failures.append(line[len("FAIL ") :])
        else:
            print(line)
    return failures, len(rows)


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

    # Only a GitHub release download may skip the download.
    url_cases = [
        (
            "https://github.com/o/r/releases/download/qemu-11.0.0-nros7/q%2Bx.tar.zst",
            ("o", "r", "qemu-11.0.0-nros7", "q+x.tar.zst"),
        ),
        (
            "https://github.com/v/v/releases/download/release/0.2026.1/v-x86.zip",
            ("v", "v", "release/0.2026.1", "v-x86.zip"),
        ),
        ("https://files.pythonhosted.org/packages/ab/cd/x.whl", None),
        ("https://github.com/o/r/archive/refs/tags/v1.tar.gz", None),
    ]
    for url, want in url_cases:
        if github_release_asset(url) != want:
            raise SystemExit(
                f"verify-index selftest: {url} parsed to "
                f"{github_release_asset(url)!r}, expected {want!r}"
            )

    # Both `sha256sum` shapes a release publishes, plus a bare digest.
    h = "a" * 64
    sums = sum_file_entries(f"{h}  ./x.tar.xz\n{h.upper()} *y.zip\nnot a line\n{h}\n")
    if sums != {"x.tar.xz": h, "y.zip": h, None: h}:
        raise SystemExit(f"verify-index selftest: sum file parsed to {sums!r}")


def main(path: str, structure_only: bool, force_download: bool) -> int:
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
        dist_failures, checked = verify_dist(index, force_download)
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
    flags = {"--structure-only", "--download"}
    args = [a for a in sys.argv[1:] if a not in flags]
    structure_only = "--structure-only" in sys.argv[1:]
    force_download = "--download" in sys.argv[1:]
    if len(args) != 1:
        sys.exit(
            "usage: verify-index.py [--structure-only | --download] <nros-sdk-index.toml>"
        )
    raise SystemExit(main(args[0], structure_only, force_download))
