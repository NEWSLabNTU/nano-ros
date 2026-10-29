//! The node ROLE a scaffold writes — `nros new --template <role>` (phase-517 W12).
//!
//! `ros2 pkg create` has no role flag (it writes one hello-world node behind
//! `--node-name`), so the role names come from the ROS demos a reader already
//! runs: `demo_nodes_cpp`'s `talker` / `listener`, `add_two_ints_server` /
//! `_client`, and the Fibonacci action pair. Each role uses the demo's own
//! interface, so a scaffolded node talks to the demo it is named after.
//!
//! Every template is a FILE under `templates/roles/<lang>/`, copied from the
//! in-tree role example for that language and shape, with the names replaced
//! by tokens:
//!
//! | token | meaning |
//! | --- | --- |
//! | `@NODE@` | the node's ROS name (`--node-name`; default: the package name, or for a component the role's demo node name) |
//! | `@NS@` | the C++ namespace (the package name, sanitised) |
//! | `@HEADER@` | the include path of a C++ component's own header |
//!
//! There are three shapes per language: the Rust declarative node (`lib.rs`,
//! used by every Rust scaffold), the C/C++ executable (`main.c` / `main.cpp`,
//! used by a C/C++ package on every platform but Zephyr — the in-tree role
//! mains are byte-identical across platforms), and the C/C++ typed component
//! (used by `--component` and by a Zephyr app, which the module's carrier
//! boots by identity).

use nros_lang::Language;

/// A node role. The order is the order `--help` lists them in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Talker,
    Listener,
    Server,
    Client,
    ActionServer,
    ActionClient,
}

impl Role {
    pub const ALL: [Role; 6] = [
        Role::Talker,
        Role::Listener,
        Role::Server,
        Role::Client,
        Role::ActionServer,
        Role::ActionClient,
    ];

