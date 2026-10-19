//! Launch remap rules as ONE const table in flash -- issue 1971, RFC-0107
//! step 1.
//!
//! The launch file's `<remap from= to=/>` rules describe the IMAGE, not one
//! executor. Before this every executor carved a RAM table of 16 rules
//! (`RemapRule`, 404 B each on thumbv7em: 6,464 B per backing) and the
//! generated C/C++ entry filled it with one `nros_cpp_declare_remap` call per
//! rule, on every tier's executor. Now codegen emits the rules once, as a
//! `static const` array of string pointers, and hands it to the runtime once
//! ([`set_launch_remaps`], `nros_cpp_set_launch_remaps`) before any tier is
//! set up. Every executor reads that table; none copies it.
//!
//! Layering is unchanged: these rules are the AUTHORITATIVE tier of
//! [`crate::names::resolve_name_layered`], and `--ros-args -r` rules (still in
//! the executor's RAM table, see `ExecutorSizing::remaps`) are the fallback.
//!
//! The Rust entry already worked this way: its launch remaps are a `&'static`
//! slice on `RuntimeCtx`, and this table is the C/C++ twin of it.

use core::ffi::{CStr, c_char};

use portable_atomic::{AtomicPtr, AtomicUsize, Ordering};

/// One launch remap rule, in the shape the generated C/C++ entry emits:
/// four NUL-terminated strings in `.rodata`.
///
/// `#[repr(C)]` so the C table (`nros_cpp_launch_remap_t`) is this struct.
/// `namespace` NULL or empty means the root namespace, as `declare_remap`
/// read it.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LaunchRemap {
    /// The owning node's name.
    pub node_name: *const c_char,
    /// The owning node's namespace (`/` for root; NULL or empty also root).
    pub namespace: *const c_char,
    /// The name as written in the source (raw; expanded at lookup).
    pub from: *const c_char,
    /// The replacement (raw; expanded at lookup).
    pub to: *const c_char,
}

// SAFETY: a `LaunchRemap` only ever points at `'static`, immutable,
// NUL-terminated strings (the constructors below, and the C ABI's contract on
// `nros_cpp_set_launch_remaps`); nothing writes through the pointers.
unsafe impl Sync for LaunchRemap {}
// SAFETY: as above.
unsafe impl Send for LaunchRemap {}

impl LaunchRemap {
    /// A rule over `'static` C strings -- the Rust spelling of one row of the
    /// generated table.
    pub const fn new(
        node_name: &'static CStr,
        namespace: &'static CStr,
        from: &'static CStr,
        to: &'static CStr,
    ) -> Self {
        Self {
            node_name: node_name.as_ptr(),
            namespace: namespace.as_ptr(),
            from: from.as_ptr(),
            to: to.as_ptr(),
        }
    }

    /// The four fields as `str`, or `None` when one is NULL (namespace
    /// excepted) or not UTF-8 -- [`set_launch_remaps`] refuses such a table,
    /// so a stored rule always reads.
    fn strs(&self) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
        // SAFETY: the type's invariant -- every non-NULL pointer is a
        // `'static` NUL-terminated string.
        let s = |p: *const c_char| -> Option<&'static str> {
            if p.is_null() {
                None
            } else {
                unsafe { CStr::from_ptr(p) }.to_str().ok()
            }
        };
        let ns = if self.namespace.is_null() {
            "/"
        } else {
            match s(self.namespace)? {
                "" => "/",
                ns => ns,
            }
        };
        Some((s(self.node_name)?, ns, s(self.from)?, s(self.to)?))
    }
}

/// Why [`set_launch_remaps`] refused a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadLaunchRemap {
    /// The row's index in the table.
    pub index: usize,
}

impl core::fmt::Display for BadLaunchRemap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "launch remap row {} has a NULL node name, `from` or `to`, or a string \
             that is not UTF-8",
            self.index
        )
    }
}

static TABLE: AtomicPtr<LaunchRemap> = AtomicPtr::new(core::ptr::null_mut());
static LEN: AtomicUsize = AtomicUsize::new(0);

