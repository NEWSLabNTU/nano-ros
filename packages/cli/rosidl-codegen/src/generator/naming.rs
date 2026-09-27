//! One derivation of "what is this generated artifact called?".
//!
//! Every message-like artifact's name is built from the same stem —
//! `<c_package>_<kind>_<type_snake>` — carrying a suffix chosen by the language
//! surface. That rule used to be AUTHORED six times (`msg.rs`, `srv.rs`,
//! `action.rs` and three arms of `cpp.rs`), each copy differing only in a kind
//! word and an extension, with the `constant_prefix` half written three more
//! times and the intra-package include path twice inside one function. No gate
//! covered the class, so a fourth language surface would have added three more
//! copies. The kind word and the suffixes are PARAMETERS here instead.
//!
//! The formats are unchanged, deliberately: this module is a refactor, so every
//! golden stays byte-identical. See [`Surface`] for what a new surface has to
//! declare.

use crate::{types::to_c_package_name, utils::to_snake_case};

/// The language surface an artifact is emitted for.
///
/// Adding one means adding a variant plus its rows in the three `const fn`
/// tables below — not a fresh copy of the naming rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// The C surface: a `.h` header plus the `.c` translation unit beside it.
    C,
    /// The C++ surface: a `.hpp` header only. Its Rust FFI glue is named from
    /// [`ArtifactNames::stem`] by the FFI renderer, not from an extension here.
    Cpp,
}

impl Surface {
    /// Extension of the header this surface emits.
    const fn header_ext(self) -> &'static str {
        match self {
            Surface::C => "h",
            Surface::Cpp => "hpp",
        }
    }

    /// Suffix the include guard carries, so the two surfaces' guards for one
    /// type never collide.
    const fn guard_suffix(self) -> &'static str {
        match self {
            Surface::C => "_H",
            Surface::Cpp => "_HPP",
        }
    }

    /// Extension of the translation unit this surface compiles, when it has
    /// one. The C++ surface is header-only, so it has none.
    const fn source_ext(self) -> Option<&'static str> {
        match self {
            Surface::C => Some("c"),
            Surface::Cpp => None,
        }
    }
}

/// The ROS interface kind an artifact describes — the word that sits between
/// the package and the type in every generated name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A `.msg` type.
    Msg,
    /// A `.srv` type.
    Srv,
    /// An `.action` type.
    Action,
}

impl Kind {
    const fn word(self) -> &'static str {
        match self {
            Kind::Msg => "msg",
            Kind::Srv => "srv",
            Kind::Action => "action",
        }
    }
}

/// Every name one generated artifact needs, all derived from [`Self::stem`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactNames {
    /// `std_msgs_msg_int32` — the stem every other name here is built from, and
    /// also the generated struct name (the C++ surface appends `_t`) and the
    /// file stem the FFI renderer splits into `<stem>_types.rs` /
    /// `<stem>_exports.rs`.
    pub stem: String,
    /// `STD_MSGS_MSG_INT32_H` / `…_HPP` — the header's include guard.
    pub include_guard: String,
    /// `STD_MSGS_MSG_INT32` — the prefix every generated constant macro for
    /// this type carries.
    pub constant_prefix: String,
    /// `std_msgs_msg_int32.h` / `.hpp`.
    pub header: String,
    /// `std_msgs_msg_int32.c` — the translation unit, for a surface that
    /// compiles one. `None` on the header-only C++ surface.
    pub source: Option<String>,
}

/// Derive every name for one `<package>/<kind>/<type>` artifact on `surface`.
pub fn artifact_names(
    surface: Surface,
    kind: Kind,
    package_name: &str,
    type_name: &str,
) -> ArtifactNames {
    let stem = format!(
        "{}_{}_{}",
        to_c_package_name(package_name),
        kind.word(),
        to_snake_case(type_name)
    );
    let upper = stem.to_uppercase();
    ArtifactNames {
        include_guard: format!("{}{}", upper, surface.guard_suffix()),
        constant_prefix: upper,
        header: format!("{}.{}", stem, surface.header_ext()),
        source: surface.source_ext().map(|ext| format!("{}.{}", stem, ext)),
        stem,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_c_surface_names_a_message() {
        let n = artifact_names(Surface::C, Kind::Msg, "std_msgs", "Int32");
        assert_eq!(n.stem, "std_msgs_msg_int32");
        assert_eq!(n.include_guard, "STD_MSGS_MSG_INT32_H");
        assert_eq!(n.constant_prefix, "STD_MSGS_MSG_INT32");
        assert_eq!(n.header, "std_msgs_msg_int32.h");
        assert_eq!(n.source.as_deref(), Some("std_msgs_msg_int32.c"));
    }

    #[test]
    fn the_cpp_surface_shares_the_stem_and_differs_in_its_suffixes() {
        let c = artifact_names(Surface::C, Kind::Srv, "example_interfaces", "AddTwoInts");
        let cpp = artifact_names(Surface::Cpp, Kind::Srv, "example_interfaces", "AddTwoInts");
        assert_eq!(c.stem, cpp.stem);
        assert_eq!(c.constant_prefix, cpp.constant_prefix);
        assert_eq!(cpp.include_guard, "EXAMPLE_INTERFACES_SRV_ADD_TWO_INTS_HPP");
        assert_eq!(cpp.header, "example_interfaces_srv_add_two_ints.hpp");
        // Header-only: no translation unit to name.
        assert_eq!(cpp.source, None);
    }

    #[test]
    fn the_kind_word_is_the_only_difference_between_kinds() {
        for (kind, word) in [
            (Kind::Msg, "msg"),
            (Kind::Srv, "srv"),
            (Kind::Action, "action"),
        ] {
            let n = artifact_names(Surface::C, kind, "test_msgs", "Fibonacci");
            assert_eq!(n.stem, format!("test_msgs_{word}_fibonacci"));
            assert_eq!(
                n.include_guard,
                format!("TEST_MSGS_{}_FIBONACCI_H", word.to_uppercase())
            );
        }
    }

    /// A package name that is not already lower-case still uppercases as one
    /// piece — the pre-refactor copies uppercased package and type separately,
    /// and this is the case that proves the two agree.
    #[test]
    fn an_upper_case_package_uppercases_as_one_piece() {
        let n = artifact_names(Surface::C, Kind::Msg, "Weird_Pkg", "camelCase");
        assert_eq!(n.stem, "Weird_Pkg_msg_camel_case");
        assert_eq!(n.include_guard, "WEIRD_PKG_MSG_CAMEL_CASE_H");
    }
}
