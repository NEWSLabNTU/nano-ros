# One-shot migration recipes

Scripts that performed a past bulk edit and are kept as the record of how it was
done. Nothing calls them; no lane runs them; they target tree layouts that may no
longer exist, so read one before re-running it.

| script | did |
| --- | --- |
| `migrate-app-main.py` | phase-112 C.3 — moved 115 C/C++ example sources to the new app-main contract |
| `migrate-rustapp.py` | phase-112 F.2 — renamed 20 Zephyr Rust examples to `nros_zephyr_<rmw>_<usecase>` |
| `collapse-*-case.sh` | phase-118 B — collapsed `examples/<plat>/<lang>/<rmw>/<case>` siblings into `<plat>/<lang>/<case>` |

They lived in `tmp/` until 2026-10-24, which made `tmp/` both disposable scratch
and committed history, and `rm -rf tmp` deleted them (issue 1524). `tmp/` now
holds no tracked file.