/// Install the image's launch remap table. Every executor reads it from then
/// on, on every road (C and C++ registration, `Executor::create_node`'s
/// handle).
///
/// Call it ONCE, at boot, before any executor registers an entity -- the
/// generated entry calls it before the board runner sets up any tier, because
/// an rclcpp-shape constructor registers its entities immediately (issue
/// 0255). A row that does not read (NULL or non-UTF-8 string) refuses the
/// whole table and installs nothing.
pub fn set_launch_remaps(table: &'static [LaunchRemap]) -> Result<(), BadLaunchRemap> {
    if let Some(index) = table.iter().position(|r| r.strs().is_none()) {
        return Err(BadLaunchRemap { index });
    }
    // Boot-only, single writer: hide the old table, swap the pointer, then
    // publish the length, so a reader never pairs a pointer with a longer
    // length than its table.
    LEN.store(0, Ordering::Release);
    TABLE.store(table.as_ptr() as *mut LaunchRemap, Ordering::Release);
    LEN.store(table.len(), Ordering::Release);
    Ok(())
}

/// The installed launch remap table (empty until [`set_launch_remaps`]).
pub fn launch_remaps() -> &'static [LaunchRemap] {
    let len = LEN.load(Ordering::Acquire);
    let ptr = TABLE.load(Ordering::Acquire);
    if len == 0 || ptr.is_null() {
        return &[];
    }
    // SAFETY: `ptr`/`len` came from one `&'static [LaunchRemap]` in
    // `set_launch_remaps` (the length is published after the pointer).
    unsafe { core::slice::from_raw_parts(ptr, len) }
}

/// The rules of `table` that belong to the node `(node_name, namespace)`, in
/// table order, as the `(from, to)` pairs `resolve_name_layered` takes.
#[cfg_attr(not(any(has_rmw, test)), allow(dead_code))]
pub(crate) fn launch_rules_for<'r>(
    table: &'r [LaunchRemap],
    node_name: &'r str,
    namespace: &'r str,
) -> impl Iterator<Item = (&'r str, &'r str)> + 'r {
    let ns = if namespace.is_empty() { "/" } else { namespace };
    table.iter().filter_map(move |r| {
        let (n, rns, from, to) = r.strs()?;
        (n == node_name && rns == ns).then_some((from, to))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    static TABLE: [LaunchRemap; 3] = [
        LaunchRemap::new(c"talker", c"/", c"chatter", c"/wire"),
        LaunchRemap::new(c"talker", c"/ns", c"chatter", c"/other_ns"),
        LaunchRemap::new(c"listener", c"", c"chatter", c"/heard"),
    ];

    #[test]
    fn rows_are_selected_by_node_name_and_namespace() {
        let got: std::vec::Vec<_> = launch_rules_for(&TABLE, "talker", "/").collect();
        assert_eq!(got, [("chatter", "/wire")]);
        let got: std::vec::Vec<_> = launch_rules_for(&TABLE, "talker", "/ns").collect();
        assert_eq!(got, [("chatter", "/other_ns")]);
    }

    #[test]
    fn an_empty_namespace_is_the_root_on_both_sides() {
        let got: std::vec::Vec<_> = launch_rules_for(&TABLE, "listener", "").collect();
        assert_eq!(got, [("chatter", "/heard")]);
        let got: std::vec::Vec<_> = launch_rules_for(&TABLE, "listener", "/").collect();
        assert_eq!(got, [("chatter", "/heard")]);
    }

    #[test]
    fn a_row_with_a_null_string_refuses_the_whole_table() {
        static BAD: [LaunchRemap; 2] = [
            LaunchRemap::new(c"talker", c"/", c"a", c"/b"),
            LaunchRemap {
                node_name: core::ptr::null(),
                namespace: core::ptr::null(),
                from: c"a".as_ptr(),
                to: c"/b".as_ptr(),
            },
        ];
        assert_eq!(set_launch_remaps(&BAD), Err(BadLaunchRemap { index: 1 }));
    }
}