    /// The `--template` value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Role::Talker => "talker",
            Role::Listener => "listener",
            Role::Server => "server",
            Role::Client => "client",
            Role::ActionServer => "action-server",
            Role::ActionClient => "action-client",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|r| r.name() == s)
    }

    /// The interface package the role's node uses — the `<depend>` its
    /// `package.xml` carries.
    #[must_use]
    pub fn interface_package(self) -> &'static str {
        match self {
            Role::Talker | Role::Listener => "std_msgs",
            _ => "example_interfaces",
        }
    }

    /// The node name the ROS demo of this role runs under — a component's
    /// default `--node-name` (a package's is the package name).
    #[must_use]
    pub fn demo_node_name(self) -> &'static str {
        match self {
            Role::Talker => "talker",
            Role::Listener => "listener",
            Role::Server => "add_two_ints_server",
            Role::Client => "add_two_ints_client",
            Role::ActionServer => "fibonacci_action_server",
            Role::ActionClient => "fibonacci_action_client",
        }
    }

    /// One line on what the node does, for the scaffold's summary.
    #[must_use]
    pub fn summary(self) -> &'static str {
        match self {
            Role::Talker => "publishes std_msgs/String on /chatter at 1 Hz",
            Role::Listener => "subscribes to std_msgs/String on /chatter",
            Role::Server => "serves example_interfaces/srv/AddTwoInts on /add_two_ints",
            Role::Client => "calls example_interfaces/srv/AddTwoInts on /add_two_ints",
            Role::ActionServer => "serves example_interfaces/action/Fibonacci on /fibonacci",
            Role::ActionClient => "sends a Fibonacci goal to /fibonacci",
        }
    }

    /// The node type every template of this role defines (Rust struct, C++
    /// class, and the C component's registered class name).
    #[must_use]
    pub fn class(self) -> &'static str {
        match self {
            Role::Talker => "Talker",
            Role::Listener => "Listener",
            Role::Server => "AddTwoIntsServer",
            Role::Client => "AddTwoIntsClient",
            Role::ActionServer => "FibonacciServer",
            Role::ActionClient => "FibonacciClient",
        }
    }

    /// The Rust declarative node (`src/lib.rs`).
    #[must_use]
    pub fn rust_node(self) -> &'static str {
        match self {
            Role::Talker => include_str!("../templates/roles/rust/talker.rs"),
            Role::Listener => include_str!("../templates/roles/rust/listener.rs"),
            Role::Server => include_str!("../templates/roles/rust/server.rs"),
            Role::Client => include_str!("../templates/roles/rust/client.rs"),
            Role::ActionServer => include_str!("../templates/roles/rust/action-server.rs"),
            Role::ActionClient => include_str!("../templates/roles/rust/action-client.rs"),
        }
    }

    /// The C or C++ executable (`src/main.c` / `src/main.cpp`).
    ///
    /// # Panics
    /// On [`Language::Rust`], which has no executable shape.
    #[must_use]
    pub fn main_source(self, lang: Language) -> &'static str {
        match (lang, self) {
            (Language::C, Role::Talker) => include_str!("../templates/roles/c/talker.main.c"),
            (Language::C, Role::Listener) => include_str!("../templates/roles/c/listener.main.c"),
            (Language::C, Role::Server) => include_str!("../templates/roles/c/server.main.c"),
            (Language::C, Role::Client) => include_str!("../templates/roles/c/client.main.c"),
            (Language::C, Role::ActionServer) => {
                include_str!("../templates/roles/c/action-server.main.c")
            }
            (Language::C, Role::ActionClient) => {
                include_str!("../templates/roles/c/action-client.main.c")
            }
            (Language::Cpp, Role::Talker) => include_str!("../templates/roles/cpp/talker.main.cpp"),
            (Language::Cpp, Role::Listener) => {
                include_str!("../templates/roles/cpp/listener.main.cpp")
            }
            (Language::Cpp, Role::Server) => include_str!("../templates/roles/cpp/server.main.cpp"),
            (Language::Cpp, Role::Client) => include_str!("../templates/roles/cpp/client.main.cpp"),
            (Language::Cpp, Role::ActionServer) => {
                include_str!("../templates/roles/cpp/action-server.main.cpp")
            }
            (Language::Cpp, Role::ActionClient) => {
                include_str!("../templates/roles/cpp/action-client.main.cpp")
            }
            (Language::Rust, _) => panic!("a Rust node has no executable template"),
        }
    }

    /// The C typed component source.
    #[must_use]
    pub fn c_component(self) -> &'static str {
        match self {
            Role::Talker => include_str!("../templates/roles/c/talker.component.c"),
            Role::Listener => include_str!("../templates/roles/c/listener.component.c"),
            Role::Server => include_str!("../templates/roles/c/server.component.c"),
            Role::Client => include_str!("../templates/roles/c/client.component.c"),
            Role::ActionServer => include_str!("../templates/roles/c/action-server.component.c"),
            Role::ActionClient => include_str!("../templates/roles/c/action-client.component.c"),
        }
    }

    /// The C++ typed component: `(header, source)`.
    #[must_use]
    pub fn cpp_component(self) -> (&'static str, &'static str) {
        match self {
            Role::Talker => (
                include_str!("../templates/roles/cpp/talker.component.hpp"),
                include_str!("../templates/roles/cpp/talker.component.cpp"),
            ),
            Role::Listener => (
                include_str!("../templates/roles/cpp/listener.component.hpp"),
                include_str!("../templates/roles/cpp/listener.component.cpp"),
            ),
            Role::Server => (
                include_str!("../templates/roles/cpp/server.component.hpp"),
                include_str!("../templates/roles/cpp/server.component.cpp"),
            ),
            Role::Client => (
                include_str!("../templates/roles/cpp/client.component.hpp"),
                include_str!("../templates/roles/cpp/client.component.cpp"),
            ),
            Role::ActionServer => (
                include_str!("../templates/roles/cpp/action-server.component.hpp"),
                include_str!("../templates/roles/cpp/action-server.component.cpp"),
            ),
            Role::ActionClient => (
                include_str!("../templates/roles/cpp/action-client.component.hpp"),
                include_str!("../templates/roles/cpp/action-client.component.cpp"),
            ),
        }
    }
}

