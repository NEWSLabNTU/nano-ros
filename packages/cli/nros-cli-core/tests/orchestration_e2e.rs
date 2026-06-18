use std::{
    fs,
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nros_cli_core::{
    cmd::{check, metadata, plan},
    orchestration::{
        build::{BuildOptions, build_generated_package},
        generate::{GenerateOptions, generate_package},
        metadata_build::{MetadataBuildOptions, build_metadata},
        plan::{NrosPlan, PlanComponent, PlanEntity, PlanParamPersistence},
        schema::ParameterValue,
        source_metadata::SourceMetadata,
    },
};
use serde_json::Value;

#[test]
fn fixture_workspace_plans_checks_and_builds_generated_package() {
    let fixture = fixture_workspace();
    let output = temp_output("orchestration_e2e");
    let out_dir = output.join("build/e2e_system/nros");
    let generated_dir = out_dir.join("generated");
    let demo_pkg = fixture.join("src/demo_pkg");

    metadata::run(metadata::Args {
        system_pkg: "e2e_system".to_string(),
        workspace: Some(fixture.clone()),
        out_dir: Some(out_dir.clone()),
        metadata: vec![fixture.join("artifacts/talker.metadata.json")],
        build: false,
        nano_ros_workspace: None,
    })
    .expect("metadata command preserves fixture source metadata");

    plan::run(plan::Args {
        system_pkg: "e2e_system".to_string(),
        launch_file: demo_pkg.join("launch/system.launch.xml"),
        record: None,
        file: None,
        exec: None,
        workspace: Some(fixture.clone()),
        out_dir: Some(out_dir.clone()),
        metadata: Vec::new(),
        manifests: vec![demo_pkg.join("manifest/system.launch.yaml")],
        launch_args: Vec::new(),
        rmw: None,
        target: None,
    })
    .expect("plan command parses launch and writes checked artifacts");

    let plan_path = out_dir.join("nros-plan.json");
    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated plan");

    let plan: NrosPlan =
        serde_json::from_str(&fs::read_to_string(&plan_path).expect("read generated plan"))
            .expect("generated plan has canonical schema");
    assert_eq!(plan.system, "e2e_system");
    assert_eq!(plan.instances.len(), 1);
    assert_eq!(plan.instances[0].package, "demo_pkg");
    assert_eq!(plan.instances[0].parameters[0].name, "rate_hz");
    assert_eq!(
        plan.instances[0].parameters[0].value,
        ParameterValue::Integer(25)
    );

    let record: Value = serde_json::from_str(
        &fs::read_to_string(out_dir.join("record.json")).expect("read record"),
    )
    .expect("record is JSON");
    let nodes = record["node"].as_array().expect("record has node array");
    assert_eq!(nodes[0]["package"].as_str(), Some("demo_pkg"));
    assert_eq!(nodes[0]["executable"].as_str(), Some("talker"));

    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated".to_string(),
        output_dir: generated_dir.clone(),
        plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture),
        release: false,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command compiles generated package");

    assert!(generated_dir.join("Cargo.toml").is_file());
    assert!(generated_dir.join("src/main.rs").is_file());
    // Phase 172.D — a successful generation drops a staleness stamp so the
    // next build with an unchanged plan + generator skips regeneration.
    assert!(
        generated_dir.join(".nros-build-stamp").is_file(),
        "172.D build stamp written after generation"
    );
    for lang in ["rust", "c", "cpp"] {
        let manifest_path = out_dir.join("interfaces").join(lang).join("manifest.json");
        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(&manifest_path)
                .unwrap_or_else(|error| panic!("read {}: {error}", manifest_path.display())),
        )
        .unwrap_or_else(|error| panic!("parse {}: {error}", manifest_path.display()));
        assert_eq!(
            manifest["schema"].as_str(),
            Some("nano-ros/interface-cache/v1")
        );
        assert_eq!(manifest["system"].as_str(), Some("e2e_system"));
        assert_eq!(
            manifest["interfaces"][0]["id"].as_str(),
            Some("std_msgs/msg/String")
        );
    }

    let binary = out_dir
        .join("target")
        .join(&plan.build.target)
        .join("debug")
        .join("nros-e2e-generated");
    assert!(
        binary.is_file(),
        "generated binary exists at {}",
        binary.display()
    );

    // Phase 172 WP-B — the compiled-form entry lib: a staticlib + C header
    // ship alongside the self-shim binary, exporting the `nros_<sys>_*` C ABI.
    let staticlib = out_dir
        .join("target")
        .join(&plan.build.target)
        .join("debug")
        .join("libnros_e2e_generated.a");
    assert!(
        staticlib.is_file(),
        "entry-lib staticlib at {}",
        staticlib.display()
    );
    let header = generated_dir.join("include/e2e_system.h");
    let header_src = fs::read_to_string(&header)
        .unwrap_or_else(|e| panic!("read entry header {}: {e}", header.display()));
    assert!(
        header_src.contains("NrosExecutor *nros_e2e_system_build_executor(const NrosConfig *cfg);")
            && header_src.contains("int32_t nros_e2e_system_register_all(NrosExecutor *executor);")
            && header_src.contains("} NrosConfig;"),
        "entry header declares the C ABI + config override:\n{header_src}"
    );
    // Source form: the vendor-includable CMake fragment over the same crate.
    let entry_cmake = generated_dir.join("CMakeLists.txt");
    let cmake_src = fs::read_to_string(&entry_cmake)
        .unwrap_or_else(|e| panic!("read entry CMakeLists {}: {e}", entry_cmake.display()));
    assert!(
        cmake_src.contains("corrosion_import_crate")
            && cmake_src.contains("CRATES nros_e2e_generated")
            && cmake_src.contains("add_library(e2e_system_entry INTERFACE)"),
        "source-form CMake fragment imports the crate + exposes the entry target:\n{cmake_src}"
    );

    let port = free_local_port();
    let _zenohd = start_zenohd(port);
    assert_generated_binary_spins(&binary, port);

    let multi_plan_path = out_dir.join("nros-plan-multi-instance.json");
    let mut multi_plan = plan.clone();
    add_second_instance(&mut multi_plan);
    fs::write(
        &multi_plan_path,
        serde_json::to_string_pretty(&multi_plan).expect("serialize multi-instance plan"),
    )
    .expect("write multi-instance plan");
    check::run(check::Args {
        plan: multi_plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated multi-instance plan");
    let multi_generated_dir = out_dir.join("generated-multi");
    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated-multi".to_string(),
        output_dir: multi_generated_dir.clone(),
        plan_path: multi_plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture_workspace()),
        release: false,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command compiles generated multi-instance package");
    assert!(
        multi_generated_dir
            .join("../target")
            .join(&multi_plan.build.target)
            .join("debug")
            .join("nros-e2e-generated-multi")
            .is_file()
    );

    // Phase 172.H — a `param_persistence` plan must produce a package whose
    // generated `apply_param_persistence` (register services + declare params +
    // attach FileParamStore) actually compiles and links.
    let persist_plan_path = out_dir.join("nros-plan-persist.json");
    let mut persist_plan = plan.clone();
    persist_plan.param_persistence = Some(PlanParamPersistence {
        backend: "file".to_string(),
        path: out_dir.join("params.store").to_string_lossy().into_owned(),
    });
    fs::write(
        &persist_plan_path,
        serde_json::to_string_pretty(&persist_plan).expect("serialize persistence plan"),
    )
    .expect("write persistence plan");
    check::run(check::Args {
        plan: persist_plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates persistence plan");
    let persist_generated_dir = out_dir.join("generated-persist");
    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated-persist".to_string(),
        output_dir: persist_generated_dir.clone(),
        plan_path: persist_plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture_workspace()),
        release: false,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command compiles generated persistence package");
    assert!(
        persist_generated_dir
            .join("../target")
            .join(&persist_plan.build.target)
            .join("debug")
            .join("nros-e2e-generated-persist")
            .is_file(),
        "generated persistence binary compiled + linked"
    );
}

/// Verifies the fixture workspace builds and boots a generated FreeRTOS package.
#[test]
fn fixture_builds_boots_freertos() {
    let fixture = fixture_workspace();
    let output = temp_output("orchestration_e2e_freertos");
    let out_dir = output.join("build/e2e_system/nros");
    let generated_dir = out_dir.join("generated-freertos");
    let plan_path = out_dir.join("nros-plan-freertos.json");
    fs::create_dir_all(&out_dir).expect("create FreeRTOS output dir");

    let mut plan = fixture_plan("plan_multi_instance.json");
    retarget_plan_to_fixture_component(&mut plan);
    retarget_plan_to_freertos(&mut plan);
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize FreeRTOS plan"),
    )
    .expect("write FreeRTOS plan");

    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated FreeRTOS plan");
    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated-freertos".to_string(),
        output_dir: generated_dir.clone(),
        plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture),
        release: false,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command compiles generated FreeRTOS package");

    let binary = out_dir
        .join("target")
        .join("thumbv7m-none-eabi")
        .join("release")
        .join("nros-e2e-generated-freertos");
    assert!(
        binary.is_file(),
        "generated FreeRTOS binary exists at {}",
        binary.display()
    );
    assert_freertos_binary_boots(&binary);
}

