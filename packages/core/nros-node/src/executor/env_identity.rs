// Issue 2739; RFC-0140 D8; user ruling 2026-11-27 on an unstated count.

//! What `$NROS_NODE_NAME` and `$NROS_NODE_NAMESPACE` do to a node.
//!
//! Both variables are read by the hosted edge (`nros::env::try_resolve_hosted`,
//! which every Linux road reaches: Rust, C's `rclc_support_init`, C++'s
//! `nros_cpp_init`). Until this module they reached only the executor's own
//! default name, so a node named in code or by the launch file kept its name
//! and the variable was read and dropped, silently. A read whose result is
//! discarded is not allowed: a value is honoured and a mismatch is warned
//! once, or it is ignored with a notice that says where to state it.
//!
//! The rules:
//!
//! - `$NROS_NODE_NAMESPACE` applies to EVERY node of the image. A namespace
//!   does not collide, so the node count does not matter. When a node stated a
//!   different non-root namespace, one warning per process says so.
//! - `$NROS_NODE_NAME` renames the node only when the image's sizing
//!   descriptor STATES that it creates exactly one
//!   ([`crate::config::STATED_NODE_COUNT`] `== Some(1)`), warning once when the
//!   stated name differs. With two or more nodes one name cannot name them all
//!   without a collision, and with an unstated count nothing here can tell, so
//!   in both cases the variable is ignored with a once-per-process notice
//!   naming where a node's name is authored.
//!
//! The values live in process-global slots because the environment is
//! process-wide and one road (C's `rclc_node_init_default`) names its node
//! before any executor exists. They are installed once, by the edge that read
//! them; nothing on an embedded target installs anything, so there every node
//! keeps its stated identity, as before.

use core::cell::UnsafeCell;
use portable_atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

/// A node name or namespace is held in 64 bytes everywhere in the executor
/// (`NodeRecord`), so a longer value could never be applied.
const CAP: usize = 64;

const EMPTY: u8 = 0;
const WRITING: u8 = 1;
const READY: u8 = 2;

/// A once-install string slot: written by the one edge that reads the
/// environment, read by every node creation after it. `no_std`, no lock.
struct Slot {
    state: AtomicU8,
    len: AtomicUsize,
    buf: UnsafeCell<[u8; CAP]>,
}

// SAFETY: `buf` is written only by the thread that won the EMPTY -> WRITING
// exchange, and read only after a READY acquire load, which orders the write
// before every read. It is never written again once READY.
unsafe impl Sync for Slot {}

impl Slot {
    const fn new() -> Self {
        Self {
            state: AtomicU8::new(EMPTY),
            len: AtomicUsize::new(0),
            buf: UnsafeCell::new([0; CAP]),
        }
    }

    /// First writer wins. `false` when `v` does not fit, so the caller can
    /// say the value was not applied rather than drop it silently.
    fn install(&self, v: &str) -> bool {
        if v.len() > CAP {
            return false;
        }
        if self
            .state
            .compare_exchange(EMPTY, WRITING, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return true;
        }
        // SAFETY: this thread won the exchange, so it is the only writer and
        // no reader looks at `buf` until READY is published below.
        unsafe { (&mut *self.buf.get())[..v.len()].copy_from_slice(v.as_bytes()) };
        self.len.store(v.len(), Ordering::Relaxed);
        self.state.store(READY, Ordering::Release);
        true
    }

    /// Test hook only (`set_for_tests`): forget the value.
    fn reset(&self) {
        self.state.store(EMPTY, Ordering::Release);
    }

    fn get(&self) -> Option<&str> {
        if self.state.load(Ordering::Acquire) != READY {
            return None;
        }
        let len = self.len.load(Ordering::Relaxed);
        // SAFETY: READY was published after the write (see `install`), and the
        // bytes are a copy of a `&str`, so they are valid UTF-8.
        let bytes = unsafe { &(&*self.buf.get())[..len] };
        core::str::from_utf8(bytes).ok()
    }
}

static ENV_NODE_NAME: Slot = Slot::new();
static ENV_NODE_NAMESPACE: Slot = Slot::new();

static WARNED_RENAME: AtomicBool = AtomicBool::new(false);
static NOTED_NAME_IGNORED: AtomicBool = AtomicBool::new(false);
static WARNED_NAMESPACE: AtomicBool = AtomicBool::new(false);

