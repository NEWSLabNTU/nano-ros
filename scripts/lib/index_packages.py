"""Read a `[prereq.*]` manager field — the ONE Python spelling (phase-447 D2).

A manager field (`apt`, `dnf`, `pacman`, `brew`) has two shapes since D2:

    apt = ["libssl3"]                                         # every release
    apt = { default = ["libssl3"], noble = ["libssl3t64"] }   # per release

`PrereqDep::manager_packages` / `ManagerPackages` is the Rust half. Every
script that reads the index goes through here rather than `entry.get("apt")`,
because on the table shape that expression returns a DICT and iterating it
yields release names — `noble` read as an apt package, silently.
"""

DEFAULT_KEY = "default"


def for_release(value, release=None):
    """The names that apply on `release`: its override, else the default."""
    if value is None:
        return []
    if isinstance(value, list):
        return list(value)
    if release is not None and release != DEFAULT_KEY and release in value:
        return list(value[release])
    return list(value.get(DEFAULT_KEY, []))


def all_names(value):
    """Every name the field mentions, under any release — for a gate asking
    "is this package indexed at all?"."""
    if value is None:
        return []
    if isinstance(value, list):
        return list(value)
    out = []
    for names in value.values():
        out.extend(names)
    return out


def host_release(path="/etc/os-release"):
    """`VERSION_CODENAME`, else `VERSION_ID` — mirrors `host_os_release`."""
    try:
        with open(path) as fh:
            text = fh.read()
    except OSError:
        return None
    fields = {}
    for line in text.splitlines():
        if "=" in line:
            k, v = line.split("=", 1)
            fields[k.strip()] = v.strip().strip('"')
    return fields.get("VERSION_CODENAME") or fields.get("VERSION_ID") or None


def self_test():
    ssl = {"default": ["libssl3"], "noble": ["libssl3t64"]}
    assert for_release(["x"], "noble") == ["x"]
    assert for_release(ssl, "noble") == ["libssl3t64"]
    assert for_release(ssl, "jammy") == ["libssl3"]
    assert for_release(ssl, None) == ["libssl3"]
    assert for_release({"noble": []}, "noble") == []
    assert for_release({"jammy": ["a"]}, "noble") == []
    assert sorted(all_names(ssl)) == ["libssl3", "libssl3t64"]
    assert "noble" not in all_names(ssl), "a release name is not a package"
    assert for_release(None) == [] and all_names(None) == []