/// Phase 126.M5.nuttx — drives the orchestration generator against
/// the NuttX QEMU ARM (Cortex-A7) board. Skipped when `NUTTX_DIR`
/// isn't set (the board crate's build.rs panics without it). When
/// the workspace is set up, asserts the generated package compiles
/// to an `armv7a-nuttx-eabihf` ELF.
///
/// QEMU boot smoke deferred — NuttX boots via its own bootloader
/// stage that integrates the generated ELF as a child app, not a
/// kernel image; the test asserts on the build artifact only.
#[test]
fn fixture_workspace_builds_generated_nuttx_package() {
    if std::env::var_os("NUTTX_DIR").is_none() {
        eprintln!(
            "[SKIPPED] NUTTX_DIR not set — run `just nuttx setup` and \
             re-export `NUTTX_DIR=third-party/nuttx/nuttx` to enable"
        );
        return;
    }
    if let Some(reason) = build_std_nightly_skip() {
        eprintln!("{reason}");
        return;
    }

    let fixture = fixture_workspace();
    let output = temp_output("orchestration_e2e_nuttx");
    let out_dir = output.join("build/e2e_system/nros");
    let generated_dir = out_dir.join("generated-nuttx");
    let plan_path = out_dir.join("nros-plan-nuttx.json");
    fs::create_dir_all(&out_dir).expect("create NuttX output dir");

    let mut plan = fixture_plan("plan_multi_instance.json");
    retarget_plan_to_fixture_component(&mut plan);
    retarget_plan_to_nuttx(&mut plan);
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize NuttX plan"),
    )
    .expect("write NuttX plan");

    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated NuttX plan");
    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated-nuttx".to_string(),
        output_dir: generated_dir.clone(),
        plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture),
        release: true,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command compiles generated NuttX package");

    let binary = out_dir
        .join("target")
        .join("armv7a-nuttx-eabihf")
        .join("release")
        .join("nros-e2e-generated-nuttx");
    assert!(
        binary.is_file(),
        "generated NuttX binary exists at {}",
        binary.display()
    );
}

/// Phase 126.M5.esp32 — drives the orchestration generator against
/// the ESP32-C3 QEMU board (riscv32imc-unknown-none-elf, esp-hal
/// bare-metal). Skipped when the pinned nightly + `rust-src` aren't
/// installed (build-std needs them). Asserts the generated package
/// compiles to a riscv32imc ELF.
///
/// QEMU boot smoke deferred — the Espressif QEMU fork + flash-image
/// step live in the `just esp32` recipes, not this codegen test.
#[test]
fn fixture_workspace_builds_generated_esp32_package() {
    if let Some(reason) = build_std_nightly_skip() {
        eprintln!("{reason}");
        return;
    }

    let fixture = fixture_workspace();
    let output = temp_output("orchestration_e2e_esp32");
    let out_dir = output.join("build/e2e_system/nros");
    let generated_dir = out_dir.join("generated-esp32");
    let plan_path = out_dir.join("nros-plan-esp32.json");
    fs::create_dir_all(&out_dir).expect("create ESP32 output dir");

    let mut plan = fixture_plan("plan_multi_instance.json");
    retarget_plan_to_fixture_component(&mut plan);
    retarget_plan_to_esp32(&mut plan);
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize ESP32 plan"),
    )
    .expect("write ESP32 plan");

    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated ESP32 plan");
    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated-esp32".to_string(),
        output_dir: generated_dir.clone(),
        plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture),
        release: true,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command compiles generated ESP32 package");

    let binary = out_dir
        .join("target")
        .join("riscv32imc-unknown-none-elf")
        .join("release")
        .join("nros-e2e-generated-esp32");
    assert!(
        binary.is_file(),
        "generated ESP32 binary exists at {}",
        binary.display()
    );

    assert_esp32_binary_boots(&binary, &out_dir);
}

