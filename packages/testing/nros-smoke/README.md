# nros-smoke

Board and driver bringup smoke binaries that exercise no nros API
(driver validation, hello-world).

Each sub-directory is a standalone Cargo package. This directory itself
is not a Cargo workspace member; sub-crates are added to the root
workspace `members` list individually as they land.

Populated by Phase 131 Group B. See
`docs/roadmap/phase-131-examples-tree-revision.md`.
