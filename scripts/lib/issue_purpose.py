"""The `purpose:` frontmatter of an adoption-probe stop (RFC-0116 D9).

One reader for `scripts/issues.py` and `scripts/gen-issue-index.py`. A stop
names the purpose that hit it, or a list when several probes hit the same stop
(`purpose: [industrial, education]`, phase-523 W3), so it counts once for each.
"""

PURPOSES = ("industrial", "field", "education")


def purposes(raw):
    """The purposes a raw `purpose:` value names, in order, as a list.

    `""` (no field) is `[]`; a scalar is one; a YAML flow list is several.
    """
    raw = (raw or "").strip()
    if raw.startswith("[") and raw.endswith("]"):
        raw = raw[1:-1]
    return [p.strip().strip("'\"") for p in raw.split(",") if p.strip()]


if __name__ == "__main__":
    assert purposes("") == []
    assert purposes("field") == ["field"]
    assert purposes("[industrial, education]") == ["industrial", "education"]
    assert purposes("['field']") == ["field"]
    print("issue_purpose: ok")