/// Test hook: `NO_OVERRIDE` reads the build's [`crate::config::STATED_NODE_COUNT`];
/// `OVERRIDE_UNSTATED` answers `None`; any other value is that count.
static COUNT_OVERRIDE: AtomicUsize = AtomicUsize::new(NO_OVERRIDE);
const NO_OVERRIDE: usize = usize::MAX;
const OVERRIDE_UNSTATED: usize = usize::MAX - 1;

/// The image's stated node count, as the rule reads it.
fn stated_count() -> Option<usize> {
    match COUNT_OVERRIDE.load(Ordering::Acquire) {
        NO_OVERRIDE => crate::config::STATED_NODE_COUNT,
        OVERRIDE_UNSTATED => None,
        n => Some(n),
    }
}

/// Test hook for the route tests (Rust here, C in `nros-c`): install `name` /
/// `namespace` as if the environment had them, and answer `count` as the
/// image's stated node count, replacing whatever a previous call installed and
/// re-arming the once-per-process messages. Never called outside tests: the
/// count of a real image is a build fact, not something to set.
#[doc(hidden)]
pub fn set_for_tests(name: Option<&str>, namespace: Option<&str>, count: Option<usize>) {
    ENV_NODE_NAME.reset();
    ENV_NODE_NAMESPACE.reset();
    install(name, namespace);
    COUNT_OVERRIDE.store(count.unwrap_or(OVERRIDE_UNSTATED), Ordering::Release);
    for f in [&WARNED_RENAME, &NOTED_NAME_IGNORED, &WARNED_NAMESPACE] {
        f.store(false, Ordering::Release);
    }
}

/// Test hook: back to "the environment said nothing", the build's own count.
#[doc(hidden)]
pub fn clear_for_tests() {
    ENV_NODE_NAME.reset();
    ENV_NODE_NAMESPACE.reset();
    COUNT_OVERRIDE.store(NO_OVERRIDE, Ordering::Release);
}

/// Record the environment's node name and namespace for this process.
///
/// Called by the hosted edge with what it read (`None` = unset or empty).
/// Only the first call installs a value; the environment does not change
/// under a running process, so a later executor resolves the same values.
pub fn install(node_name: Option<&str>, namespace: Option<&str>) {
    for (var, slot, value) in [
        ("NROS_NODE_NAME", &ENV_NODE_NAME, node_name),
        ("NROS_NODE_NAMESPACE", &ENV_NODE_NAMESPACE, namespace),
    ] {
        if let Some(v) = value
            && !slot.install(v)
        {
            nros_log::log_warn!(
                nros_log::get_logger("nros"),
                "{}={} is ignored: it is longer than {} bytes, the most a node's {} holds",
                var,
                v,
                CAP,
                if var == "NROS_NODE_NAME" {
                    "name"
                } else {
                    "namespace"
                }
            );
        }
    }
}

/// Whether this process's `$NROS_NODE_NAME` renames its node: set, and the
/// image states exactly one node.
#[must_use]
pub fn renames_node() -> bool {
    ENV_NODE_NAME.get().is_some() && stated_count() == Some(1)
}

/// Whether this process's `$NROS_NODE_NAMESPACE` replaces every node's
/// namespace.
#[must_use]
pub fn replaces_namespace() -> bool {
    ENV_NODE_NAMESPACE.get().is_some()
}

/// What the environment did to `$NROS_NODE_NAME`, for the notice and the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameRule {
    /// The variable is unset: the stated name stands.
    Unset,
    /// The image states one node: the variable names it.
    Renamed,
    /// The image states this many nodes (two or more, or zero): one name
    /// cannot name them all, so the variable is ignored.
    IgnoredCount(usize),
    /// The image does not state its node count: ignored.
    IgnoredUnstated,
}

/// One node's identity after the environment rung, and what decided it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved<'a> {
    /// The name the node is created with.
    pub name: &'a str,
    /// The namespace the node is created with.
    pub namespace: &'a str,
    /// What `$NROS_NODE_NAME` did.
    pub name_rule: NameRule,
    /// `Some(stated)` when `$NROS_NODE_NAMESPACE` replaced a node's own
    /// non-root namespace that differs from it.
    pub namespace_overrode: Option<&'a str>,
}

