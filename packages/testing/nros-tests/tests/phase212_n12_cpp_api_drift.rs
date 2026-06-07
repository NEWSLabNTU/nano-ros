//! Phase 212.N.12 / Phase 220.B — C++ API drift lint.
//!
//! Phase 212.N.12 (commits `3d77f1349` + `1ff17e10d`) renamed several
//! `nros-cpp` symbols used by example sources:
//!
//! * `nros::EntityKind` → `nros::NodeEntityKind`
//! * `NodeEntityDescriptor::id` → `NodeEntityDescriptor::stable_id`
//! * Generated `<Service>::SERVICE_NAME` / `SERVICE_HASH` /
//!   `<Action>::ACTION_NAME` / `ACTION_HASH` constants were dropped;
//!   examples now pass plain string literals (e.g.
//!   `"example_interfaces/srv/AddTwoInts"`) for `type_name`.
//!
//! Phase 220 Track B fixed the threadx-linux cpp examples that still
//! used the retired spellings. This lint scans `examples/**/cpp/**/*.cpp`
//! for any remaining occurrences so a future N.12-shaped rename sweep
//! that misses a downstream consumer fails the test suite instead of
//! the C++ compile.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const RETIRED_NEEDLES: &[(&str, &str)] = &[
    // Symbol → reason / replacement.
    ("nros::EntityKind", "use nros::NodeEntityKind"),
    ("::EntityKind::", "use ::NodeEntityKind::"),
    (".id = ", "field renamed to stable_id"),
    (
        "::SERVICE_NAME",
        "constant retired — use plain \"pkg/srv/Name\" literal",
    ),
    ("::SERVICE_HASH", "constant retired — use \"\" literal"),
    (
        "::ACTION_NAME",
        "constant retired — use plain \"pkg/action/Name\" literal",
    ),
    ("::ACTION_HASH", "constant retired — use \"\" literal"),
];

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at packages/testing/nros-tests/.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .expect("repo root from manifest dir")
        .to_path_buf()
}