/// Phase 126.M5.esp32 — flash-image the generated ESP32-C3 ELF and
/// boot it under the Espressif `qemu-system-riscv32` fork, asserting
/// the board banner. Skips cleanly when `espflash` or the Espressif
/// QEMU fork (with the `esp32c3` machine model) is unavailable —
/// stock distro `qemu-system-riscv32` (≤ 8.x) lacks that model.
fn assert_esp32_binary_boots(binary: &Path, out_dir: &Path) {
    if Command::new("espflash").arg("--version").output().is_err() {
        eprintln!("[SKIPPED] espflash not found — `cargo install espflash`");
        return;
    }
    let qemu_has_esp32c3 = Command::new("qemu-system-riscv32")
        .args(["-machine", "help"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("esp32c3"))
        .unwrap_or(false);
    if !qemu_has_esp32c3 {
        eprintln!(
            "[SKIPPED] qemu-system-riscv32 with `esp32c3` machine not found \
             (need the Espressif fork — `just esp32 setup-qemu`)"
        );
        return;
    }

    let flash = out_dir.join("nros-e2e-generated-esp32.bin");
    let save = Command::new("espflash")
        .args([
            "save-image",
            "--chip",
            "esp32c3",
            "--flash-size",
            "4mb",
            "--merge",
        ])
        .arg(binary)
        .arg(&flash)
        .output()
        .expect("run espflash save-image");
    assert!(
        save.status.success(),
        "espflash save-image failed:\n{}{}",
        String::from_utf8_lossy(&save.stdout),
        String::from_utf8_lossy(&save.stderr),
    );

    let output = Command::new("timeout")
        .arg("12s")
        .arg("qemu-system-riscv32")
        .args(["-M", "esp32c3", "-icount", "3", "-nographic", "-drive"])
        .arg(format!("file={},if=mtd,format=raw", flash.display()))
        .output()
        .expect("run qemu-system-riscv32 esp32c3");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // timeout exit 124 = QEMU ran the full window without crashing.
    assert!(
        output.status.code() == Some(124) || output.status.success(),
        "generated ESP32 binary exited unexpectedly with {:?}\n{}",
        output.status,
        combined
    );
    assert!(
        combined.contains("nros ESP32-C3 QEMU Platform"),
        "generated ESP32 binary did not print platform banner\n{}",
        combined
    );
}

/// Phase 126.M5.zephyr — drives the orchestration generator against
/// the Zephyr platform. Unlike the FreeRTOS/NuttX siblings the
/// generated package is not a binary crate: zephyr-lang-rust expects
/// a `staticlib` named `rustapp` (consumed via CMake's
/// `rust_cargo_application()`), so cargo cannot link the artifact on
/// its own — that's the kernel image stage owned by `west build`.
/// The test asserts on file layout instead: the package contains a
/// `[lib]`-shaped Cargo.toml, a `src/lib.rs` exporting
/// `rust_main`, plus the Zephyr CMakeLists.txt + prj.conf glue. A
/// full `west build` smoke run is gated separately on `ZEPHYR_BASE`.
#[test]
fn fixture_workspace_generates_zephyr_package_shape() {
    let fixture = fixture_workspace();
    let output = temp_output("orchestration_e2e_zephyr");
    let out_dir = output.join("build/e2e_system/nros");
    let generated_dir = out_dir.join("generated-zephyr");
    let plan_path = out_dir.join("nros-plan-zephyr.json");
    fs::create_dir_all(&out_dir).expect("create Zephyr output dir");

    let mut plan = fixture_plan("plan_multi_instance.json");
    retarget_plan_to_fixture_component(&mut plan);
    retarget_plan_to_zephyr(&mut plan);
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize Zephyr plan"),
    )
    .expect("write Zephyr plan");

    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated Zephyr plan");

    generate_package(&GenerateOptions {
        package_name: "nros-e2e-generated-zephyr".to_string(),
        output_dir: generated_dir.clone(),
        plan_path: plan_path.clone(),
        nros_path: nano_ros_workspace().join("packages/core/nros"),
        nros_orchestration_path: nano_ros_workspace().join("packages/core/nros-orchestration"),
        component_workspace: Some(fixture),
    })
    .expect("generate Zephyr package");

    let cargo_toml = fs::read_to_string(generated_dir.join("Cargo.toml"))
        .expect("read generated Zephyr Cargo.toml");
    assert!(
        cargo_toml.contains("[lib]")
            && cargo_toml.contains("name = \"rustapp\"")
            && cargo_toml.contains("crate-type = [\"staticlib\"]"),
        "generated Zephyr Cargo.toml exposes the zephyr-lang-rust staticlib shape:\n{cargo_toml}"
    );
    assert!(
        cargo_toml.contains("zephyr = \"0.1.0\""),
        "generated Zephyr Cargo.toml depends on the `zephyr` crate:\n{cargo_toml}"
    );
    assert!(
        cargo_toml.contains("zephyr-build = \"0.1.0\""),
        "generated Zephyr Cargo.toml carries the `zephyr-build` build dep:\n{cargo_toml}"
    );
    assert!(
        cargo_toml.contains("platform-zephyr"),
        "generated Zephyr Cargo.toml enables the platform-zephyr feature:\n{cargo_toml}"
    );

    let lib_rs = fs::read_to_string(generated_dir.join("src/lib.rs"))
        .expect("read generated Zephyr src/lib.rs");
    assert!(
        lib_rs.contains("extern \"C\" fn rust_main()"),
        "generated Zephyr lib.rs exports rust_main():\n{lib_rs}"
    );
    assert!(
        !generated_dir.join("src/main.rs").is_file(),
        "Zephyr generated package must not emit src/main.rs"
    );

    let cmake = fs::read_to_string(generated_dir.join("CMakeLists.txt"))
        .expect("read generated Zephyr CMakeLists.txt");
    assert!(
        cmake.contains("find_package(Zephyr") && cmake.contains("rust_cargo_application()"),
        "generated Zephyr CMakeLists.txt wires rust_cargo_application():\n{cmake}"
    );

    let prj_conf =
        fs::read_to_string(generated_dir.join("prj.conf")).expect("read generated Zephyr prj.conf");
    assert!(
        prj_conf.contains("CONFIG_RUST=y") && prj_conf.contains("CONFIG_NROS=y"),
        "generated Zephyr prj.conf enables Rust + nros:\n{prj_conf}"
    );

    // Zephyr's cargo target comes from zephyr-lang-rust CMake, not
    // a `[build] target = ...` entry. The orchestrator must NOT
    // emit a `.cargo/config.toml` for Zephyr.
    assert!(
        !generated_dir.join(".cargo").join("config.toml").is_file(),
        "Zephyr generated package must not emit .cargo/config.toml \
         (target triple comes from zephyr-lang-rust at CMake time)"
    );
}

/// Phase 126.M5.threadx-riscv64 — drives the orchestration generator
/// against bare-metal ThreadX on QEMU RISC-V virt
/// (riscv64gc-unknown-none-elf). no_std/no_main + `#[no_mangle] extern
/// "C" fn main`; ThreadX kernel + NetX Duo over virtio-net. Asserts
/// the generated package compiles to a riscv64gc ELF.
#[test]
fn fixture_workspace_builds_generated_threadx_riscv64_package() {
    let fixture = fixture_workspace();
    let output = temp_output("orchestration_e2e_threadx_riscv64");
    let out_dir = output.join("build/e2e_system/nros");
    let generated_dir = out_dir.join("generated-threadx-riscv64");
    let plan_path = out_dir.join("nros-plan-threadx-riscv64.json");
    fs::create_dir_all(&out_dir).expect("create ThreadX-RISCV64 output dir");

    let mut plan = fixture_plan("plan_multi_instance.json");
    retarget_plan_to_fixture_component(&mut plan);
    retarget_plan_to_threadx_riscv64(&mut plan);
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize ThreadX-RISCV64 plan"),
    )
    .expect("write ThreadX-RISCV64 plan");

    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated ThreadX-RISCV64 plan");
    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated-threadx-riscv64".to_string(),
        output_dir: generated_dir.clone(),
        plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture),
        release: true,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command compiles generated ThreadX-RISCV64 package");

    let binary = out_dir
        .join("target")
        .join("riscv64gc-unknown-none-elf")
        .join("release")
        .join("nros-e2e-generated-threadx-riscv64");
    assert!(
        binary.is_file(),
        "generated ThreadX-RISCV64 binary exists at {}",
        binary.display()
    );
}

/// Phase 126.M5.stm32f4 — drives the orchestration generator against
/// the STM32F4 board (NUCLEO-F429ZI, Cortex-M4F, thumbv7em-none-eabihf).
/// no_std/no_main + cortex-m-rt `#[entry]`; defmt-rtt diagnostics +
/// panic-probe. Asserts the generated package compiles to a thumbv7em
/// ELF (real-hardware target — no QEMU boot).
#[test]
fn fixture_workspace_builds_generated_stm32f4_package() {
    let fixture = fixture_workspace();
    let output = temp_output("orchestration_e2e_stm32f4");
    let out_dir = output.join("build/e2e_system/nros");
    let generated_dir = out_dir.join("generated-stm32f4");
    let plan_path = out_dir.join("nros-plan-stm32f4.json");
    fs::create_dir_all(&out_dir).expect("create STM32F4 output dir");

    let mut plan = fixture_plan("plan_multi_instance.json");
    retarget_plan_to_fixture_component(&mut plan);
    retarget_plan_to_stm32f4(&mut plan);
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize STM32F4 plan"),
    )
    .expect("write STM32F4 plan");

    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated STM32F4 plan");
    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated-stm32f4".to_string(),
        output_dir: generated_dir.clone(),
        plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture),
        release: true,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command compiles generated STM32F4 package");

    let binary = out_dir
        .join("target")
        .join("thumbv7em-none-eabihf")
        .join("release")
        .join("nros-e2e-generated-stm32f4");
    assert!(
        binary.is_file(),
        "generated STM32F4 binary exists at {}",
        binary.display()
    );
}

