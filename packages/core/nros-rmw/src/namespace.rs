//! The ROS root namespace has ONE spelling across the RMW seam: `/`.
//!
//! An entity that states no namespace arrives as `""`, and a node the
//! executor announces in the root arrives as `/`. They are the same node, so
//! every table that keys or compares on a namespace asks this helper first.

// Issue 2947: the C-ABI shim's node table kept `("talker", "")` and
// `("talker", "/")` apart, so a one-node image refused the node's `/rosout`
// writer. The normalisation had been spelled by hand at a dozen sites, and
// the one that lacked it was the one that broke.

/// The root namespace, in the one spelling every table stores.
pub const ROOT_NAMESPACE: &str = "/";

/// `namespace` in the spelling every table stores: `""` becomes
/// [`ROOT_NAMESPACE`], anything else is returned unchanged.
#[inline]
pub const fn canonical_namespace(namespace: &str) -> &str {
    if namespace.is_empty() {
        ROOT_NAMESPACE
    } else {
        namespace
    }
}

/// Do `a` and `b` name the same namespace? `""` and `/` are both the root.
#[inline]
pub fn same_namespace(a: &str, b: &str) -> bool {
    canonical_namespace(a) == canonical_namespace(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_root_has_one_spelling() {
        assert_eq!(canonical_namespace(""), "/");
        assert_eq!(canonical_namespace("/"), "/");
        assert_eq!(canonical_namespace("/robot"), "/robot");
        assert!(same_namespace("", "/"));
        assert!(!same_namespace("", "/robot"));
    }
}