/// The pure rule: no globals, no logging.
#[must_use]
pub fn resolve<'a>(
    env_name: Option<&'a str>,
    env_namespace: Option<&'a str>,
    stated_count: Option<usize>,
    stated_name: &'a str,
    stated_namespace: &'a str,
) -> Resolved<'a> {
    let (name, name_rule) = match (env_name, stated_count) {
        (None, _) => (stated_name, NameRule::Unset),
        (Some(env), Some(1)) => (env, NameRule::Renamed),
        (Some(_), Some(n)) => (stated_name, NameRule::IgnoredCount(n)),
        (Some(_), None) => (stated_name, NameRule::IgnoredUnstated),
    };
    let (namespace, namespace_overrode) = match env_namespace {
        None => (stated_namespace, None),
        Some(env) => {
            let root = stated_namespace.is_empty() || stated_namespace == "/";
            let overrode = (!root && stated_namespace != env).then_some(stated_namespace);
            (env, overrode)
        }
    };
    Resolved {
        name,
        namespace,
        name_rule,
        namespace_overrode,
    }
}

/// Apply this process's environment to a node about to be created with
/// `stated_name` / `stated_namespace`, warning or noting once per process.
///
/// Every road that names a node calls this exactly where the name is fixed:
/// `NodeBuilder::build` (Rust, C++, C's `rclc_executor_node_init`) and C's
/// legacy `rclc_node_init_ex`.
#[must_use]
pub fn apply<'a>(stated_name: &'a str, stated_namespace: &'a str) -> Resolved<'a> {
    let r = resolve(
        ENV_NODE_NAME.get(),
        ENV_NODE_NAMESPACE.get(),
        stated_count(),
        stated_name,
        stated_namespace,
    );
    let logger = || nros_log::get_logger("nros");
    match r.name_rule {
        NameRule::Renamed
            if r.name != stated_name && !WARNED_RENAME.swap(true, Ordering::AcqRel) =>
        {
            nros_log::log_warn!(
                logger(),
                "NROS_NODE_NAME={} renames node `{}`, the image's only node",
                r.name,
                stated_name
            );
        }
        NameRule::IgnoredCount(n) if !NOTED_NAME_IGNORED.swap(true, Ordering::AcqRel) => {
            nros_log::log_warn!(
                logger(),
                "NROS_NODE_NAME={} is ignored: this image creates {} nodes, and one name \
                 cannot name them all. Name each node in system.toml, or with a launch remap",
                ENV_NODE_NAME.get().unwrap_or(""),
                n
            );
        }
        NameRule::IgnoredUnstated if !NOTED_NAME_IGNORED.swap(true, Ordering::AcqRel) => {
            nros_log::log_warn!(
                logger(),
                "NROS_NODE_NAME={} is ignored: this image does not state how many nodes it \
                 creates ([image] node_count, which `nros build` writes from the contract), \
                 so a rename could collide. Name the node in system.toml, or with a launch remap",
                ENV_NODE_NAME.get().unwrap_or("")
            );
        }
        _ => {}
    }
    if let Some(stated) = r.namespace_overrode
        && !WARNED_NAMESPACE.swap(true, Ordering::AcqRel)
    {
        nros_log::log_warn!(
            logger(),
            "NROS_NODE_NAMESPACE={} overrides node `{}`'s namespace `{}`",
            r.namespace,
            r.name,
            stated
        );
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_variables_keep_the_stated_identity() {
        let r = resolve(None, None, Some(1), "talker", "/ns");
        assert_eq!((r.name, r.namespace), ("talker", "/ns"));
        assert_eq!(r.name_rule, NameRule::Unset);
        assert_eq!(r.namespace_overrode, None);
    }

    #[test]
    fn one_stated_node_is_renamed() {
        let r = resolve(Some("w0node"), None, Some(1), "talker", "");
        assert_eq!(r.name, "w0node");
        assert_eq!(r.name_rule, NameRule::Renamed);
    }

    #[test]
    fn two_or_more_stated_nodes_ignore_the_name() {
        let r = resolve(Some("w0node"), None, Some(3), "talker", "");
        assert_eq!(r.name, "talker");
        assert_eq!(r.name_rule, NameRule::IgnoredCount(3));
    }

    #[test]
    fn an_unstated_count_ignores_the_name() {
        let r = resolve(Some("w0node"), None, None, "talker", "");
        assert_eq!(r.name, "talker");
        assert_eq!(r.name_rule, NameRule::IgnoredUnstated);
    }

    #[test]
    fn the_namespace_applies_whatever_the_count() {
        for count in [Some(1), Some(2), None] {
            let r = resolve(None, Some("/robot1"), count, "talker", "");
            assert_eq!(r.namespace, "/robot1", "count {count:?}");
            assert_eq!(
                r.namespace_overrode, None,
                "a root namespace overrides nothing"
            );
        }
    }

    #[test]
    fn a_differing_stated_namespace_is_reported() {
        let r = resolve(None, Some("/robot1"), None, "talker", "/fleet");
        assert_eq!(r.namespace, "/robot1");
        assert_eq!(r.namespace_overrode, Some("/fleet"));
        let same = resolve(None, Some("/robot1"), None, "talker", "/robot1");
        assert_eq!(same.namespace_overrode, None);
    }

    #[test]
    fn a_slot_installs_once_and_refuses_what_cannot_fit() {
        let s = Slot::new();
        assert_eq!(s.get(), None);
        assert!(s.install("first"));
        assert!(s.install("second"), "a later install is not an error");
        assert_eq!(s.get(), Some("first"));
        let bytes = [b'x'; CAP + 1];
        let long = core::str::from_utf8(&bytes).unwrap();
        assert!(!Slot::new().install(long));
    }
}

/// The Rust route (issue 2739 acceptance): a node created through the
/// executor, `NodeBuilder::build`, carries the environment's identity by the
/// rule. Gated like `node.rs`'s builder tests: `MockSession` is the backend.
#[cfg(all(test, feature = "alloc", not(feature = "rmw-cffi")))]
mod route_tests {
    extern crate std;
    use super::{clear_for_tests, set_for_tests};
    use crate::{executor::Executor, mock::MockSession};
    use std::sync::{Mutex, MutexGuard};

    /// The slots are process-wide: these tests take turns, and put the
    /// environment back for every other test in the binary.
    static LOCK: Mutex<()> = Mutex::new(());

    struct Env(#[allow(dead_code)] MutexGuard<'static, ()>);
    impl Env {
        fn set(name: Option<&str>, ns: Option<&str>, count: Option<usize>) -> Self {
            let g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
            set_for_tests(name, ns, count);
            Env(g)
        }
    }
    impl Drop for Env {
        fn drop(&mut self) {
            clear_for_tests();
        }
    }

    fn identity(
        exec: &Executor,
        id: crate::executor::NodeId,
    ) -> (std::string::String, std::string::String) {
        let r = exec.node(id).expect("node");
        (r.name.as_str().into(), r.namespace.as_str().into())
    }

    #[test]
    fn one_stated_node_takes_the_env_name() {
        let _e = Env::set(Some("w0node"), None, Some(1));
        let mut exec: Executor = Executor::from_session(MockSession::new());
        let id = exec.node_builder("talker").build().expect("node");
        assert_eq!(identity(&exec, id).0, "w0node");
    }

    #[test]
    fn two_stated_nodes_keep_their_names() {
        let _e = Env::set(Some("w0node"), None, Some(2));
        let mut exec: Executor = Executor::from_session(MockSession::new());
        let a = exec.node_builder("talker").build().expect("node");
        let b = exec.node_builder("listener").build().expect("node");
        assert_eq!(identity(&exec, a).0, "talker");
        assert_eq!(identity(&exec, b).0, "listener");
    }

    #[test]
    fn an_unstated_count_keeps_the_name() {
        let _e = Env::set(Some("w0node"), None, None);
        let mut exec: Executor = Executor::from_session(MockSession::new());
        let id = exec.node_builder("talker").build().expect("node");
        assert_eq!(identity(&exec, id).0, "talker");
    }

    #[test]
    fn the_env_namespace_reaches_a_node_with_its_own() {
        let _e = Env::set(None, Some("/w0ns2"), Some(2));
        let mut exec: Executor = Executor::from_session(MockSession::new());
        let a = exec
            .node_builder("talker")
            .namespace("/fleet")
            .build()
            .expect("node");
        let b = exec.node_builder("listener").build().expect("node");
        assert_eq!(identity(&exec, a), ("talker".into(), "/w0ns2".into()));
        assert_eq!(identity(&exec, b), ("listener".into(), "/w0ns2".into()));
    }

    /// A launch rule is keyed by the STATED identity, so it still applies to a
    /// node the environment renamed and re-namespaced.
    #[test]
    fn a_launch_remap_keyed_by_the_stated_identity_still_applies() {
        let _e = Env::set(Some("w0node"), Some("/w0ns2"), Some(1));
        let mut exec: Executor = Executor::from_session(MockSession::new());
        if exec
            .declare_remap("talker", "/", "chatter", "/remapped")
            .is_err()
        {
            // A build with no runtime remap table has nothing to check here.
            return;
        }
        let id = exec.node_builder("talker").build().expect("node");
        assert_eq!(identity(&exec, id), ("w0node".into(), "/w0ns2".into()));
        let r = exec
            .resolve_entity_name_on(Some(id), "chatter")
            .expect("resolve");
        assert_eq!(r.as_str(), "/remapped");
    }
}