/// Phase 126.M5.bare-metal — drives the orchestration generator
/// against the pure Cortex-M3 board (MPS2-AN385, thumbv7m-none-eabi).
/// no_std/no_main + cortex-m-rt `#[entry]`; the board crate owns
/// hardware + smoltcp init and the cortex-m-rt linker script. Asserts
/// the generated package compiles to a thumbv7m ELF.
#[test]
fn fixture_workspace_builds_generated_bare_metal_package() {
    let fixture = fixture_workspace();
    let output = temp_output("orchestration_e2e_bare_metal");
    let out_dir = output.join("build/e2e_system/nros");
    let generated_dir = out_dir.join("generated-bare-metal");
    let plan_path = out_dir.join("nros-plan-bare-metal.json");
    fs::create_dir_all(&out_dir).expect("create bare-metal output dir");

    let mut plan = fixture_plan("plan_multi_instance.json");
    retarget_plan_to_fixture_component(&mut plan);
    retarget_plan_to_bare_metal(&mut plan);
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize bare-metal plan"),
    )
    .expect("write bare-metal plan");

    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated bare-metal plan");
    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated-bare-metal".to_string(),
        output_dir: generated_dir.clone(),
        plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture),
        release: true,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command compiles generated bare-metal package");

    let binary = out_dir
        .join("target")
        .join("thumbv7m-none-eabi")
        .join("release")
        .join("nros-e2e-generated-bare-metal");
    assert!(
        binary.is_file(),
        "generated bare-metal binary exists at {}",
        binary.display()
    );
}

/// Phase 172 W.5.8 — a service + action component on a no_std target (bare-metal
/// Cortex-M3, no SDK) dispatches real bodies through a function-local `static mut`
/// context (no `Box::leak`/alloc). Compile-verifies the static-ctx codegen: the
/// generated package must build to a `thumbv7m-none-eabi` ELF and its build.rs
/// must carry the static context (not the std Box::leak ctx / tick loop).
/// Verifies the fixture workspace builds a generated bare-metal service/action package.
#[test]
fn fixture_builds_service_action_baremetal() {
    let fixture = fixture_workspace();
    let output = temp_output("orchestration_e2e_bare_metal_svc_act");
    let out_dir = output.join("build/e2e_system/nros");
    let generated_dir = out_dir.join("generated-bare-metal-svc-act");
    let plan_path = out_dir.join("nros-plan-bare-metal-svc-act.json");
    fs::create_dir_all(&out_dir).expect("create bare-metal svc/act output dir");

    let mut plan = fixture_plan("plan_service_action.json");
    retarget_plan_to_fixture_component(&mut plan);
    retarget_plan_to_bare_metal(&mut plan);
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize bare-metal svc/act plan"),
    )
    .expect("write bare-metal svc/act plan");

    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated bare-metal svc/act plan");
    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated-bare-metal-svc-act".to_string(),
        output_dir: generated_dir.clone(),
        plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture),
        release: true,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command compiles generated bare-metal svc/act package");

    let build_rs =
        fs::read_to_string(generated_dir.join("build.rs")).expect("read generated build.rs");
    assert!(build_rs.contains("static mut SVC_CTX_"));
    assert!(build_rs.contains("static mut ACT_CTX_"));
    assert!(!build_rs.contains("::std::boxed::Box::into_raw"));
    assert!(!build_rs.contains("TICK_ENTRIES"));

    let binary = out_dir
        .join("target")
        .join("thumbv7m-none-eabi")
        .join("release")
        .join("nros-e2e-generated-bare-metal-svc-act");
    assert!(
        binary.is_file(),
        "generated bare-metal svc/act binary exists at {}",
        binary.display()
    );
}

/// Phase 172 W.5.11 — no_std action *execution*. The dedicated `fib_server`
/// component (single action + a real `tick` that drives feedback/result via
/// `for_each_active_goal`) on a no_std target (bare-metal Cortex-M3) compiles
/// through the module-level static action ctx + `tick_{idx}` + the infinite
/// `run_tick_loop_nostd` (no `thread_local`/alloc/`is_halted`); the no_std self
/// shim spins via it. Compile-only (no embedded action client to exchange with).
/// Verifies the fixture workspace builds a generated bare-metal Fibonacci action package.
#[test]
fn fixture_builds_fibonacci_action_baremetal() {
    let fixture = fixture_workspace();
    let output = temp_output("orchestration_e2e_bare_metal_fib");
    let out_dir = output.join("build/e2e_system/nros");
    let generated_dir = out_dir.join("generated-bare-metal-fib");
    let plan_path = out_dir.join("nros-plan-bare-metal-fib.json");
    fs::create_dir_all(&out_dir).expect("create bare-metal fib output dir");

    // plan_fibonacci_action already targets the single-entity `demo_pkg::fib_server`.
    let mut plan = fixture_plan("plan_fibonacci_action.json");
    retarget_plan_to_bare_metal(&mut plan);
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize bare-metal fib plan"),
    )
    .expect("write bare-metal fib plan");

    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated bare-metal fib plan");
    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated-bare-metal-fib".to_string(),
        output_dir: generated_dir.clone(),
        plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture),
        release: true,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command compiles generated bare-metal fib package");

    let build_rs =
        fs::read_to_string(generated_dir.join("build.rs")).expect("read generated build.rs");
    assert!(build_rs.contains("static mut ACT_HANDLE_"));
    assert!(build_rs.contains("fn tick_"));
    assert!(build_rs.contains("pub fn run_tick_loop_nostd("));
    assert!(!build_rs.contains("TICK_ENTRIES"));
    let main_rs = fs::read_to_string(generated_dir.join("src/main.rs")).expect("read main.rs");
    assert!(main_rs.contains("run_tick_loop_nostd(&mut executor)"));

    let binary = out_dir
        .join("target")
        .join("thumbv7m-none-eabi")
        .join("release")
        .join("nros-e2e-generated-bare-metal-fib");
    assert!(
        binary.is_file(),
        "generated bare-metal fib binary exists at {}",
        binary.display()
    );
}

/// Phase 126.M5.threadx — drives the orchestration generator against
/// the ThreadX-Linux board (host-hosted ThreadX + NetX Duo over the
/// NSOS BSD shim). Builds as a normal x86_64 Linux ELF — no custom
/// target or build-std, the board crate owns the kernel/NetX link.
/// Asserts the generated package compiles to a host binary.
#[test]
fn fixture_workspace_builds_generated_threadx_linux_package() {
    let fixture = fixture_workspace();
    let output = temp_output("orchestration_e2e_threadx_linux");
    let out_dir = output.join("build/e2e_system/nros");
    let generated_dir = out_dir.join("generated-threadx-linux");
    let plan_path = out_dir.join("nros-plan-threadx-linux.json");
    fs::create_dir_all(&out_dir).expect("create ThreadX-Linux output dir");

    let mut plan = fixture_plan("plan_multi_instance.json");
    retarget_plan_to_fixture_component(&mut plan);
    retarget_plan_to_threadx_linux(&mut plan);
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize ThreadX-Linux plan"),
    )
    .expect("write ThreadX-Linux plan");

    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated ThreadX-Linux plan");
    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated-threadx-linux".to_string(),
        output_dir: generated_dir.clone(),
        plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture),
        release: true,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command compiles generated ThreadX-Linux package");

    let binary = out_dir
        .join("target")
        .join("x86_64-unknown-linux-gnu")
        .join("release")
        .join("nros-e2e-generated-threadx-linux");
    assert!(
        binary.is_file(),
        "generated ThreadX-Linux binary exists at {}",
        binary.display()
    );
}

