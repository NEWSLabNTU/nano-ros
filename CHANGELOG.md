# Changelog

All notable changes to nano-ros are recorded here. Format follows
[Keep a Changelog v1.1.0](https://keepachangelog.com/en/1.1.0/).
Versioning is per-crate semver within the workspace; entries here are
grouped by date / release window rather than a single workspace
version.

Per-crate detailed changes live in each crate's `CHANGELOG.md`
(populated as crates approach 1.0). This top-level file captures
workspace-wide changes, retirements, format-stability events, and
contributor-visible policy shifts.

## [Unreleased]

### Added

- _placeholder — populated as Phase 211 work lands_

### Changed

- _placeholder_

### Deprecated

- _placeholder_

### Removed

- _placeholder_

### Fixed

- _placeholder_

### Security

- _placeholder_

## Policy notes

- **Workspace-wide breaking changes** (RTOS API surface bumps, manifest
  schema bumps, retirements) get an entry here on the release date.
- **Per-phase milestones** (Phase 211 PoC closeout, Verus retirement
  in 211.6, wcr alpha in 211.8, Sentinel demo in 211.9) get a
  dedicated section noting the contributor-visible artifact.
- **Vendored fork bumps** in `third-party/` get an entry only when the
  fork branch advances in a way that affects in-tree builds.
- Detailed roadmap history lives in `docs/roadmap/` (active phases) and
  `docs/roadmap/archived/` (completed phases). This changelog
  cross-references those docs rather than duplicating their content.

## See also

- `docs/roadmap/` — active and archived phase plans
- `book/src/release-notes/` — _reserved for future user-facing release
  highlights_ (not populated yet)
- Per-crate `CHANGELOG.md` files — granular per-crate semver history
  (created when crates approach 1.0)
