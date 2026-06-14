use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use nros_cli_core::orchestration::{
    generate::{GenerateOptions, generate_package},
    plan::NrosPlan,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("orchestration")
        .join(name)
}

fn temp_output(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nros_cli_core_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn generate_fixture(name: &str, plan_fixture: &str) -> PathBuf {
    let output_dir = temp_output(name);
    generate_plan(name, fixture(plan_fixture), output_dir.clone());
    output_dir
}

fn generate_plan(name: &str, plan_path: PathBuf, output_dir: PathBuf) {
    // Phase 195.C — point `nros_path` at the hermetic fixture workspace so
    // `profile()` resolves boards from its bundled `packages/boards` without
    // the nano-ros superproject present (the CLI ships from a separate repo).
    let root = fixture_workspace();
    generate_package(&GenerateOptions {
        package_name: "nros-generated-test".to_string(),
        output_dir,
        plan_path,
        nros_path: root.join("packages/core/nros"),
        nros_orchestration_path: root.join("packages/core/nros-orchestration"),
        component_workspace: None,
    })
    .unwrap_or_else(|error| panic!("{name} generated package writes: {error:?}"));
}

/// Hermetic fixture workspace bundled in the crate: carries
/// `packages/boards/*/nros-board.toml` (Phase 195.C) so board resolution works
/// without the real nano-ros `packages/boards`.
fn fixture_workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/board-workspace")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root ancestor")
        .to_path_buf()
}

fn generate_workspace_backed_fixture(name: &str, plan_fixture: &str) -> PathBuf {
    let output_dir = temp_output(name);
    let root = workspace_root();
    generate_package(&GenerateOptions {
        package_name: "nros-generated-test".to_string(),
        output_dir: output_dir.clone(),
        plan_path: fixture(plan_fixture),
        nros_path: root.join("packages/core/nros"),
        nros_orchestration_path: root.join("packages/core/nros-orchestration"),
        component_workspace: None,
    })
    .unwrap_or_else(|error| panic!("{name} generated package writes: {error:?}"));
    output_dir
}

#[test]
fn generated_package_writes_manifest_build_script_and_main() {
    let output_dir = generate_fixture("generated_package_writes_files", "plan_pub_sub.json");

    let cargo_toml = fs::read_to_string(output_dir.join("Cargo.toml")).expect("read Cargo.toml");
    assert!(cargo_toml.contains("name = \"nros-generated-test\""));
    // Phase 126.M4 — per-RMW `rmw-*-cffi` feature names dropped (Phase
    // 128.C); generator now emits a single `nros/rmw-cffi` umbrella.
    // Phase 248 C5c — the `nros` umbrella is platform-agnostic: no
    // `nros/platform-posix` feature. Posix `nros_platform_*` symbols come
    // from the `nros-platform-cffi[posix-c-port]` dep (asserted below).
    assert!(
        cargo_toml
            .contains("default = [\"std\", \"nros/rmw-cffi\", \"nros-orchestration/rmw-cffi\"]")
    );
    // Phase 195.C — `nros_path` is now the real workspace (so board descriptors
    // resolve), so match the dep-path suffix rather than a fixed prefix.
    assert!(cargo_toml.contains("nros = { path = \""));
    assert!(cargo_toml.contains("packages/core/nros\""));
    assert!(cargo_toml.contains("packages/core/nros-orchestration\""));
    assert!(cargo_toml.contains("packages/core/nros-platform-cffi\", default-features = false, features = [\"posix-c-port\"] }"));
    // No dependency on the CLI crate itself (the fixture-workspace path happens
    // to contain "nros-cli-core", so match the dependency form, not the bare name).
    assert!(!cargo_toml.contains("nros-cli-core = "));
    assert!(!cargo_toml.contains("serde_json = "));

    let build_rs = fs::read_to_string(output_dir.join("build.rs")).expect("read build.rs");
    assert!(build_rs.contains("const PLAN_PATH: &str ="));
    assert!(build_rs.contains("// Generated from: "));
    assert!(build_rs.contains("pub const CALLBACK_COUNT: usize = 2;"));
    assert!(build_rs.contains("pub const SCHED_CONTEXT_COUNT: usize = 1;"));
    assert!(build_rs.contains("pub static COMPONENTS: [ComponentSpec; 2]"));
    assert!(build_rs.contains("pub static INSTANCES: [InstanceSpec; 2]"));
    assert!(build_rs.contains("pub static NODES: [NodeSpec; 2]"));
    assert!(build_rs.contains("pub static PARAMETERS: [ParameterSpec; 1]"));
    assert!(build_rs.contains("pub static SCHED_CONTEXTS: [SchedContextSpec; 1]"));
    assert!(build_rs.contains("pub static CALLBACK_BINDINGS: [CallbackBindingSpec; 2]"));
    assert!(build_rs.contains("pub static SYSTEM: SystemSpec"));
    assert!(build_rs.contains("GeneratedNodeRuntime"));
    assert!(build_rs.contains("register_component::<demo_nodes_rs::talker::Component>"));
    assert!(build_rs.contains("register_component::<demo_nodes_rs::listener::Component>"));
    assert!(build_rs.contains("PlanId("));
    assert!(build_rs.contains("SchedClassSpec::Fifo"));
    assert!(build_rs.contains("PrioritySpec::BestEffort"));
    assert!(build_rs.contains("deadline_policy: DeadlinePolicySpec::Activated"));
    assert!(build_rs.contains("pub fn register_backends()"));
    assert!(build_rs.contains("nros_rmw_zenoh::register()"));
    assert!(build_rs.contains("instantiate_callback_handles"));
    assert!(build_rs.contains("handles.set("));
    assert!(!build_rs.contains("serde_json"));
    assert!(!build_rs.contains("nros_cli_core"));

    // Phase 172 WP-B — the wiring moved into the entry lib, so it lives in the
    // generated `nros_generated` module (emitted into build.rs), not main.rs.
    assert!(build_rs.contains("pub fn build_executor("));
    assert!(build_rs.contains("pub fn register_all("));
    // W.5.6 — std build with a rust executable component: the tick machinery is
    // emitted and a per-instance tick entry is registered. The component's `tick`
    // is driven each spin via `run_tick_loop` between dispatch.
    assert!(build_rs.contains("static TICK_ENTRIES:"));
    assert!(build_rs.contains("impl nros::ActionExecutor for GenActionExec"));
    // M-F.4.a — the codegen-side `ClientDispatch` impl ships beside
    // `GenActionExec` and is wired into the per-instance tick closure as the
    // third `TickCtx::new` argument (the substrate frozen in nros's
    // `d15565efe` made `TickCtx::new` 3-arg).
    assert!(build_rs.contains("impl nros::component::ClientDispatch for GenClientDispatch"));
    assert!(build_rs.contains("struct GenClientDispatch<"));
    assert!(build_rs.contains("nros::TickCtx::new("));
    assert!(build_rs.contains("pub fn run_tick_loop("));
    assert!(build_rs.contains("executor.spin_once("));
    assert!(build_rs.contains("as nros::ExecutableComponent>::tick("));
    assert!(build_rs.contains("TICK_ENTRIES.with("));

    // Phase 172 WP-B — native/posix `self` is now the compiled-form entry lib:
    // a thin `src/main.rs` shim over the lib's `build_executor` + `register_all`.
    let main_rs = fs::read_to_string(output_dir.join("src/main.rs")).expect("read main.rs");
    assert!(main_rs.contains("fn main() -> core::result::Result<(), nros::NodeError> {"));
    assert!(main_rs.contains(
        "use nros_generated_test::{SYSTEM, build_executor, register_all, run_tick_loop};"
    ));
    assert!(main_rs.contains("ExecutorConfig::from_env()"));
    assert!(main_rs.contains("build_executor(&config)?"));
    assert!(main_rs.contains("register_all(&mut executor)?"));
    // W.5.6 — the std arm spins via the manual tick loop (not plain spin_blocking).
    assert!(main_rs.contains("return run_tick_loop(&mut executor);"));
    assert!(!main_rs.contains("spin_blocking(SpinOptions::default())"));
    // The wiring lives in the lib now, not main.
    assert!(!main_rs.contains("nros_generated::register_backends();"));
    assert!(!main_rs.contains("#![no_std]"));
    assert!(!main_rs.contains("::run("));

    // Phase 172 WP-B — src/lib.rs hosts the wiring + the `nros_<sys>_*` C ABI;
    // Cargo.toml is a standalone `lib` + `staticlib` crate.
    let lib_rs = fs::read_to_string(output_dir.join("src/lib.rs")).expect("read lib.rs");
    // The entry lib re-exports the wiring the self shim / board shim needs
    // (TRANSPORT_LOCATOR rides along for the baked locator).
    assert!(lib_rs.contains(
        "pub use nros_generated::{SYSTEM, TRANSPORT_LOCATOR, build_executor, register_all, run_tick_loop};"
    ));
    // C ABI symbol prefix is the system name (`demo_system`), not the crate.
    assert!(lib_rs.contains("pub extern \"C\" fn nros_demo_system_build_executor("));
    assert!(lib_rs.contains("pub extern \"C\" fn nros_demo_system_register_all("));
    assert!(cargo_toml.contains("[workspace]"));
    assert!(cargo_toml.contains("crate-type = [\"lib\", \"staticlib\"]"));
    assert!(output_dir.join("include/demo_system.h").is_file());
}