#[test]
fn fixture_workspace_links_mixed_c_component_archive() {
    let fixture = fixture_workspace();
    let output = temp_output("orchestration_e2e_mixed_c");
    let out_dir = output.join("build/e2e_system/nros");
    let generated_dir = out_dir.join("generated-mixed-c");
    let plan_path = out_dir.join("nros-plan-mixed-c.json");
    fs::create_dir_all(&out_dir).expect("create mixed C output dir");

    let archive = build_native_counter_archive(&output, "c_counter", "counter.c", "cc");
    let component_config = output.join("c_counter.nros.toml");
    write_native_component_config(
        &component_config,
        "c_counter",
        "nros_component_counter",
        "c",
        &archive,
        "c_counter.metadata.json",
    );
    let source_metadata = output.join("c_counter.metadata.json");
    write_native_source_metadata(
        &source_metadata,
        "c_counter",
        "nros_component_counter",
        "c",
        "counter_node",
        "counter",
        "/c",
    );

    let cpp_archive = build_native_counter_archive(&output, "cpp_counter", "counter.cpp", "c++");
    let cpp_component_config = output.join("cpp_counter.nros.toml");
    write_native_component_config(
        &cpp_component_config,
        "cpp_counter",
        "nros_component_cpp_counter",
        "cpp",
        &cpp_archive,
        "cpp_counter.metadata.json",
    );
    let cpp_source_metadata = output.join("cpp_counter.metadata.json");
    write_native_source_metadata(
        &cpp_source_metadata,
        "cpp_counter",
        "nros_component_cpp_counter",
        "cpp",
        "cpp_counter_node",
        "cpp_counter",
        "/cpp",
    );

    let mut plan = fixture_plan("plan_multi_instance.json");
    retarget_plan_to_fixture_component(&mut plan);
    add_native_counter_component(
        &mut plan,
        "c_counter",
        "c_counter::counter",
        "nros_component_counter",
        "c",
        "counter",
        "/c",
        "counter_node",
        &component_config,
        &source_metadata,
    );
    add_native_counter_component(
        &mut plan,
        "cpp_counter",
        "cpp_counter::counter",
        "nros_component_cpp_counter",
        "cpp",
        "cpp_counter",
        "/cpp",
        "cpp_counter_node",
        &cpp_component_config,
        &cpp_source_metadata,
    );
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize mixed C plan"),
    )
    .expect("write mixed C plan");

    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated mixed C plan");
    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated-mixed-c".to_string(),
        output_dir: generated_dir.clone(),
        plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture),
        release: false,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command links generated package with C component archive");

    let build_rs = fs::read_to_string(generated_dir.join("build.rs")).expect("read build.rs");
    assert!(build_rs.contains("cargo:rustc-link-lib=static=c_counter"));
    assert!(build_rs.contains("cargo:rustc-link-lib=static=cpp_counter"));
    let binary = out_dir
        .join("target")
        .join(&plan.build.target)
        .join(if plan.build.profile == "release" {
            "release"
        } else {
            "debug"
        })
        .join("nros-e2e-generated-mixed-c");
    assert!(
        binary.is_file(),
        "generated mixed C binary exists at {}",
        binary.display()
    );
}

#[test]
fn fixture_workspace_builds_generated_service_action_package() {
    let fixture = fixture_workspace();
    let output = temp_output("orchestration_e2e_service_action");
    let out_dir = output.join("build/e2e_system/nros");
    let generated_dir = out_dir.join("generated-service-action");
    let plan_path = out_dir.join("nros-plan-service-action.json");
    fs::create_dir_all(&out_dir).expect("create service/action output dir");

    let mut plan = fixture_plan("plan_service_action.json");
    retarget_plan_to_fixture_component(&mut plan);
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize service/action plan"),
    )
    .expect("write service/action plan");

    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated service/action plan");
    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated-service-action".to_string(),
        output_dir: generated_dir.clone(),
        plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture),
        release: false,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command compiles generated service/action package");

    let generated_tables =
        fs::read_to_string(generated_dir.join("build.rs")).expect("read generated build.rs");
    assert!(generated_tables.contains("register_service_raw_sized_on::<1024, 1024>"));
    assert!(generated_tables.contains("register_action_server_raw_sized::<1024, 1024, 1024, 4>"));
    let binary = out_dir
        .join("target")
        .join(&plan.build.target)
        .join(if plan.build.profile == "release" {
            "release"
        } else {
            "debug"
        })
        .join("nros-e2e-generated-service-action");
    assert!(
        binary.is_file(),
        "generated service/action binary exists at {}",
        binary.display()
    );
}