/// The names a template's tokens stand for.
#[derive(Debug, Clone)]
pub struct Names<'a> {
    /// `@NODE@` — the node's ROS name.
    pub node: &'a str,
    /// `@NS@` — the C++ namespace.
    pub ns: &'a str,
    /// `@HEADER@` — the C++ component header's include path.
    pub header: &'a str,
}

/// Replace a template's tokens.
#[must_use]
pub fn render(template: &str, names: &Names<'_>) -> String {
    template
        .replace("@NODE@", names.node)
        .replace("@NS@", names.ns)
        .replace("@HEADER@", names.header)
}

/// A ROS node name (`[A-Za-z_][A-Za-z0-9_]*`). `ros2 pkg create --node-name`
/// takes the same shape, and every template spells it as a C/Rust string, a
/// component id and (for C) part of an identifier.
pub fn validate_node_name(s: &str) -> Result<(), String> {
    let mut chars = s.chars();
    let ok = match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(format!(
            "`{s}` is not a ROS node name: use letters, digits and `_`, not starting with a digit"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every template carries the node-name token exactly where the node is
    /// named, and no shape leaves a token behind after rendering.
    #[test]
    fn every_template_renders_with_no_token_left() {
        let names = Names {
            node: "my_node",
            ns: "my_pkg",
            header: "my_pkg/Talker.hpp",
        };
        for role in Role::ALL {
            let mut sources = vec![role.rust_node(), role.c_component()];
            sources.push(role.main_source(Language::C));
            sources.push(role.main_source(Language::Cpp));
            let (h, c) = role.cpp_component();
            sources.push(h);
            sources.push(c);
            for src in sources {
                let out = render(src, &names);
                for tok in ["@NODE@", "@NS@", "@HEADER@"] {
                    assert!(!out.contains(tok), "{}: {tok} left", role.name());
                }
            }
            assert!(
                role.rust_node().contains("@NODE@"),
                "{}: the Rust node does not take --node-name",
                role.name()
            );
            for lang in [Language::C, Language::Cpp] {
                assert!(
                    role.main_source(lang).contains("\"@NODE@\""),
                    "{} {lang}: the executable does not take --node-name",
                    role.name()
                );
            }
            assert!(
                role.rust_node()
                    .contains(&format!("pub struct {};", role.class())),
                "{}: Rust node type is not {}",
                role.name(),
                role.class()
            );
            let (h, _) = role.cpp_component();
            assert!(
                h.contains(&format!("class {} ", role.class()))
                    || h.contains(&format!("class {} {{", role.class())),
                "{}: C++ component class is not {}",
                role.name(),
                role.class()
            );
        }
    }

    /// Each role's sources name the interface package it depends on.
    #[test]
    fn every_template_uses_its_roles_interface() {
        for role in Role::ALL {
            let pkg = role.interface_package();
            assert!(
                role.rust_node().contains(&format!("use {pkg}::")),
                "{}",
                role.name()
            );
            assert!(
                role.main_source(Language::C).contains(pkg),
                "{}",
                role.name()
            );
            assert!(
                role.main_source(Language::Cpp).contains(pkg),
                "{}",
                role.name()
            );
            assert!(role.c_component().contains(pkg), "{}", role.name());
        }
    }

    #[test]
    fn node_names_follow_ros() {
        assert!(validate_node_name("talker_1").is_ok());
        assert!(validate_node_name("_x").is_ok());
        assert!(validate_node_name("1x").is_err());
        assert!(validate_node_name("my-node").is_err());
        assert!(validate_node_name("").is_err());
    }
}
