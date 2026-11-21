//! Issue 2812 -- what the runtime's own service families cost on the platform
//! heap, PER NODE, beyond the one node a port's heap was measured on.
//!
//! Every node that serves the PARAMETER services runs six servers, and every
//! LIFECYCLE state machine runs five. Each server owns a request/reply pair of
//! `PARAM_SERVICE_BUFFER_SIZE` bytes (nros-node's `config`;
//! `LIFECYCLE_SERVICE_BUFFER_SIZE` is the same constant) on the platform
//! heap. A port whose heap default was measured on a one-node image therefore
//! carries one node's worth, and needs, for each further node or machine:
//!
//! ```text
//! extra = (param_nodes - 1) x (6 x 2 x buffer + param_remainder)
//!       + (machines    - 1) x (5 x 2 x buffer + lifecycle_remainder)
//! ```
//!
//! The remainder is what one more node costs beyond its buffer pairs, measured
//! per port. A count of 0 or 1 adds nothing, so an image with one node and one
//! machine (or none) derives exactly the heap it had before this term existed.
//!
//! This is the Rust twin of `cmake/NanoRosServiceHeap.cmake`, which the cmake
//! roads (Zephyr's heap floor, the FreeRTOS and ThreadX cmake roads) use. A
//! test below reads that file, so the two cannot drift apart.

use crate::SizingDescriptor;

/// Servers one parameter-serving node registers.
pub const PARAM_SERVICES_PER_NODE: usize = 6;
/// Servers one lifecycle state machine registers.
pub const LIFECYCLE_SERVICES_PER_MACHINE: usize = 5;
/// nros-node's default `PARAM_SERVICE_BUFFER_SIZE`, which every in-tree image
/// that does not state one runs with.
pub const DEFAULT_SERVICE_BUFFER: usize = 4096;

/// The heap a port needs beyond its one-node measurement for `param_nodes`
/// parameter-serving nodes and `machines` lifecycle state machines, each
/// server owning a `buffer`-byte request/reply pair.
///
/// Each count is floored at one: zero means the family is off, which costs
/// nothing beyond the measured image, never a negative term.
#[must_use]
pub const fn extra_bytes(
    param_nodes: usize,
    machines: usize,
    buffer: usize,
    param_remainder: usize,
    lifecycle_remainder: usize,
) -> usize {
    let more_nodes = param_nodes.saturating_sub(1);
    let more_machines = machines.saturating_sub(1);
    more_nodes * (PARAM_SERVICES_PER_NODE * 2 * buffer + param_remainder)
        + more_machines * (LIFECYCLE_SERVICES_PER_MACHINE * 2 * buffer + lifecycle_remainder)
}

/// The two counts an image's descriptor states: `(param_nodes, machines)`.
///
/// `param_nodes` is `[image] param_service_nodes`, zero when absent. `machines`
/// is [`crate::infra::lifecycle_machines`] over `[image] infra_families` and
/// `[image] lifecycle_nodes`, the rule the queryable pool prices the lifecycle
/// family with. No descriptor states nothing: `(0, 0)`, no per-node term.
#[must_use]
pub fn described_counts(desc: Option<&SizingDescriptor>) -> (usize, usize) {
    let Some(desc) = desc else {
        return (0, 0);
    };
    let param_nodes = desc
        .image
        .param_service_nodes()
        .stated()
        .copied()
        .unwrap_or(0);
    let families = desc.image.infra_families().stated().map(|f| f.tag());
    let lifecycle_nodes = desc.image.lifecycle_nodes().stated().map(usize::to_string);
    // An image that states no families says nothing about lifecycle either:
    // its one machine (if any) is the measured one.
    let machines = if families.is_some() {
        crate::infra::lifecycle_machines(families, lifecycle_nodes.as_deref())
    } else {
        0
    };
    (param_nodes, machines)
}

/// [`extra_bytes`] for the image a build script is building: the counts from
/// the descriptor [`crate::from_build_env`] names, at nros-node's default
/// buffer, with the calling port's measured remainders.
///
/// The default buffer, because `NROS_EXECUTOR_PARAM_SERVICE_BUFFER_BYTES` has one reader
/// (nros-node's build script) and every in-tree image runs the default; a
/// stated smaller buffer over-provisions by the difference, never under.
///
/// # Panics
/// On a descriptor the build names but that does not parse, as every
/// consumer of [`crate::from_build_env`] does.
#[must_use]
pub fn extra_bytes_from_build_env(param_remainder: usize, lifecycle_remainder: usize) -> usize {
    let desc = crate::from_build_env().unwrap_or_else(|e| panic!("{e}"));
    let (param_nodes, machines) = described_counts(desc.as_ref());
    extra_bytes(
        param_nodes,
        machines,
        DEFAULT_SERVICE_BUFFER,
        param_remainder,
        lifecycle_remainder,
    )
}

/// The integer a cmake file's first `set(<name> <n>)` gives `name`, plain or
/// `CACHE INTERNAL`. Reads a cmake TEXT, never the environment: the drift
/// tests that hold this module and each port's remainders to their cmake
/// twins call it on an `include_str!`.
pub fn cmake_cache_value(cmake: &str, name: &str) -> Option<usize> {
    cmake.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("set(")?.strip_prefix(name)?;
        // `set(NAME 6)` or `set(NAME 6 CACHE INTERNAL "...")`; never a longer
        // name that starts with this one.
        if !rest.starts_with(char::is_whitespace) {
            return None;
        }
        rest.split_whitespace()
            .next()?
            .trim_end_matches(')')
            .parse()
            .ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_node_and_one_machine_add_nothing() {
        for (n, m) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert_eq!(
                extra_bytes(n, m, DEFAULT_SERVICE_BUFFER, 999, 999),
                0,
                "{n} {m}"
            );
        }
    }

    #[test]
    fn each_further_node_and_machine_adds_its_buffer_pairs_and_remainder() {
        // Four parameter nodes: three more, twelve buffers each.
        assert_eq!(extra_bytes(4, 1, 4096, 0, 0), 3 * 12 * 4096);
        assert_eq!(extra_bytes(4, 1, 4096, 100, 0), 3 * (12 * 4096 + 100));
        // Three machines: two more, ten buffers each.
        assert_eq!(extra_bytes(1, 3, 4096, 0, 50), 2 * (10 * 4096 + 50));
        assert_eq!(extra_bytes(2, 2, 1024, 0, 0), 12 * 1024 + 10 * 1024);
    }

    /// `cmake/NanoRosServiceHeap.cmake` is the same formula for the cmake
    /// roads: its constants must be these, and its function must agree on
    /// every (nodes, machines) pair the drift check tries.
    #[test]
    fn the_cmake_twin_states_the_same_constants() {
        let cmake = include_str!("../../../../cmake/NanoRosServiceHeap.cmake");
        let cmake_set = |name: &str| -> usize {
            cmake_cache_value(cmake, name)
                .unwrap_or_else(|| panic!("NanoRosServiceHeap.cmake sets no `{name}`"))
        };
        assert_eq!(
            cmake_set("NROS_PARAM_SERVICES_PER_NODE"),
            PARAM_SERVICES_PER_NODE
        );
        assert_eq!(
            cmake_set("NROS_LIFECYCLE_SERVICES_PER_MACHINE"),
            LIFECYCLE_SERVICES_PER_MACHINE
        );
        assert_eq!(
            cmake_set("NROS_PARAM_SERVICE_DEFAULT_BUFFER"),
            DEFAULT_SERVICE_BUFFER
        );
    }
}