/// Phase 172 W.5.10 — the tick-driven action runtime exchange. Builds a generated
/// Fibonacci action server (the `demo_pkg` component accepts the goal in
/// `on_callback`, then its `tick` iterates `for_each_active_goal`, publishes
/// growing-sequence feedback, and `complete_goal`s) and runs the prebuilt
/// `examples/native/rust/action-client` against it over a zenohd router. Proves
/// the W.5.6 `run_tick_loop` + `GenActionExec` drive a real goal end-to-end over
/// the wire — the client must observe goal acceptance + feedback.
#[test]
fn fibonacci_action_tick_drives_example_client_exchange() {
    let fixture = fixture_workspace();
    let output = temp_output("orchestration_e2e_fib_action");
    let out_dir = output.join("build/e2e_system/nros");
    let generated_dir = out_dir.join("generated-fibonacci");
    let plan_path = out_dir.join("nros-plan-fibonacci.json");
    fs::create_dir_all(&out_dir).expect("create fibonacci output dir");

    // The plan already targets the dedicated `demo_pkg::fib_server` component
    // (single node + single action), so its `MAX_ENTITIES` matches what the
    // component declares — no `retarget_plan_to_fixture_component`.
    let plan = fixture_plan("plan_fibonacci_action.json");
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize fibonacci plan"),
    )
    .expect("write fibonacci plan");

    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated fibonacci plan");
    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated-fibonacci".to_string(),
        output_dir: generated_dir.clone(),
        plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture),
        release: true,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command compiles generated fibonacci server");

    let server_bin = out_dir
        .join("target")
        .join("x86_64-unknown-linux-gnu")
        .join("release")
        .join("nros-e2e-generated-fibonacci");
    assert!(
        server_bin.is_file(),
        "generated fibonacci server binary exists at {}",
        server_bin.display()
    );

    let client_bin = ensure_action_client_binary();

    let port = free_local_port();
    let _zenohd = start_zenohd(port);
    let locator = format!("tcp/127.0.0.1:{port}");

    // Server first; it has no stdout markers, so give discovery a moment.
    let mut server = Command::new(&server_bin)
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("RUST_LOG", "debug")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn fibonacci server {}: {e}", server_bin.display()));
    // Fail fast if the server died on boot (registration error, etc.).
    thread::sleep(Duration::from_secs(3));
    if let Some(status) = server.try_wait().expect("poll server status") {
        let out = server.wait_with_output().expect("collect server output");
        panic!(
            "fibonacci server exited early ({status})\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let _server_guard = ChildGuard(server);

    let client = Command::new(&client_bin)
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn action client {}: {e}", client_bin.display()));

    let (exited, transcript) = wait_capture(client, Duration::from_secs(30));
    assert!(
        exited,
        "action client did not finish within 30s; transcript:\n{transcript}"
    );
    assert!(
        transcript.contains("Goal accepted!"),
        "client never saw goal acceptance (generated tick path):\n{transcript}"
    );
    assert!(
        transcript.contains("Feedback #"),
        "client never received tick-published feedback:\n{transcript}"
    );
    assert!(
        transcript.contains("Action client finished"),
        "client did not finish cleanly:\n{transcript}"
    );
}

/// Resolve a prebuilt native zenoh example binary (`examples/native/rust/<dir>`,
/// bin `<bin>`), building it on demand (incremental) if `just native
/// build-fixture-rust` hasn't run.
fn ensure_example_binary(dir: &str, bin: &str) -> PathBuf {
    let ex_dir = nano_ros_workspace().join(format!("examples/native/rust/{dir}"));
    let binary = ex_dir.join(format!("target-zenoh/release/{bin}"));
    if binary.is_file() {
        return binary;
    }
    let status = Command::new(env!("CARGO"))
        .current_dir(&ex_dir)
        .args([
            "build",
            "--release",
            "--no-default-features",
            "--features",
            "rmw-zenoh",
            "--target-dir",
            "target-zenoh",
        ])
        .status()
        .unwrap_or_else(|e| panic!("build {dir} example: {e}"));
    assert!(
        status.success(),
        "building the {dir} example failed; run `just native build-fixture-rust`"
    );
    assert!(
        binary.is_file(),
        "{bin} binary missing after build at {}",
        binary.display()
    );
    binary
}

/// The `action-client` example — its vendored `example_interfaces` Fibonacci CDR
/// matches the generated server's hand-mirrored types, so they interoperate.
fn ensure_action_client_binary() -> PathBuf {
    ensure_example_binary("action-client", "action-client")
}

/// Poll `child` until it exits or `timeout` elapses (killing it on timeout),
/// then collect its full stdout+stderr. Returns `(exited_on_its_own, transcript)`.
fn wait_capture(mut child: Child, timeout: Duration) -> (bool, String) {
    let deadline = Instant::now() + timeout;
    let exited = loop {
        if child
            .try_wait()
            .expect("poll client process status")
            .is_some()
        {
            break true;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            break false;
        }
        thread::sleep(Duration::from_millis(100));
    };
    let out = child.wait_with_output().expect("collect client output");
    let mut transcript = String::from_utf8_lossy(&out.stdout).into_owned();
    transcript.push_str(&String::from_utf8_lossy(&out.stderr));
    (exited, transcript)
}

/// Rewrite the bridge plan's two zenoh locators (both `build.transports` and the
/// `[[bridge]]` endpoints) to the test's actual router addresses — the generated
/// `SESSION_SPECS` bake these at codegen time.
fn bridge_set_locators(plan: &mut NrosPlan, loc_a: &str, loc_b: &str) {
    assert_eq!(
        plan.build.transports.len(),
        2,
        "bridge plan has 2 transports"
    );
    plan.build.transports[0].locator = Some(loc_a.to_string());
    plan.build.transports[1].locator = Some(loc_b.to_string());
    assert_eq!(plan.bridges.len(), 1, "bridge plan has 1 bridge");
    let connect = &mut plan.bridges[0].connect;
    assert_eq!(connect.len(), 2, "bridge connects 2 endpoints");
    connect[0].locator = Some(loc_a.to_string());
    connect[1].locator = Some(loc_b.to_string());
}

/// Phase 172 — bridge topic-forwarding runtime exchange. The generated bridge
/// package opens two zenoh sessions (router A + router B); its own
/// `chatter_talker` component publishes `std_msgs/Int32` on `/chatter` over
/// endpoint 0 (router A), and the generated `register_bridges` relay forwards it
/// to endpoint 1 (router B), where the prebuilt `listener` example receives it.
/// Proves the cross-session relay (`register_bridges`: generic-sub →
/// generic-pub `publish_raw_with_attachment` with `bridge_origin` echo
/// suppression) actually forwards data over the wire.
#[test]
fn bridge_forwards_chatter_across_two_zenoh_routers() {
    let fixture = fixture_workspace();
    let output = temp_output("orchestration_e2e_bridge");
    let out_dir = output.join("build/e2e_system/nros");
    let generated_dir = out_dir.join("generated-bridge");
    let plan_path = out_dir.join("nros-plan-bridge.json");
    fs::create_dir_all(&out_dir).expect("create bridge output dir");

    let port_a = free_local_port();
    let mut port_b = free_local_port();
    while port_b == port_a {
        port_b = free_local_port();
    }
    let loc_a = format!("tcp/127.0.0.1:{port_a}");
    let loc_b = format!("tcp/127.0.0.1:{port_b}");

    let mut plan = fixture_plan("plan_bridge_forward.json");
    bridge_set_locators(&mut plan, &loc_a, &loc_b);
    fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan).expect("serialize bridge plan"),
    )
    .expect("write bridge plan");

    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated bridge plan");
    build_generated_package(&BuildOptions {
        package_name: "nros-e2e-generated-bridge".to_string(),
        output_dir: generated_dir.clone(),
        plan_path,
        workspace_root: nano_ros_workspace(),
        component_workspace: Some(fixture),
        release: true,
        target: None,
        cargo_args: Vec::new(),
        force: false,
    })
    .expect("build command compiles generated bridge package");

    // The generated bridge build.rs carries the relay (sanity — the unit test
    // covers emission in detail).
    let build_rs =
        fs::read_to_string(generated_dir.join("build.rs")).expect("read generated build.rs");
    assert!(build_rs.contains("pub fn register_bridges("));
    assert!(build_rs.contains("publish_raw_with_attachment"));

    let bridge_bin = out_dir
        .join("target")
        .join("x86_64-unknown-linux-gnu")
        .join("release")
        .join("nros-e2e-generated-bridge");
    assert!(
        bridge_bin.is_file(),
        "generated bridge binary exists at {}",
        bridge_bin.display()
    );

    let listener_bin = ensure_example_binary("listener", "listener");

    // Two independent routers — without the bridge, A and B are isolated.
    let _zenohd_a = start_zenohd(port_a);
    let _zenohd_b = start_zenohd(port_b);

    // The bridge bakes both locators in SESSION_SPECS — no env needed.
    let bridge = Command::new(&bridge_bin)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn bridge {}: {e}", bridge_bin.display()));
    let _bridge_guard = ChildGuard(bridge);
    thread::sleep(Duration::from_secs(3));

    // Listener on router B: it only receives `/chatter` if the bridge forwarded
    // it from router A. Runs until killed, so scrape its output over a window.
    let listener = Command::new(&listener_bin)
        .env("NROS_LOCATOR", &loc_b)
        .env("NROS_SESSION_MODE", "client")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn listener {}: {e}", listener_bin.display()));
    let (_exited, transcript) = wait_capture(listener, Duration::from_secs(12));
    assert!(
        transcript.contains("Received:"),
        "listener on router B never received bridged /chatter:\n{transcript}"
    );
}

/// Phase 172.E driver — compile the `demo_pkg::talker` component in metadata
/// mode, run it against the in-memory recorder, and assert the emitted JSON is
/// valid source-metadata describing the node + publisher + timer it declares.
#[test]
fn metadata_mode_build_emits_source_metadata_for_component() {
    let fixture = fixture_workspace();
    let out = temp_output("metadata_build");
    let output_path = out.join("talker.metadata.json");

    build_metadata(&MetadataBuildOptions {
        component_id: "demo_pkg::talker".to_string(),
        package: "demo_pkg".to_string(),
        component: "talker".to_string(),
        executable: Some("talker".to_string()),
        exported_symbol: Some("nros_component_talker".to_string()),
        component_dir: fixture.join("src/demo_pkg"),
        nano_ros_workspace: nano_ros_workspace(),
        output_path: output_path.clone(),
        harness_dir: out.join("probe"),
    })
    .expect("metadata-mode build produces source metadata");

    let raw = fs::read_to_string(&output_path).expect("read produced metadata");
    let meta: SourceMetadata = serde_json::from_str(&raw).expect("valid SourceMetadata JSON");
    assert_eq!(meta.package, "demo_pkg");
    assert_eq!(meta.component, "talker");
    assert_eq!(meta.nodes.len(), 1);
    let node = &meta.nodes[0];
    assert_eq!(node.id, "node_talker");
    assert_eq!(node.publishers.len(), 1);
    assert_eq!(node.publishers[0].id, "pub_chatter");
    assert_eq!(node.timers.len(), 1);
    assert_eq!(node.timers[0].id, "timer_publish");
}