/// Phase 172 W.5.3 + W.5.7 — a rust component's timer callback generates a real
/// `ExecutableComponent::on_callback` dispatch (not the noop `|| {}`). On a std
/// target the instance shares one `State` across its callbacks via the
/// per-instance `Rc<RefCell>` shared prelude (`Resolveri0`/`state_i0`); the
/// resolver is keyed on the *source* entity id and dispatch uses the *source*
/// callback name wrapped as a product-facing `Callback`.
#[test]
fn executable_timer_emits_on_callback_dispatch() {
    let output_dir = generate_fixture(
        "executable_timer_emits_on_callback_dispatch",
        "plan_pub_sub.json",
    );
    let build_rs = fs::read_to_string(output_dir.join("build.rs")).expect("read build.rs");

    // Real dispatch into the component body (replaces the noop tick).
    assert!(
        build_rs.contains("as nros::ExecutableComponent>::on_callback"),
        "timer dispatches the component body:\n{build_rs}"
    );
    assert!(build_rs.contains("nros::CallbackCtx::new"));
    assert!(build_rs.contains("as nros::ExecutableComponent>::init()"));
    // W.5.7 — per-instance shared state: one `Rc<RefCell<State>>` cloned into the
    // callback closure, borrowed mutably on dispatch.
    assert!(
        build_rs.contains("::std::rc::Rc::new(::core::cell::RefCell::new(")
            && build_rs.contains("let state_i0 ="),
        "instance state shared via Rc<RefCell>:\n{build_rs}"
    );
    assert!(build_rs.contains("::std::rc::Rc::clone(&state_i0)"));
    assert!(build_rs.contains(".borrow_mut()"));
    // Generated resolver keyed on the SOURCE entity id (`pub_chatter`), not the
    // plan-prefixed `talker_1/pub_chatter`.
    assert!(
        build_rs.contains("impl nros::PublisherResolver for Resolveri0")
            && build_rs.contains("\\\"pub_chatter\\\" =>"),
        "resolver keyed on source entity id:\n{build_rs}"
    );
    assert!(
        !build_rs.contains("\\\"talker_1/pub_chatter\\\" =>"),
        "must not key on the plan-prefixed entity id"
    );
    // Publisher created via the builder; dispatch uses the SOURCE callback name.
    assert!(build_rs.contains(".publisher(\\\"/chatter\\\").generic("));
    assert!(
        build_rs.contains("nros::Callback::__from_id(nros::CallbackId::new(\\\"cb_timer\\\"))")
    );
    assert!(
        !build_rs.contains(
            "nros::Callback::__from_id(nros::CallbackId::new(\\\"talker_1/cb_timer\\\"))"
        )
    );
}

