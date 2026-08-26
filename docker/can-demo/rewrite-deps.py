"""Point zenoh-c's zenoh dependencies at a local checkout.

Rewrites the git source out of the dependency lines entirely, rather than
adding a [patch] section. A [patch] leaves the original git source in place for
cargo to resolve, and zenoh-c's build script invokes the opaque-types sub-build
with --offline, where resolving `branch = "main"` from a cached checkout fails.
Removing the source means there is nothing left to resolve.
"""
import re, sys

root, *manifests = sys.argv[1:]
paths = {
    "zenoh": f"{root}/zenoh",
    "zenoh-ext": f"{root}/zenoh-ext",
    "zenoh-protocol": f"{root}/commons/zenoh-protocol",
    "zenoh-runtime": f"{root}/commons/zenoh-runtime",
    "zenoh-util": f"{root}/commons/zenoh-util",
}
git_re = re.compile(
    r'git\s*=\s*"https://github\.com/eclipse-zenoh/zenoh\.git"\s*,\s*'
    r'(?:branch|rev|tag)\s*=\s*"[^"]*"'
)
for m in manifests:
    out, changed = [], 0
    for line in open(m):
        name = line.split("=", 1)[0].strip() if "=" in line else ""
        if name in paths and git_re.search(line):
            line, n = git_re.subn(f'path = "{paths[name]}"', line)
            changed += n
        out.append(line)
    if not changed:
        sys.exit(f"{m}: no zenoh git dependency was rewritten; layout changed")
    open(m, "w").writelines(out)
    print(f"  {m}: rewrote {changed} dependency source(s) to local paths")
