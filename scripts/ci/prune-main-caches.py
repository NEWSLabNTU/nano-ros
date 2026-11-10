#!/usr/bin/env python3
"""Keep only the newest Actions cache entry of one key family on `main`.

Issue 2412. `warm-cache` saves the sccache directory on every push to `main`
under `sccache-Linux-check-<sha>`. Cache keys are immutable, so a per-commit
key is the only way to update the entry, and each save added another ~2 GB
entry beside the last. On 2026-11-10 three of them sat on `main` and the
repository held 10.67 GB of its 10 GB budget, so GitHub's LRU eviction was
choosing what to drop for every other lane.

Every reader restores through the `sccache-Linux-check-` prefix, which takes
the newest matching entry, so the older ones are never read. This deletes them,
right after the save, keeping the entry this run wrote (`--keep-key`). When this
run saved nothing (an exact hit, or a failed save), it keeps the newest entry
of the family instead, so a prune can never leave the family empty.

Scope is the ref named by `--ref` (`refs/heads/main`); caches scoped to pull
requests and merge-queue refs are never touched.

Usage (in a job with `permissions: actions: write`):
  GITHUB_TOKEN=... GITHUB_REPOSITORY=owner/repo \\
    scripts/ci/prune-main-caches.py --prefix sccache-Linux-check- \\
      --ref refs/heads/main --keep-key sccache-Linux-check-<sha>
  scripts/ci/prune-main-caches.py --self-test
"""

import argparse
import json
import os
import sys
import urllib.parse
import urllib.request

API = os.environ.get("GITHUB_API_URL", "https://api.github.com")


def choose(entries, prefix, keep_key):
    """Split the family's entries into (kept, deleted).

    `entries` are the API's cache objects (`id`, `key`, `ref`, `created_at`,
    `size_in_bytes`), already filtered to one ref. A key outside the family is
    neither kept nor deleted: the API's `key` filter is a prefix match, and
    this is the second check that makes the deletion safe on its own.
    """
    family = [e for e in entries if e["key"].startswith(prefix)]
    family.sort(key=lambda e: e["created_at"], reverse=True)
    kept = [e for e in family if e["key"] == keep_key]
    if not kept and family:
        kept = [family[0]]
    kept_ids = {e["id"] for e in kept}
    return kept, [e for e in family if e["id"] not in kept_ids]


def request(method, path, token):
    req = urllib.request.Request(
        f"{API}{path}",
        method=method,
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        body = resp.read()
    return json.loads(body) if body else None


def list_entries(repo, prefix, ref, token):
    out, page = [], 1
    while True:
        q = urllib.parse.urlencode(
            {"key": prefix, "ref": ref, "per_page": 100, "page": page}
        )
        data = request("GET", f"/repos/{repo}/actions/caches?{q}", token)
        batch = data.get("actions_caches", [])
        out.extend(e for e in batch if e.get("ref") == ref)
        if len(batch) < 100:
            return out
        page += 1


def self_test():
    def e(i, key, t):
        return {"id": i, "key": key, "created_at": t, "size_in_bytes": 1}

    p = "sccache-Linux-check-"
    a = e(1, p + "aaa", "2026-11-08T00:00:00Z")
    b = e(2, p + "bbb", "2026-11-09T00:00:00Z")
    c = e(3, p + "ccc", "2026-11-10T00:00:00Z")
    other = e(4, "sccache-Linux-other-x", "2026-11-11T00:00:00Z")

    kept, gone = choose([a, b, c, other], p, p + "bbb")
    assert [k["id"] for k in kept] == [2], kept
    assert sorted(g["id"] for g in gone) == [1, 3], gone

    kept, gone = choose([a, b, c], p, p + "zzz")
    assert [k["id"] for k in kept] == [3], "no save: keep the newest"
    assert sorted(g["id"] for g in gone) == [1, 2]

    kept, gone = choose([other], p, p + "zzz")
    assert kept == [] and gone == [], "a key outside the family is never deleted"

    kept, gone = choose([], p, p + "zzz")
    assert kept == [] and gone == []
    print("prune-main-caches --self-test: OK (4 cases)")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--prefix")
    ap.add_argument("--ref")
    ap.add_argument("--keep-key")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        self_test()
        return 0
    if not (a.prefix and a.ref and a.keep_key):
        ap.error("--prefix, --ref and --keep-key are required")
    token = os.environ.get("GITHUB_TOKEN", "")
    repo = os.environ.get("GITHUB_REPOSITORY", "")
    if not token or not repo:
        print("prune-main-caches: GITHUB_TOKEN and GITHUB_REPOSITORY must be set", file=sys.stderr)
        return 2

    kept, gone = choose(list_entries(repo, a.prefix, a.ref, token), a.prefix, a.keep_key)
    mib = lambda e: e["size_in_bytes"] / 1048576
    for k in kept:
        print(f"keep    {k['key']}  {mib(k):.0f} MiB  ({k['created_at']})")
    freed = 0
    for g in gone:
        print(f"delete  {g['key']}  {mib(g):.0f} MiB  ({g['created_at']})")
        if not a.dry_run:
            request("DELETE", f"/repos/{repo}/actions/caches/{g['id']}", token)
        freed += g["size_in_bytes"]
    verb = "would free" if a.dry_run else "freed"
    print(f"prune-main-caches: {a.ref} {a.prefix}*: kept {len(kept)}, {verb} {freed / 1048576:.0f} MiB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