#[test]
fn generated_package_features_follow_rtos_plan() {
    let root = temp_output("generated_package_features_follow_rtos_plan");
    fs::create_dir_all(&root).expect("create temp plan dir");
    let plan_path = root.join("nros-plan.json");
    let plan = include_str!("fixtures/orchestration/plan_pub_sub.json")
        .replace(
            "\"target\": \"x86_64-unknown-linux-gnu\"",
            "\"target\": \"thumbv7em-none-eabihf\"",
        )
        .replace("\"board\": \"native\"", "\"board\": \"zephyr\"")
        .replace("\"rmw\": \"zenoh\"", "\"rmw\": \"xrce\"")
        .replace("\"rmw-zenoh\"", "\"rmw-xrce\"");
    fs::write(&plan_path, plan).expect("write RTOS plan");

    let output_dir = root.join("generated");
    generate_plan(
        "generated_package_features_follow_rtos_plan",
        plan_path,
        output_dir.clone(),
    );
    let cargo_toml = fs::read_to_string(output_dir.join("Cargo.toml")).expect("read Cargo.toml");

    // Phase 248 C5c — the `nros` umbrella is platform-agnostic: the default
    // set keeps only the local `platform-zephyr` alias (gating the generated
    // pkg's platform deps/cfg) + the rmw-cffi vtable; `nros/platform-zephyr`
    // is gone. The concrete platform now comes from a direct `nros-platform`
    // dep (asserted below).
    assert!(cargo_toml.contains(
        "default = [\"platform-zephyr\", \"nros/rmw-cffi\", \"nros-orchestration/rmw-cffi\"]"
    ));
    assert!(cargo_toml.contains("nros-platform = { path = \""));
    assert!(cargo_toml.contains("packages/core/nros-platform\", default-features = false, features = [\"platform-zephyr\"] }"));
    assert!(!cargo_toml.contains("\"std\""));
    assert!(!cargo_toml.contains("platform-posix"));
    assert!(!cargo_toml.contains("nros-platform-cffi"));
}

#[test]
fn declared_serial_transport_selects_board_feature() {
    // Phase 173.5 — a `[[transport]]` entry drives the board crate's
    // transport feature. A bare-metal board + a single serial transport
    // ⇒ the board dep disables defaults and selects `serial` (swapping
    // off the board's default `ethernet`).
    let root = temp_output("declared_serial_transport_selects_board_feature");
    fs::create_dir_all(&root).expect("create temp plan dir");
    let plan_path = root.join("nros-plan.json");
    let plan = include_str!("fixtures/orchestration/plan_pub_sub.json")
        .replace(
            "\"target\": \"x86_64-unknown-linux-gnu\"",
            "\"target\": \"thumbv7m-none-eabi\"",
        )
        .replace("\"board\": \"native\"", "\"board\": \"baremetal\"")
        .replace(
            "\"cfg\": {}",
            "\"cfg\": {}, \"transports\": [{ \"kind\": \"serial\", \"device\": \"UART0\", \"baudrate\": 115200, \"locator\": \"serial/UART0#baudrate=115200\" }]",
        );
    fs::write(&plan_path, plan).expect("write transport plan");

    let output_dir = root.join("generated");
    generate_plan(
        "declared_serial_transport_selects_board_feature",
        plan_path,
        output_dir.clone(),
    );

    let cargo_toml = fs::read_to_string(output_dir.join("Cargo.toml")).expect("read Cargo.toml");
    // Phase 248 C5b — the declared RMW (`zenoh`) now lowers to the board's
    // `rmw-zenoh` feature, appended after the transport feature.
    assert!(
        cargo_toml.contains(
            "packages/boards/nros-board-mps2-an385\", default-features = false, features = [\"serial\", \"rmw-zenoh\"] }"
        ),
        "serial transport selects the board `serial` feature with defaults off + the declared rmw feature:\n{cargo_toml}"
    );

    // Phase 173.5 — the transport `locator` becomes the generated
    // TRANSPORT_LOCATOR const, and the board entry prefers it over the
    // board Config default.
    // build.rs embeds the generated tables as an escaped string
    // literal, so match the const name + locator value as substrings.
    let build_rs = fs::read_to_string(output_dir.join("build.rs")).expect("read build.rs");
    assert!(
        build_rs.contains("TRANSPORT_LOCATOR") && build_rs.contains("serial/UART0#baudrate=115200"),
        "transport locator emitted as const:\n{build_rs}"
    );
    let main_rs = fs::read_to_string(output_dir.join("src/main.rs")).expect("read main.rs");
    // Phase 172 flip — the board shim `use`s the entry lib, so TRANSPORT_LOCATOR
    // is imported (no `nros_generated::` prefix).
    assert!(
        main_rs.contains("TRANSPORT_LOCATOR.unwrap_or(board_config.zenoh_locator)"),
        "board shim prefers the transport locator:\n{main_rs}"
    );

    // Phase 173.5 — NanoRosOwned: the serial baudrate lands in the board
    // `Config` via apply_transport_config, which the board entry calls on
    // a Config::default() before run().
    assert!(
        build_rs.contains("apply_transport_config") && build_rs.contains("set_baudrate(115200)"),
        "baudrate written into board Config:\n{build_rs}"
    );
    // Phase 172 flip — the board shim calls the entry lib's apply hook through
    // the lib crate path.
    assert!(
        main_rs.contains("nros_generated_test::apply_transport_config(&mut cfg)"),
        "board shim applies the transport Config override:\n{main_rs}"
    );
}

#[test]
fn multi_homed_interfaces_emit_set_interfaces_call() {
    // Phase 172.K.7 — an ethernet `[[transport]]` with an `interfaces` list
    // bakes a `set_interfaces(&["eth0", "eth1"])` call into the board Config
    // override (the seam a multi-homed board / Cyclone `<Interfaces>` reads).
    let root = temp_output("multi_homed_interfaces_emit_set_interfaces_call");
    fs::create_dir_all(&root).expect("create temp plan dir");
    let plan_path = root.join("nros-plan.json");
    let plan = include_str!("fixtures/orchestration/plan_pub_sub.json")
        .replace(
            "\"target\": \"x86_64-unknown-linux-gnu\"",
            "\"target\": \"thumbv7m-none-eabi\"",
        )
        .replace("\"board\": \"native\"", "\"board\": \"baremetal\"")
        .replace(
            "\"cfg\": {}",
            "\"cfg\": {}, \"transports\": [{ \"kind\": \"ethernet\", \"ip\": \"10.0.2.50/24\", \"interfaces\": [\"eth0\", \"eth1\"] }]",
        );
    fs::write(&plan_path, plan).expect("write transport plan");

    let output_dir = root.join("generated");
    generate_plan(
        "multi_homed_interfaces_emit_set_interfaces_call",
        plan_path,
        output_dir.clone(),
    );

    let build_rs = fs::read_to_string(output_dir.join("build.rs")).expect("read build.rs");
    assert!(
        build_rs.contains("apply_transport_config")
            && build_rs.contains("set_interfaces(&[\\\"eth0\\\", \\\"eth1\\\"])"),
        "interfaces list written into board Config:\n{build_rs}"
    );
}