/// Phase 172.E CLI wiring — `nros metadata --build` discovers the declared
/// `probe_pkg` component (via its `component_nros.toml`), compiles + runs it in
/// metadata mode to produce the missing `source-metadata`, then collects it.
/// Verifies metadata build discovers sources and produces missing source metadata.
#[test]
fn metadata_build_discovers_missing_sources() {
    let ws = codegen_root().join("testing_workspaces/metadata_build_ws");
    let produced = ws.join("src/probe_pkg/node.metadata.json");
    let _ = fs::remove_file(&produced); // start clean (gitignored)
    let out = temp_output("metadata_build_discovery");

    metadata::run(metadata::Args {
        system_pkg: "probe_sys".to_string(),
        workspace: Some(ws.clone()),
        out_dir: Some(out.clone()),
        metadata: Vec::new(),
        build: true,
        nano_ros_workspace: Some(nano_ros_workspace()),
    })
    .expect("nros metadata --build discovers + produces missing source metadata");

    // The component's declared source-metadata path was produced...
    assert!(
        produced.is_file(),
        "metadata-mode build wrote {}",
        produced.display()
    );
    let meta: SourceMetadata =
        serde_json::from_str(&fs::read_to_string(&produced).expect("read produced"))
            .expect("valid SourceMetadata");
    assert_eq!(meta.package, "probe_pkg");
    assert_eq!(meta.nodes.len(), 1);
    assert_eq!(meta.nodes[0].id, "probe_node");
    assert_eq!(meta.nodes[0].timers.len(), 1);
    // ...and collected into the out metadata dir.
    assert!(out.join("metadata/node.metadata.json").is_file());

    let _ = fs::remove_file(&produced); // don't leave it in the source tree
}

fn fixture_workspace() -> PathBuf {
    codegen_root().join("testing_workspaces/orchestration_e2e")
}

fn codegen_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("codegen root ancestor")
        .to_path_buf()
}

fn nano_ros_workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("nano-ros workspace ancestor")
        .to_path_buf()
}

/// Phase 177.5 — precondition guard for `-Z build-std` e2e tests (NuttX,
/// ESP32). Returns a `[SKIPPED]` reason when the pinned nightly + its
/// `rust-src` component isn't installed, so the build-std targets skip
/// cleanly instead of failing partway through with an opaque
/// `can't find crate for 'core'`. The channel is read from the
/// workspace `tools/rust-toolchain.toml` — the single source of truth
/// the generated packages and `just` recipes also pin to.
fn build_std_nightly_skip() -> Option<String> {
    let toolchain_file = nano_ros_workspace().join("tools/rust-toolchain.toml");
    let channel = fs::read_to_string(&toolchain_file).ok().and_then(|text| {
        text.lines().find_map(|line| {
            let rest = line
                .trim()
                .strip_prefix("channel")?
                .trim_start()
                .strip_prefix('=')?;
            Some(rest.trim().trim_matches('"').to_string())
        })
    })?;

    let has_rust_src = Command::new("rustup")
        .args(["component", "list", "--installed", "--toolchain", &channel])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .any(|c| c.trim() == "rust-src")
        })
        .unwrap_or(false);

    if has_rust_src {
        None
    } else {
        Some(format!(
            "[SKIPPED] `-Z build-std` needs the pinned nightly + rust-src — install with \
             `rustup toolchain install {channel}` && \
             `rustup component add rust-src --toolchain {channel}` \
             (channel pinned in tools/rust-toolchain.toml)"
        ))
    }
}

fn fixture_plan(name: &str) -> NrosPlan {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/orchestration")
        .join(name);
    serde_json::from_str(&fs::read_to_string(&path).expect("read plan fixture"))
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn retarget_plan_to_fixture_component(plan: &mut NrosPlan) {
    for component in &mut plan.components {
        if component.id == "demo_nodes_rs::talker" {
            component.id = "demo_pkg::talker".to_string();
            component.package = "demo_pkg".to_string();
            component.component = "talker".to_string();
        }
    }
    for instance in &mut plan.instances {
        if instance.component == "demo_nodes_rs::talker" {
            instance.component = "demo_pkg::talker".to_string();
            instance.package = "demo_pkg".to_string();
        }
    }
}

fn retarget_plan_to_freertos(plan: &mut NrosPlan) {
    plan.build.target = "thumbv7m-none-eabi".to_string();
    plan.build.board = "freertos".to_string();
    plan.build.rmw = "zenoh".to_string();
    plan.build.profile = "release".to_string();
}

fn retarget_plan_to_nuttx(plan: &mut NrosPlan) {
    plan.build.target = "armv7a-nuttx-eabihf".to_string();
    plan.build.board = "nuttx".to_string();
    plan.build.rmw = "zenoh".to_string();
    plan.build.profile = "release".to_string();
}

fn retarget_plan_to_esp32(plan: &mut NrosPlan) {
    plan.build.target = "riscv32imc-unknown-none-elf".to_string();
    plan.build.board = "esp32-qemu".to_string();
    plan.build.rmw = "zenoh".to_string();
    plan.build.profile = "release".to_string();
}

fn retarget_plan_to_zephyr(plan: &mut NrosPlan) {
    // native_sim/native/64 is the host-side QEMU-free Zephyr target
    // exercised by examples/zephyr; cargo never sees this triple
    // directly (the target comes from zephyr-lang-rust at CMake
    // time), so the field is recorded for plan completeness only.
    plan.build.target = "x86_64-unknown-linux-gnu".to_string();
    plan.build.board = "zephyr".to_string();
    plan.build.rmw = "zenoh".to_string();
    plan.build.profile = "release".to_string();
}

fn retarget_plan_to_bare_metal(plan: &mut NrosPlan) {
    // Pure Cortex-M3 (MPS2-AN385, thumbv7m-none-eabi). no_std/no_main,
    // cortex-m-rt `#[entry]`, semihosting panic + QEMU exit.
    plan.build.target = "thumbv7m-none-eabi".to_string();
    plan.build.board = "bare-metal".to_string();
    plan.build.rmw = "zenoh".to_string();
    plan.build.profile = "release".to_string();
}

fn retarget_plan_to_stm32f4(plan: &mut NrosPlan) {
    // STM32F4 (NUCLEO-F429ZI, Cortex-M4F, thumbv7em-none-eabihf).
    // no_std/no_main, cortex-m-rt `#[entry]`, defmt-rtt diagnostics,
    // panic-probe. Real-hardware flash target (probe-rs) — the e2e
    // test asserts the build artifact only.
    plan.build.target = "thumbv7em-none-eabihf".to_string();
    plan.build.board = "stm32f4".to_string();
    plan.build.rmw = "zenoh".to_string();
    plan.build.profile = "release".to_string();
}

fn retarget_plan_to_threadx_riscv64(plan: &mut NrosPlan) {
    // Bare-metal ThreadX on QEMU RISC-V virt (riscv64gc-unknown-none-elf).
    // no_std/no_main, `#[no_mangle] extern "C" fn main`, ThreadX kernel +
    // NetX Duo over virtio-net. The `riscv64` target discriminates it
    // from the host-hosted threadx-linux variant.
    plan.build.target = "riscv64gc-unknown-none-elf".to_string();
    plan.build.board = "threadx".to_string();
    plan.build.rmw = "zenoh".to_string();
    plan.build.profile = "release".to_string();
}

fn retarget_plan_to_threadx_linux(plan: &mut NrosPlan) {
    // ThreadX-Linux is a host-hosted build: ThreadX kernel + NetX Duo
    // (via the NSOS BSD shim) link into a normal x86_64 Linux ELF.
    // No custom target / build-std — the board crate owns the link.
    plan.build.target = "x86_64-unknown-linux-gnu".to_string();
    plan.build.board = "threadx".to_string();
    plan.build.rmw = "zenoh".to_string();
    plan.build.profile = "release".to_string();
}

fn add_second_instance(plan: &mut NrosPlan) {
    let mut instance = plan.instances[0].clone();
    let old_instance_id = instance.id.clone();
    let new_instance_id = "talker_clone";
    instance.id = new_instance_id.to_string();
    instance.launch_name = "talker_clone".to_string();
    instance.namespace = "/clone".to_string();
    for node in &mut instance.nodes {
        let old_node_id = node.id.clone();
        node.id = node.id.replacen(&old_instance_id, new_instance_id, 1);
        node.resolved_name = "/clone/talker".to_string();
        node.namespace = "/clone".to_string();
        for entity in &mut node.entities {
            rewrite_entity_id(entity, &old_instance_id, new_instance_id);
        }
        for parameter in &mut instance.parameters {
            if parameter.node == old_node_id {
                parameter.node = node.id.clone();
            }
        }
    }
    for callback in &mut instance.callbacks {
        callback.id = callback.id.replacen(&old_instance_id, new_instance_id, 1);
    }
    for binding in &mut instance.sched_bindings {
        binding.callback = binding
            .callback
            .replacen(&old_instance_id, new_instance_id, 1);
    }
    for interface in &mut plan.interfaces {
        let extra = interface
            .used_by
            .iter()
            .filter(|entity| entity.starts_with(&old_instance_id))
            .map(|entity| entity.replacen(&old_instance_id, new_instance_id, 1))
            .collect::<Vec<_>>();
        interface.used_by.extend(extra);
        interface.used_by.sort();
        interface.used_by.dedup();
    }
    plan.instances.push(instance);
}

#[allow(clippy::too_many_arguments)]
fn add_native_counter_component(
    plan: &mut NrosPlan,
    package: &str,
    component_id: &str,
    symbol: &str,
    language: &str,
    executable: &str,
    namespace: &str,
    source_node: &str,
    config: &Path,
    metadata: &Path,
) {
    plan.components.push(PlanComponent {
        id: component_id.to_string(),
        package: package.to_string(),
        component: symbol.to_string(),
        language: language.to_string(),
        source_metadata: metadata.display().to_string(),
        component_config: Some(config.display().to_string()),
    });

    let mut instance = plan.instances[0].clone();
    instance.id = package.to_string();
    instance.component = component_id.to_string();
    instance.package = package.to_string();
    instance.executable = executable.to_string();
    instance.launch_name = executable.to_string();
    instance.namespace = namespace.to_string();
    instance.parameters.clear();
    instance.callbacks.clear();
    instance.sched_bindings.clear();
    instance.trace.source_metadata = metadata.display().to_string();
    instance.trace.launch_record_entity = package.to_string();
    instance.nodes.truncate(1);
    let node = &mut instance.nodes[0];
    node.id = format!("{package}/{source_node}");
    node.source_node = source_node.to_string();
    node.resolved_name = format!("{namespace}/{executable}");
    node.namespace = namespace.to_string();
    node.entities.clear();
    plan.instances.push(instance);
}

fn write_native_component_config(
    path: &Path,
    package: &str,
    symbol: &str,
    language: &str,
    archive: &Path,
    source_metadata: &str,
) {
    fs::write(
        path,
        format!(
            r#"version = 1
package = "{package}"
component = "{symbol}"
language = "{language}"

[linkage]
crate_name = ""
executable = ""
exported_symbol = "{symbol}"
static_library = "{}"

[metadata]
source_metadata = "{source_metadata}"

[overrides]
parameters = {{}}
remaps = []
"#,
            archive.display()
        ),
    )
    .expect("write native component config");
}

fn write_native_source_metadata(
    path: &Path,
    package: &str,
    symbol: &str,
    language: &str,
    node_id: &str,
    node_name: &str,
    namespace: &str,
) {
    fs::write(
        path,
        format!(
            r#"{{
  "version": 1,
  "package": "{package}",
  "component": "{symbol}",
  "language": "{language}",
  "executable": null,
  "exported_symbol": "{symbol}",
  "nodes": [
    {{
      "id": "{node_id}",
      "name": "{node_name}",
      "namespace": "{namespace}",
      "entities": [],
      "parameters": []
    }}
  ],
  "callbacks": []
}}"#
        ),
    )
    .expect("write native source metadata");
}

