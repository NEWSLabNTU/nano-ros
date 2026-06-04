//! Phase 219 — codegen modules.
//!
//! Houses the in-process implementations behind the `nros codegen <…>`
//! verb family. Today this is just the Entry-pkg codegen path
//! (`nros codegen entry --lang {rust|c|cpp}` — see
//! `docs/roadmap/phase-219-cpp-entry-pkg.md` §3.2).
//!
//! Codegen verbs that pre-date Phase 219 (`nros codegen` /
//! `nros codegen cyclonedds-descriptors` / `nros codegen-system`) still
//! live in `crate::cmd` since they each have a single-file body; this
//! `codegen` module is the home for multi-file codegen libraries that
//! the CLI verb crust dispatches into.

pub mod entry;