#[test]
fn bridge_two_transports_emit_open_multi_and_session_specs() {
    // Phase 173.5 — two `[[transport]]` entries (each with its own rmw)
    // put the build in bridge mode: both RMW deps are emitted, a
    // SESSION_SPECS array is generated, and the entry opens via
    // Executor::open_multi instead of Executor::open.
    let root = temp_output("bridge_two_transports");
    fs::create_dir_all(&root).expect("create temp plan dir");
    let plan_path = root.join("nros-plan.json");
    let plan = include_str!("fixtures/orchestration/plan_pub_sub.json").replace(
        "\"cfg\": {}",
        "\"cfg\": {}, \"transports\": [\
            { \"kind\": \"ethernet\", \"ip\": \"dhcp\", \"rmw\": \"zenoh\", \"locator\": \"tcp/10.0.2.2:7447\" },\
            { \"kind\": \"serial\", \"device\": \"UART0\", \"baudrate\": 115200, \"rmw\": \"cyclonedds\" }\
        ]",
    );
    fs::write(&plan_path, plan).expect("write bridge plan");

    let output_dir = root.join("generated");
    generate_plan("bridge_two_transports", plan_path, output_dir.clone());

    // Both RMW backends are linked.
    let cargo_toml = fs::read_to_string(output_dir.join("Cargo.toml")).expect("read Cargo.toml");
    assert!(
        cargo_toml.contains("nros-rmw-zenoh ="),
        "zenoh backend dep emitted:\n{cargo_toml}"
    );
    assert!(
        cargo_toml.contains("Cyclone DDS is a CMake/C++ project"),
        "cyclonedds backend slot noted:\n{cargo_toml}"
    );

    // SESSION_SPECS array + per-transport specs in the generated tables.
    let build_rs = fs::read_to_string(output_dir.join("build.rs")).expect("read build.rs");
    assert!(
        build_rs.contains("SESSION_SPECS"),
        "session specs:\n{build_rs}"
    );
    assert!(
        build_rs.contains("SessionSpec::new(\\\"zenoh\\\"")
            && build_rs.contains("SessionSpec::new(\\\"cyclonedds\\\""),
        "per-transport specs:\n{build_rs}"
    );

    // Phase 172 flip — open_multi lives in the entry lib's
    // `build_executor_bridge`; the thin shim `use`s + calls it (no
    // `nros_generated::` prefix, no `run_system_bridge` helper).
    assert!(
        build_rs.contains("Executor::open_multi(&SESSION_SPECS)"),
        "build_executor_bridge opens via open_multi:\n{build_rs}"
    );
    let main_rs = fs::read_to_string(output_dir.join("src/main.rs")).expect("read main.rs");
    assert!(
        main_rs.contains("build_executor_bridge()?")
            && main_rs.contains("register_all(&mut executor)?"),
        "bridge shim routes through build_executor_bridge:\n{main_rs}"
    );
}

/// Verifies multi-domain nodes emit one session per domain and route by session index.
#[test]
fn multi_domain_emit_session_routes() {
    // Phase 172.K.5 — two nodes on distinct ROS domains put the (non-bridge)
    // build in multi-domain mode: a SESSION_SPECS array with one spec per
    // distinct domain (same rmw), open_multi via build_executor_bridge, NODES
    // carry their domain, and build_component_node routes each node to the
    // session whose domain matches.
    let root = temp_output("multi_domain");
    fs::create_dir_all(&root).expect("create temp plan dir");
    let mut plan: NrosPlan = serde_json::from_str(include_str!(
        "fixtures/orchestration/plan_multi_instance.json"
    ))
    .expect("parse multi-instance plan");
    // Assign the two nodes to distinct domains (0 and 5).
    let mut domain = [0u32, 5u32].into_iter();
    for instance in &mut plan.instances {
        for node in &mut instance.nodes {
            node.domain_id = Some(domain.next().unwrap_or(5));
        }
    }
    let plan_path = root.join("nros-plan.json");
    fs::write(&plan_path, serde_json::to_string_pretty(&plan).unwrap())
        .expect("write multi-domain plan");

    let output_dir = root.join("generated");
    generate_plan("multi_domain", plan_path, output_dir.clone());

    let build_rs = fs::read_to_string(output_dir.join("build.rs")).expect("read build.rs");
    // One SessionSpec per distinct domain (0 and 5), same rmw + locator.
    assert!(
        build_rs.contains("SESSION_SPECS"),
        "session specs:\n{build_rs}"
    );
    assert!(
        build_rs.contains(".domain_id(0)") && build_rs.contains(".domain_id(5)"),
        "one spec per distinct domain:\n{build_rs}"
    );
    // NODES carry their assigned domain (not the old hardcoded None).
    assert!(
        build_rs.contains("domain_id: Some(0)") && build_rs.contains("domain_id: Some(5)"),
        "nodes carry their domain:\n{build_rs}"
    );
    // build_component_node routes each node to the session matching its domain.
    assert!(
        build_rs.contains("SESSION_SPECS.iter().position(|s| s.domain_id == domain_id)")
            && build_rs.contains(".session_idx(session_idx)"),
        "per-node session routing:\n{build_rs}"
    );
    assert!(
        build_rs.contains("Executor::open_multi(&SESSION_SPECS)"),
        "build_executor_bridge opens via open_multi:\n{build_rs}"
    );
    // Hosted shim opens the multi-session executor.
    let main_rs = fs::read_to_string(output_dir.join("src/main.rs")).expect("read main.rs");
    assert!(
        main_rs.contains("build_executor_bridge()?"),
        "multi-domain shim routes through build_executor_bridge:\n{main_rs}"
    );
}