fn walk_cpp(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for ent in entries.flatten() {
        let p = ent.path();
        // Skip generated/build trees.
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if matches!(name, "build" | "generated")
            || name.starts_with("build-")
            || name.starts_with("target")
        {
            continue;
        }
        if p.is_dir() {
            walk_cpp(&p, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("cpp") {
            out.push(p);
        }
    }
}

fn compile_cpp_snippet(name: &str, source: &str) {
    let tmp = tempfile::tempdir().expect("tempdir for C++ compat snippet");
    let src = tmp.path().join(format!("{name}.cpp"));
    fs::write(&src, source).expect("write C++ compat snippet");

    let root = repo_root();
    let generated_cpp_include = root.join(
        "examples/workspaces/mixed/build-workspace-fixtures/nano_ros/packages/core/nros-cpp/include",
    );
    let generated_c_include = root.join(
        "examples/workspaces/mixed/build-workspace-fixtures/nano_ros/packages/core/nros-c/include",
    );
    let cxx = std::env::var("CXX").unwrap_or_else(|_| "c++".to_string());
    let mut command = Command::new(&cxx);
    command.arg("-std=c++14").arg("-fsyntax-only");
    if generated_cpp_include
        .join("nros/nros_cpp_config_generated.h")
        .exists()
    {
        command.arg("-I").arg(&generated_cpp_include);
    }
    if generated_c_include
        .join("nros/nros_config_generated.h")
        .exists()
    {
        command.arg("-I").arg(&generated_c_include);
    }
    let output = command
        .arg("-I")
        .arg(root.join("packages/core/nros-cpp/include"))
        .arg("-I")
        .arg(root.join("packages/core/nros-c/include"))
        .arg("-I")
        .arg(root.join("cmake/compat/include"))
        .arg(&src)
        .output()
        .unwrap_or_else(|err| panic!("spawn {cxx} for {name}: {err}"));

    assert!(
        output.status.success(),
        "C++ compat snippet `{name}` failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn examples_cpp_have_no_retired_n12_symbols() {
    let examples = repo_root().join("examples");
    assert!(
        examples.is_dir(),
        "examples/ missing at {}",
        examples.display()
    );
    let mut files = Vec::new();
    walk_cpp(&examples, &mut files);
    assert!(
        !files.is_empty(),
        "scanner found no .cpp files under examples/ — walker broken?"
    );

    let mut violations = Vec::new();
    for file in &files {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        for (lineno, line) in text.lines().enumerate() {
            // Skip the lint-test itself (this very file lives outside
            // examples/, but be defensive about other meta-files).
            // We also ignore commented-out hits.
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") {
                continue;
            }
            for (needle, hint) in RETIRED_NEEDLES {
                if line.contains(needle) {
                    violations.push(format!(
                        "{}:{}: contains retired N.12 symbol `{}` — {}",
                        file.strip_prefix(repo_root()).unwrap_or(file).display(),
                        lineno + 1,
                        needle,
                        hint,
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "retired Phase 212.N.12 C++ symbols still present in examples (\
         {} violation(s)):\n{}",
        violations.len(),
        violations.join("\n"),
    );
}

#[test]
fn declared_node_typed_helpers_compile() {
    compile_cpp_snippet(
        "declared_node_typed_helpers",
        r#"
#include <nros/node_pkg.hpp>

struct Msg {
    static constexpr const char* TYPE_NAME = "std_msgs/msg/Int32";
    static constexpr const char* TYPE_HASH = "";
};

int main() {
    nros::DeclaredNode node;
    (void)node.create_publisher<Msg>("chatter", nros::QoS::default_profile());
    (void)node.create_subscription<Msg>("/chatter", "on_message", nros::QoS::default_profile());
    return 0;
}
"#,
    );
}

#[test]
fn rclcpp_node_options_and_component_factory_compile() {
    compile_cpp_snippet(
        "rclcpp_node_options_component_factory",
        r#"
#include <nros/rclcpp_compat.hpp>

#include <memory>
#include <string>
#include <type_traits>

namespace nros_compat_component_detail {
template <typename T>
typename std::enable_if<std::is_constructible<T, rclcpp::NodeOptions>::value, std::shared_ptr<T>>::type
make_component(const char*) {
    return std::make_shared<T>(rclcpp::NodeOptions{});
}
template <typename T>
typename std::enable_if<!std::is_constructible<T, rclcpp::NodeOptions>::value && std::is_constructible<T>::value, std::shared_ptr<T>>::type
make_component(const char*) {
    return std::make_shared<T>();
}
template <typename T>
typename std::enable_if<!std::is_constructible<T, rclcpp::NodeOptions>::value && !std::is_constructible<T>::value && std::is_constructible<T, const std::string&>::value, std::shared_ptr<T>>::type
make_component(const char* name) {
    return std::make_shared<T>(std::string(name));
}
} // namespace nros_compat_component_detail

class OptionComponent : public rclcpp::Node {
  public:
    explicit OptionComponent(const rclcpp::NodeOptions& options)
        : rclcpp::Node("option_component", options) {}
};

class DefaultComponent : public rclcpp::Node {
  public:
    DefaultComponent() : rclcpp::Node("default_component") {}
};

class NameComponent : public rclcpp::Node {
  public:
    explicit NameComponent(const std::string& name) : rclcpp::Node(name) {}
};

static_assert(std::is_constructible<OptionComponent, rclcpp::NodeOptions>::value,
              "NodeOptions constructor should be available");

int main() {
    auto options = rclcpp::NodeOptions().use_intra_process_comms(true);
    OptionComponent direct(options);
    (void)direct.get_node_options().use_intra_process_comms();
    auto a = nros_compat_component_detail::make_component<OptionComponent>("option");
    auto b = nros_compat_component_detail::make_component<DefaultComponent>("default");
    auto c = nros_compat_component_detail::make_component<NameComponent>("named");
    (void)a;
    (void)b;
    (void)c;
    return 0;
}
"#,
    );
}
