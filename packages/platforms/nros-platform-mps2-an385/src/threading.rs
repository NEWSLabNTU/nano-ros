//! Threading stubs for single-threaded bare-metal MPS2-AN385.
//!
//! All threading primitives are no-ops. Task init returns an error
//! to prevent accidental thread creation.

// Threading stubs are implemented directly on Mps2An385Platform in lib.rs
// since they are all trivial no-ops returning 0 (or -1 for task_init).