#[test]
fn generated_package_wires_freertos_entry() {
    let root = temp_output("generated_package_wires_freertos_entry");
    fs::create_dir_all(&root).expect("create temp plan dir");
    let plan_path = root.join("nros-plan.json");
    let plan = include_str!("fixtures/orchestration/plan_pub_sub.json")
        .replace(
            "\"target\": \"x86_64-unknown-linux-gnu\"",
            "\"target\": \"thumbv7m-none-eabi\"",
        )
        .replace("\"board\": \"native\"", "\"board\": \"freertos\"");
    fs::write(&plan_path, plan).expect("write FreeRTOS plan");

    let output_dir = root.join("generated");
    generate_plan(
        "generated_package_wires_freertos_entry",
        plan_path,
        output_dir.clone(),
    );

    let cargo_toml = fs::read_to_string(output_dir.join("Cargo.toml")).expect("read Cargo.toml");
    // Phase 248 C5c-platform — board-driven platform selection: the entry deps
    // the `nros-board-mps2-an385-freertos` crate, which brings
    // `nros-platform/platform-freertos` directly, so the umbrella `nros` dep no
    // longer carries `nros/platform-freertos`. The `platform-freertos` local
    // alias (the generated entry pkg's own feature) stays.
    assert!(cargo_toml.contains(
        "default = [\"platform-freertos\", \"nros/rmw-cffi\", \"nros-orchestration/rmw-cffi\"]"
    ));
    assert!(cargo_toml.contains("nros-board-mps2-an385-freertos"));
    assert!(cargo_toml.contains("panic-semihosting"));

    let cargo_config =
        fs::read_to_string(output_dir.join(".cargo/config.toml")).expect("read cargo config");
    assert!(cargo_config.contains("[target.thumbv7m-none-eabi]"));
    assert!(cargo_config.contains("mps2_an385.ld"));

    let main_rs = fs::read_to_string(output_dir.join("src/main.rs")).expect("read main.rs");
    // Phase 173.2b collapsed the per-platform `#[cfg(feature = ...)]` entry
    // blocks into one shape chosen by `profile().board_entry`. FreeRTOS is a
    // bare-metal `BoardRun`, so the generated `main.rs` is unconditional
    // `#![no_std]` / `#![no_main]` with a single `_start` entry that drives
    // the board crate's `run()` — no `cfg(feature = "platform-freertos")`
    // gate survives.
    assert!(main_rs.contains("#![no_std]"));
    assert!(main_rs.contains("#![no_main]"));
    assert!(main_rs.contains("use panic_semihosting as _;"));
    assert!(main_rs.contains("extern \"C\" fn _start() -> !"));
    assert!(main_rs.contains("nros_board_mps2_an385_freertos::run("));
    assert!(main_rs.contains("nros_board_mps2_an385_freertos::Config::default()"));
    // Single shape: no other platform's entry leaks in.
    assert!(!main_rs.contains("ExecutorConfig::from_env()"));
    assert!(!main_rs.contains("esp_hal::main"));
}

#[test]
fn generated_package_registers_service_and_action_callbacks() {
    let output_dir = generate_fixture(
        "generated_package_registers_service_and_action_callbacks",
        "plan_service_action.json",
    );
    let build_rs = fs::read_to_string(output_dir.join("build.rs")).expect("read build.rs");

    assert!(build_rs.contains("pub const CALLBACK_COUNT: usize = 2;"));
    // W.5.3/.5 — rust executable component on a std target: service + action emit
    // real dispatch (trampolines reading a Box::leak'd ctx), not the C-fn-ptr noops.
    assert!(build_rs.contains("svc_tramp_"));
    assert!(build_rs.contains("nros::CallbackCtx::with_reply"));
    assert!(build_rs.contains("goal_tramp_"));
    assert!(build_rs.contains("cancel_tramp_"));
    assert!(build_rs.contains("nros::CallbackCtx::with_goal_decision"));
    assert!(build_rs.contains("nros::CallbackCtx::with_cancel_decision"));
    assert!(build_rs.contains("as nros::ExecutableComponent>::on_callback"));
    assert!(build_rs.contains("register_service_raw_sized_on::<1024, 1024>"));
    assert!(build_rs.contains("register_action_server_raw_sized::<1024, 1024, 1024, 4>"));
    // Accepted stays the noop until the W.5.6 tick hook drives execution.
    assert!(build_rs.contains("noop_raw_accepted"));
    assert!(build_rs.contains("qos: nros::QosSettings::services_default()"));
    assert!(build_rs.contains(".handle_id()"));
    assert!(!build_rs.contains("unsupported generated callback"));
}

/// Phase 172 W.5.8 — the same rust executable service + action component on a
/// **no_std** target (freertos) dispatches real bodies via a function-local
/// `static mut` context (no `Box::leak`/`Rc`/alloc), read through `addr_of_mut!`.
/// The std-only tick machinery (`TICK_ENTRIES` / `run_tick_loop`) is absent.
#[test]
fn generated_no_std_service_action_uses_static_context() {
    let output_dir = generate_fixture(
        "generated_no_std_service_action_uses_static_context",
        "plan_service_action_freertos.json",
    );
    let build_rs = fs::read_to_string(output_dir.join("build.rs")).expect("read build.rs");

    // Real dispatch — same trampolines + decision sinks as the std path.
    assert!(build_rs.contains("svc_tramp_"));
    assert!(build_rs.contains("goal_tramp_"));
    assert!(build_rs.contains("cancel_tramp_"));
    assert!(build_rs.contains("nros::CallbackCtx::with_reply"));
    assert!(build_rs.contains("nros::CallbackCtx::with_goal_decision"));
    assert!(build_rs.contains("as nros::ExecutableComponent>::on_callback"));
    // The context lives in a `static mut`, read via addr_of_mut! (service is
    // fn-local; the action's is module-level so the W.5.11 tick can reach it).
    assert!(build_rs.contains("static mut SVC_CTX_"));
    assert!(build_rs.contains("static mut ACT_CTX_"));
    assert!(build_rs.contains("core::ptr::addr_of_mut!(SVC_CTX_"));
    assert!(build_rs.contains("core::ptr::addr_of_mut!(ACT_CTX_"));
    assert!(build_rs.contains("register_service_raw_sized_on::<1024, 1024>"));
    assert!(build_rs.contains("register_action_server_raw_sized::<1024, 1024, 1024, 4>"));
    // W.5.11 — no_std action execution: per-action `static mut` handle + `tick_`
    // fn + `GenActionExec`, driven by `run_tick_loop_nostd` (infinite spin_once +
    // tick, no halt); the no_std self shim spins via it.
    assert!(build_rs.contains("static mut ACT_HANDLE_"));
    assert!(build_rs.contains("fn tick_"));
    assert!(build_rs.contains("impl nros::ActionExecutor for GenActionExec"));
    assert!(build_rs.contains("pub fn run_tick_loop_nostd("));
    // No std-only mechanisms: no Box::leak ctx, no Rc shared state, no
    // thread_local tick registry, no std `run_tick_loop`.
    assert!(!build_rs.contains("::std::boxed::Box::into_raw"));
    assert!(!build_rs.contains("::std::rc::Rc::new"));
    assert!(!build_rs.contains("TICK_ENTRIES"));
    assert!(!build_rs.contains("thread_local!"));
    assert!(!build_rs.contains("unsupported generated callback"));
}