fn build_native_counter_archive(
    output: &Path,
    package: &str,
    source_file: &str,
    compiler: &str,
) -> PathBuf {
    let build_dir = output.join(format!("{package}_build"));
    fs::create_dir_all(&build_dir).expect("create native counter build dir");
    let object = build_dir.join("counter.o");
    let archive = build_dir.join(format!("lib{package}.a"));
    let source = fixture_workspace()
        .join("src")
        .join(package)
        .join(source_file);
    let cc_status = Command::new(compiler)
        .arg("-c")
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .status()
        .unwrap_or_else(|error| panic!("compile {package} fixture: {error}"));
    assert!(cc_status.success(), "compile {package} fixture failed");
    let ar_status = Command::new("ar")
        .arg("crs")
        .arg(&archive)
        .arg(&object)
        .status()
        .unwrap_or_else(|error| panic!("archive {package} fixture: {error}"));
    assert!(ar_status.success(), "archive {package} fixture failed");
    archive
}

fn rewrite_entity_id(entity: &mut PlanEntity, old_instance_id: &str, new_instance_id: &str) {
    match entity {
        PlanEntity::Publisher {
            id, resolved_name, ..
        }
        | PlanEntity::Subscriber {
            id, resolved_name, ..
        }
        | PlanEntity::ServiceServer {
            id, resolved_name, ..
        }
        | PlanEntity::ServiceClient {
            id, resolved_name, ..
        }
        | PlanEntity::ActionServer {
            id, resolved_name, ..
        }
        | PlanEntity::ActionClient {
            id, resolved_name, ..
        } => {
            *id = id.replacen(old_instance_id, new_instance_id, 1);
            *resolved_name = resolved_name.replacen("/talker", "/clone/talker", 1);
        }
        PlanEntity::Timer { id, .. } => {
            *id = id.replacen(old_instance_id, new_instance_id, 1);
        }
    }
}

fn assert_freertos_binary_boots(binary: &Path) {
    let output = Command::new("timeout")
        .arg("8s")
        .arg("qemu-system-arm")
        .args([
            "-cpu",
            "cortex-m3",
            "-machine",
            "mps2-an385",
            "-nographic",
            "-semihosting-config",
            "enable=on,target=native",
            "-kernel",
        ])
        .arg(binary)
        .output()
        .unwrap_or_else(|error| panic!("run qemu-system-arm for {}: {error}", binary.display()));
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.code() == Some(124) || output.status.success(),
        "generated FreeRTOS binary exited unexpectedly with {:?}\n{}",
        output.status,
        combined
    );
    // Phase 126.M5 — board prints `nros FreeRTOS Platform` (see
    // `packages/boards/nros-board-freertos/src/node.rs:231`); the
    // historical "QEMU" word was lost when the board crate split
    // platform vs board ID. Match the live banner so generated FreeRTOS
    // binaries are exercised against the same string the runtime emits.
    assert!(
        combined.contains("nros FreeRTOS Platform"),
        "generated FreeRTOS binary did not print platform banner\n{}",
        combined
    );
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_local_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral localhost port");
    listener
        .local_addr()
        .expect("ephemeral listener local address")
        .port()
}

fn start_zenohd(port: u16) -> ChildGuard {
    let zenohd = nano_ros_workspace().join("build/zenohd/zenohd");
    assert!(
        zenohd.is_file(),
        "zenohd binary missing at {}; run `just build-zenohd`",
        zenohd.display()
    );

    let child = Command::new(&zenohd)
        .arg("--listen")
        .arg(format!("tcp/127.0.0.1:{port}"))
        .arg("--no-multicast-scouting")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|error| panic!("spawn zenohd {}: {error}", zenohd.display()));
    let guard = ChildGuard(child);
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return guard;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("zenohd did not listen on tcp/127.0.0.1:{port}");
}

fn assert_generated_binary_spins(binary: &Path, port: u16) {
    let mut child = Command::new(binary)
        .env("NROS_LOCATOR", format!("tcp/127.0.0.1:{port}"))
        .env("NROS_SESSION_MODE", "client")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("spawn generated binary {}: {error}", binary.display()));

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Some(status) = child
            .try_wait()
            .expect("poll generated binary process status")
        {
            let output = child
                .wait_with_output()
                .expect("collect generated binary output");
            panic!(
                "generated binary exited early with {status}\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(50));
    }

    child.kill().expect("stop spinning generated binary");
    let status = child.wait().expect("wait for stopped generated binary");
    assert!(
        !status.success(),
        "generated binary should still be spinning until the test stops it"
    );
}

fn temp_output(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{name}-{}-{stamp}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}