/// Verifies a generated service/action package is readable by Cargo metadata.
#[test]
fn generated_service_action_readable_by_cargo() {
    let output_dir = generate_workspace_backed_fixture(
        "generated_service_action_package_is_readable_by_cargo_metadata",
        "plan_service_action.json",
    );
    let manifest_path = output_dir.join("Cargo.toml");

    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .arg("--no-deps")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .output()
        .expect("run cargo metadata for generated service/action package");

    assert!(
        output.status.success(),
        "cargo metadata failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn generated_package_is_readable_by_cargo_metadata() {
    let output_dir =
        generate_workspace_backed_fixture("generated_package_cargo_metadata", "plan_pub_sub.json");
    let manifest_path = output_dir.join("Cargo.toml");

    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .arg("--no-deps")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .output()
        .expect("run cargo metadata for generated package");

    assert!(
        output.status.success(),
        "cargo metadata failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"name\":\"nros-generated-test\""));
    assert!(stdout.contains("\"src_path\""));
}

#[test]
fn generated_package_output_is_stable() {
    let output_dir = generate_fixture("generated_package_output_is_stable", "plan_pub_sub.json");
    let first_cargo = fs::read_to_string(output_dir.join("Cargo.toml")).expect("read Cargo.toml");
    let first_build = fs::read_to_string(output_dir.join("build.rs")).expect("read build.rs");
    let first_main = fs::read_to_string(output_dir.join("src/main.rs")).expect("read main.rs");

    let root = fixture_workspace();
    generate_package(&GenerateOptions {
        package_name: "nros-generated-test".to_string(),
        output_dir: output_dir.clone(),
        plan_path: fixture("plan_pub_sub.json"),
        nros_path: root.join("packages/core/nros"),
        nros_orchestration_path: root.join("packages/core/nros-orchestration"),
        component_workspace: None,
    })
    .expect("second generated package write");

    assert_eq!(
        first_cargo,
        fs::read_to_string(output_dir.join("Cargo.toml")).expect("reread Cargo.toml")
    );
    assert_eq!(
        first_build,
        fs::read_to_string(output_dir.join("build.rs")).expect("reread build.rs")
    );
    assert_eq!(
        first_main,
        fs::read_to_string(output_dir.join("src/main.rs")).expect("reread main.rs")
    );
}

#[test]
fn generated_tables_cover_multiple_instances_of_same_component() {
    let output_dir = generate_fixture(
        "generated_tables_multi_instance",
        "plan_multi_instance.json",
    );
    let build_rs = fs::read_to_string(output_dir.join("build.rs")).expect("read build.rs");

    assert!(build_rs.contains("pub const CALLBACK_COUNT: usize = 2;"));
    assert!(build_rs.contains("pub static COMPONENTS: [ComponentSpec; 1]"));
    assert!(build_rs.contains("pub static INSTANCES: [InstanceSpec; 2]"));
    assert!(build_rs.contains("pub static PARAMETERS: [ParameterSpec; 2]"));
    assert!(build_rs.contains("left_talker"));
    assert!(build_rs.contains("right_talker"));
    assert!(build_rs.contains("/left/talker"));
    assert!(build_rs.contains("/right/talker"));
    assert!(build_rs.contains("parameter_start: 0, parameter_len: 1"));
    assert!(build_rs.contains("parameter_start: 1, parameter_len: 1"));
    assert!(build_rs.contains("value: ParameterValue::I64(5)"));
    assert!(build_rs.contains("value: ParameterValue::I64(2)"));
    assert!(build_rs.contains("CallbackBindingSpec { callback_index: 0, sched_context_index: 1 }"));
    assert!(build_rs.contains("CallbackBindingSpec { callback_index: 1, sched_context_index: 1 }"));
}

// Phase 173.6 — changing one nros.toml transport line re-generates a
// working build with zero hand edits. Generate a bare-metal package
// with `ethernet`, then the same plan with `serial`, and assert the
// only delta is the board's transport feature (the board crate path,
// other deps, and the entry are byte-identical).
#[test]
fn one_transport_line_change_reflows_only_the_board_feature() {
    fn gen_with_transport(tag: &str, kind: &str) -> String {
        let root = temp_output(tag);
        fs::create_dir_all(&root).expect("create temp plan dir");
        let plan_path = root.join("nros-plan.json");
        let plan = include_str!("fixtures/orchestration/plan_pub_sub.json")
            .replace(
                "\"target\": \"x86_64-unknown-linux-gnu\"",
                "\"target\": \"thumbv7m-none-eabi\"",
            )
            .replace("\"board\": \"native\"", "\"board\": \"baremetal\"")
            .replace(
                "\"cfg\": {}",
                &format!("\"cfg\": {{}}, \"transports\": [{{ \"kind\": \"{kind}\" }}]"),
            );
        fs::write(&plan_path, plan).expect("write plan");
        let output_dir = root.join("generated");
        generate_plan(tag, plan_path, output_dir.clone());
        fs::read_to_string(output_dir.join("Cargo.toml")).expect("read Cargo.toml")
    }

    let eth = gen_with_transport("reflow_ethernet", "ethernet");
    let ser = gen_with_transport("reflow_serial", "serial");

    // Phase 248 C5b — the declared RMW (`zenoh`) lowers to the board's
    // `rmw-zenoh` feature, appended after the transport feature (identical in
    // both variants, so the only delta remains the transport token).
    assert!(eth.contains("default-features = false, features = [\"ethernet\", \"rmw-zenoh\"]"));
    assert!(ser.contains("default-features = false, features = [\"serial\", \"rmw-zenoh\"]"));

    // Everything except the board feature is identical: the diff is the
    // single `["ethernet"]`/`["serial"]` transport token. Normalise that token
    // and assert the rest matches — proving no other manifest edit is needed.
    let eth_norm = eth.replace(
        "[\"ethernet\", \"rmw-zenoh\"]",
        "[\"<transport>\", \"rmw-zenoh\"]",
    );
    let ser_norm = ser.replace(
        "[\"serial\", \"rmw-zenoh\"]",
        "[\"<transport>\", \"rmw-zenoh\"]",
    );
    assert_eq!(
        eth_norm, ser_norm,
        "ethernet vs serial manifests differ only in the transport feature"
    );
}

// Phase 173.7 — negative gate: nano-ros never emits kernel params. The
// net fragment nano-ros appends to the Zephyr base prj.conf must be
// net-only — no tick / heap / stack / scheduler / pthread knobs (those
// are the board's, untouched).
#[test]
fn generator_emits_no_kernel_params_in_net_fragment() {
    let root = temp_output("no_kernel_params_fragment");
    fs::create_dir_all(&root).expect("create temp plan dir");
    let plan_path = root.join("nros-plan.json");
    let plan = include_str!("fixtures/orchestration/plan_pub_sub.json")
        .replace(
            "\"target\": \"x86_64-unknown-linux-gnu\"",
            "\"target\": \"thumbv7em-none-eabihf\"",
        )
        .replace("\"board\": \"native\"", "\"board\": \"zephyr\"")
        .replace(
            "\"cfg\": {}",
            "\"cfg\": {}, \"transports\": [{ \"kind\": \"ethernet\", \"ip\": \"10.0.2.50/24\" }]",
        );
    fs::write(&plan_path, plan).expect("write zephyr plan");
    let output_dir = root.join("generated");
    generate_plan("no_kernel_params_fragment", plan_path, output_dir.clone());

    let prj = fs::read_to_string(output_dir.join("prj.conf")).expect("read prj.conf");
    // Isolate the nano-ros-added fragment (everything after the marker).
    let fragment = prj
        .split("Phase 173.7 — net config")
        .nth(1)
        .expect("net fragment present");
    assert!(fragment.contains("CONFIG_NET_CONFIG_MY_IPV4_ADDR=\"10.0.2.50\""));
    for forbidden in [
        "CLOCK",
        "HEAP",
        "STACK",
        "SCHED",
        "PTHREAD",
        "TICKS_PER_SEC",
        "CONFIG_MAIN",
    ] {
        assert!(
            !fragment.contains(forbidden),
            "net fragment must not set kernel param `{forbidden}`:\n{fragment}"
        );
    }
}

/// Phase 173.6 — grep gate: the generated package bakes in no network
/// constant of its own. Every transport/IP/locator value must come from
/// either the board `Config::default` (board-intrinsic, in the board
/// crate — not the generated package) or `nros.toml`. A zero-config
/// (no `[[transport]]`) package therefore must contain no IPv4 literal
/// and no `tcp/`/`serial/` locator anywhere in its generated sources.
#[test]
fn zero_config_package_hardcodes_no_network_constants() {
    /// True if `s` contains a `d.d.d.d` IPv4-looking literal.
    fn contains_ipv4(s: &str) -> bool {
        let bytes = s.as_bytes();
        // Slide a window; count dot-separated all-digit groups.
        for start in 0..bytes.len() {
            let mut i = start;
            let mut groups = 0;
            loop {
                let g0 = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i == g0 {
                    break; // no digits → not an octet
                }
                groups += 1;
                if groups == 4 {
                    return true;
                }
                if i < bytes.len() && bytes[i] == b'.' {
                    i += 1; // consume separator, continue
                } else {
                    break;
                }
            }
        }
        false
    }

    let output_dir = generate_fixture("zero_config_no_net_constants", "plan_pub_sub.json");
    for file in ["src/main.rs", "build.rs"] {
        let text = fs::read_to_string(output_dir.join(file)).expect("read generated file");
        assert!(
            !contains_ipv4(&text),
            "{file} hardcodes an IPv4 literal (should come from board Config / nros.toml):\n{text}"
        );
        assert!(
            !text.contains("tcp/") && !text.contains("serial/"),
            "{file} hardcodes a locator (should come from board Config / nros.toml):\n{text}"
        );
    }
}

/// Phase 173.6 — esp32-s3 (Xtensa) is a single `profile()` row: the
/// generator emits the espup `esp` channel rust-toolchain.toml + the
/// xtensa target in .cargo/config.toml, with zero edits to the
/// (collapsed) per-platform render arms.
#[test]
fn esp32s3_selects_esp_toolchain_and_xtensa_target() {
    let root = temp_output("esp32s3_esp_toolchain");
    fs::create_dir_all(&root).expect("create temp plan dir");
    let plan_path = root.join("nros-plan.json");
    let plan = include_str!("fixtures/orchestration/plan_pub_sub.json")
        .replace(
            "\"target\": \"x86_64-unknown-linux-gnu\"",
            "\"target\": \"xtensa-esp32s3-none-elf\"",
        )
        .replace("\"board\": \"native\"", "\"board\": \"esp32s3\"");
    fs::write(&plan_path, plan).expect("write esp32s3 plan");
    let output_dir = root.join("generated");
    generate_plan("esp32s3_esp_toolchain", plan_path, output_dir.clone());

    let toolchain =
        fs::read_to_string(output_dir.join("rust-toolchain.toml")).expect("rust-toolchain.toml");
    assert!(
        toolchain.contains("channel = \"esp\""),
        "esp32-s3 selects the espup esp channel:\n{toolchain}"
    );
    let cargo_config =
        fs::read_to_string(output_dir.join(".cargo/config.toml")).expect("cargo config");
    assert!(
        cargo_config.contains("target = \"xtensa-esp32s3-none-elf\""),
        "esp32-s3 targets xtensa:\n{cargo_config}"
    );

    // The chip selects the S3 board crate + esp-hal esp32s3 features, and
    // the entry runs through it — all from the single profile() row.
    let cargo_toml = fs::read_to_string(output_dir.join("Cargo.toml")).expect("Cargo.toml");
    assert!(
        cargo_toml.contains("nros-board-esp32s3 =")
            && cargo_toml.contains("features = [\"esp32s3\", \"unstable\"]"),
        "esp32-s3 board crate + esp-hal s3 features:\n{cargo_toml}"
    );
    let main_rs = fs::read_to_string(output_dir.join("src/main.rs")).expect("main.rs");
    assert!(
        main_rs.contains("nros_board_esp32s3::run"),
        "esp32-s3 entry runs through the S3 board:\n{main_rs}"
    );
}

/// Phase 172.A — a plan with no `[lifecycle]` block emits the `apply_lifecycle`
/// hook as a no-op (so the build needs no `lifecycle-services` feature) and
/// `run_executor` still calls it.
#[test]
fn generated_package_emits_noop_lifecycle_when_unmanaged() {
    let output_dir = generate_fixture("lifecycle_unmanaged", "plan_pub_sub.json");

    let cargo_toml = fs::read_to_string(output_dir.join("Cargo.toml")).expect("read Cargo.toml");
    assert!(
        !cargo_toml.contains("lifecycle-services"),
        "unmanaged plan must not enable lifecycle-services:\n{cargo_toml}"
    );

    let build_rs = fs::read_to_string(output_dir.join("build.rs")).expect("read build.rs");
    assert!(build_rs.contains("pub fn apply_lifecycle"));
    assert!(
        build_rs.contains("let _ = executor;"),
        "unmanaged apply_lifecycle is a no-op:\n{build_rs}"
    );
    assert!(!build_rs.contains("register_lifecycle_services"));
    // Phase 172 WP-B — apply_lifecycle is now invoked from register_all (in the
    // entry lib), which the self shim calls.
    assert!(
        build_rs.contains("apply_lifecycle(executor)?;"),
        "register_all invokes apply_lifecycle:\n{build_rs}"
    );

    let main_rs = fs::read_to_string(output_dir.join("src/main.rs")).expect("read main.rs");
    assert!(main_rs.contains("register_all(&mut executor)?"));
}

/// Phase 172.A — a `[lifecycle] autostart = "active"` plan registers the
/// REP-2002 services + drives configure→activate at boot, and enables the
/// `nros/lifecycle-services` feature on the generated crate.
#[test]
fn generated_package_wires_lifecycle_when_managed() {
    use nros_cli_core::orchestration::plan::{LifecycleAutostart, NrosPlan, PlanLifecycle};

    // Mark the pub/sub fixture plan lifecycle-managed (active) + write to temp.
    let mut plan: NrosPlan =
        serde_json::from_str(&fs::read_to_string(fixture("plan_pub_sub.json")).expect("read plan"))
            .expect("parse plan");
    plan.lifecycle = Some(PlanLifecycle {
        autostart: LifecycleAutostart::Active,
    });
    let base = temp_output("lifecycle_managed");
    fs::create_dir_all(&base).expect("create temp base");
    let plan_path = base.join("nros-plan.json");
    fs::write(&plan_path, serde_json::to_string_pretty(&plan).unwrap()).expect("write plan");

    let output_dir = base.join("out");
    generate_plan("lifecycle_managed", plan_path, output_dir.clone());

    let cargo_toml = fs::read_to_string(output_dir.join("Cargo.toml")).expect("read Cargo.toml");
    assert!(
        cargo_toml.contains("nros/lifecycle-services"),
        "managed plan enables lifecycle-services:\n{cargo_toml}"
    );

    let build_rs = fs::read_to_string(output_dir.join("build.rs")).expect("read build.rs");
    assert!(build_rs.contains("executor.register_lifecycle_services()?;"));
    assert!(build_rs.contains("nros::LifecycleTransition::Configure"));
    assert!(build_rs.contains("nros::LifecycleTransition::Activate"));
}

/// Phase 212.M-F.4.a — a rust executable component with a service-client +
/// action-client entity on a std target emits the `GenClientDispatch` runtime
/// `ClientDispatch` impl alongside `GenActionExec`, registers the client handles
/// inline, and feeds them into the per-instance tick closure (the substrate
/// `TickCtx::new` is now 3-arg). Mirrors the W.5.6 action-server tick path.
#[test]
fn generated_service_client_emits_gen_client_dispatch() {
    let output_dir = generate_fixture(
        "generated_service_client_emits_gen_client_dispatch",
        "plan_service_client.json",
    );
    let build_rs = fs::read_to_string(output_dir.join("build.rs")).expect("read build.rs");

    // The codegen-side `ClientDispatch` impl + struct are emitted.
    assert!(build_rs.contains("struct GenClientDispatch<"));
    assert!(build_rs.contains("impl nros::component::ClientDispatch for GenClientDispatch"));
    // The service-client handle is registered through the executor (Phase 82
    // arena-backed raw client). Reply buffer size matches the action-server
    // path (1024) for symmetry.
    assert!(build_rs.contains("register_service_client_raw_sized_on::<1024>"));
    // The action-client handle is registered via the `RawActionClientSpec`
    // shape — same buffer sizes as `register_action_client_raw`.
    assert!(build_rs.contains("register_action_client_raw_sized::<1024, 1024, 1024>"));
    assert!(build_rs.contains("nros::RawActionClientSpec {"));
    // The per-instance tick closure captures both client arrays + constructs
    // `GenClientDispatch` over them. Stable entity-id keys (`cli_reset`,
    // `cli_count`) make the array entries auditable.
    assert!(build_rs.contains("__tick_sclients_i"));
    assert!(build_rs.contains("__tick_aclients_i"));
    // Keys are emitted into the build.rs `GENERATED_TABLES: &str` constant, so
    // the entity-id literals appear escaped in the host file (`\"cli_reset\"`).
    assert!(build_rs.contains(r#"\"cli_reset\""#));
    assert!(build_rs.contains(r#"\"cli_count\""#));
    // `TickCtx::new` is called with the 3-arg shape (pub resolver, action
    // executor, client dispatch) — the substrate signature post-M-F.4.
    assert!(build_rs.contains("nros::TickCtx::new("));
    // No `unsupported generated callback` — the timer callback dispatch path
    // still wires up normally.
    assert!(!build_rs.contains("unsupported generated callback"));
}
