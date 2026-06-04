## Phase 5: OMG IDL 4.2 Support

**Goal**: Add native support for `.idl` files (OMG IDL 4.2 subset) to enable advanced ROS 2 features and interoperability with native DDS applications.

**Duration**: 6 weeks

**Motivation**:
- `.idl` is the primary format for ROS 2 interfaces (`.msg/.srv/.action` are legacy formats converted to IDL)
- IDL supports features unavailable in `.msg` format: `@key` annotations, `@default` values, `@verbatim` comments, wide strings
- Required for full ROS 2 compatibility and DDS interoperability
- Real-world usage: packages like `rclrs_example_msgs` use `.idl` files directly

**Status**: ✅ Complete (4/4 subphases complete)

| Subphase                      | Status      | Progress                                         |
|-------------------------------|-------------|--------------------------------------------------|
| 5.1: IDL Lexer and Parser     | ✅ Complete | All lexer tests passing, parser functional       |
| 5.2: IDL Code Generation      | ✅ Complete | Constant modules, @default, enums working        |
| 5.3: Integration and Testing  | ✅ Complete | All tests passing, cargo-ros2-bindgen integrated |
| 5.4: Documentation and Polish | ✅ Complete | 194/194 tests passing (100% pass rate)          |

**Latest Achievement**: Phase 5 complete! Fixed constant module parsing order and RMW type path resolution (`crate::rosidl_runtime_rs::` prefix). All 194 tests passing (100%). Full OMG IDL 4.2 support with lexer, parser, code generation, constant modules, @default annotations, and enums.

---

## Architecture and Package Structure

### Learning from ROS 2 Ecosystem

**Key Findings from `external/rosidl_rust`**:
- ROS 2's official generators use Python's `rosidl_parser` package (from `rosidl_python`)
- This parser handles `.msg`, `.srv`, `.action`, AND `.idl` files
- All generators (Python, C++, Rust) consume the same AST from `rosidl_parser.parser.parse_idl_file()`
- The parser produces a unified AST using `rosidl_parser.definition` classes

**Our Approach**:
- Implement pure Rust IDL parser (no Python dependency)
- Extend existing `rosidl-parser` crate to support IDL format
- Produce compatible AST structure for code generation
- Reuse existing `rosidl-codegen` templates with IDL extensions

### Package Structure

Phase 5 will extend existing packages rather than creating new ones:

```
build-tools/
├── rosidl-parser/              # EXTEND THIS
│   ├── src/
│   │   ├── lib.rs
│   │   ├── msg.rs              # Existing: .msg parser
│   │   ├── srv.rs              # Existing: .srv parser
│   │   ├── action.rs           # Existing: .action parser
│   │   ├── idl/                # NEW: IDL parser module
│   │   │   ├── mod.rs          # IDL parser entry point
│   │   │   ├── lexer.rs        # IDL lexer (tokenization)
│   │   │   ├── parser.rs       # IDL parser (AST building)
│   │   │   ├── types.rs        # IDL type definitions
│   │   │   └── ast.rs          # IDL AST nodes
│   │   └── types.rs            # Shared type definitions
│   └── Cargo.toml
│
├── rosidl-codegen/             # EXTEND THIS
│   ├── src/
│   │   ├── lib.rs
│   │   ├── generator.rs        # EXTEND: Add IDL code gen
│   │   ├── types.rs            # EXTEND: Add IDL type mapping
│   │   └── ...
│   ├── templates/              # EXTEND: Add IDL templates
│   │   ├── msg.rs.jinja        # Existing
│   │   ├── srv.rs.jinja        # Existing
│   │   ├── action.rs.jinja     # Existing
│   │   ├── idl_msg.rs.jinja    # NEW: IDL message template
│   │   ├── constants.rs.jinja  # NEW: Constant module template
│   │   └── enum.rs.jinja       # NEW: Enum template
│   └── Cargo.toml
│
└── rosidl-bindgen/             # EXTEND THIS
    ├── src/
    │   ├── lib.rs              # EXTEND: Add .idl file discovery
    │   └── ...
    └── Cargo.toml
```

### Initialization Steps

#### Step 1: Set up IDL parser module structure

```bash
# Create IDL parser module
cd build-tools/rosidl-parser
mkdir -p src/idl
touch src/idl/mod.rs
touch src/idl/lexer.rs
touch src/idl/parser.rs
touch src/idl/types.rs
touch src/idl/ast.rs
```

#### Step 2: Update `rosidl-parser/src/lib.rs`

Add module declaration and public exports:

```rust
// Existing modules
pub mod msg;
pub mod srv;
pub mod action;
pub mod types;

// NEW: IDL module
pub mod idl;

// Re-export IDL parser
pub use idl::{parse_idl_file, IdlFile};
```

#### Step 3: Create IDL templates

```bash
# Create IDL-specific templates
cd build-tools/rosidl-codegen
touch templates/idl_msg.rs.jinja
touch templates/constants.rs.jinja
touch templates/enum.rs.jinja
```

#### Step 4: Add dependencies

Update `rosidl-parser/Cargo.toml`:

```toml
[dependencies]
logos = "0.14"           # Existing: Fast lexer
chumsky = "1.0.0-alpha"  # Existing: Parser combinators
# No new dependencies needed - reuse existing tools
```

#### Step 5: Add tests directory

```bash
# Create test infrastructure
cd build-tools/rosidl-parser
mkdir -p tests/idl
touch tests/idl/test_lexer.rs
touch tests/idl/test_parser.rs
touch tests/idl/test_integration.rs

# Add test fixtures
mkdir -p tests/fixtures/idl
# Copy MyMessage.idl from ros2_rust_examples for testing
cp ../../testing_workspaces/ros2_rust_examples/rclrs/rclrs_example_msgs/msg/MyMessage.idl \
   tests/fixtures/idl/
```

---

### Subphase 5.1: IDL Lexer and Parser (2 weeks)

**Objective**: Implement lexer and parser for OMG IDL 4.2 subset used by ROS 2.

#### Tasks

**Lexer**:
- [ ] Tokenize IDL keywords (`module`, `struct`, `const`, `enum`, `sequence`, etc.)
- [ ] Handle IDL primitive types (short, long, long long, unsigned variants, int8-64, uint8-64, float, double, long double, char, wchar, boolean, octet)
- [ ] Parse string and wide string types (`string`, `wstring`, bounded variants)
- [ ] Handle comments (line `//` and block `/* */`)
- [ ] Parse literals (integers, floats, fixed-point `d`, scientific notation, strings, wide strings with unicode)
- [ ] Handle annotations (`@key`, `@default`, `@verbatim`, `@range`, `@transfer_mode`)
- [ ] Parse module structure and nested modules

**Parser**:
- [ ] Build AST for IDL files
- [ ] Module hierarchy parsing (`package_name` → interface type → definitions)
- [ ] Struct definition parsing (members, multiple members per line)
- [ ] Constant module parsing (nested constant modules like `MyMessage_Constants`)
- [ ] Sequence parsing (bounded and unbounded)
- [ ] Array parsing (fixed-size, multidimensional)
- [ ] Enum parsing
- [ ] Annotation parsing and attachment to AST nodes
- [ ] Import/include statement handling
- [ ] Type reference resolution across modules

#### Example IDL Features to Support

```idl
module package_name {
  module msg {
    // Constant module
    module MyMessage_Constants {
      const short SHORT_CONSTANT = -23;
      const unsigned long UNSIGNED_LONG_CONSTANT = 42;
      const float FLOAT_CONSTANT = 1.25;
      const boolean BOOLEAN_CONSTANT = TRUE;
      const string STRING_CONSTANT = "string_value";
      const wstring WSTRING_CONSTANT = "wstring_value_™";
    };

    // Struct with annotations
    @verbatim(language="comment", text="Documentation of MyMessage.")
    @transfer_mode(SHMEM_REF)
    struct MyMessage {
      short short_value, short_value2;  // Multiple members per line

      @default(value=123)
      unsigned short unsigned_short_value;

      @key
      @range(min=-10, max=10)
      long long_value;

      string string_value;
      string<5> bounded_string_value;
      wstring wstring_value;
      wstring<23> bounded_wstring_value;

      sequence<short> unbounded_short_values;
      sequence<short, 5> bounded_short_values;
      sequence<string<3>> unbounded_values_of_bounded_strings;
      sequence<string<3>, 4> bounded_values_of_bounded_strings;

      short array_short_values[23];

      // Scientific notation and fixed-point
      @default(value=1.9e10)
      float int_and_frac_with_positive_scientific;
      @default(value=8.7d)
      float fixed_int_and_frac;
    };
  };
};
```

#### Testing

- [ ] Unit tests for lexer (20 tests)
  - Primitive types
  - String literals (regular and wide)
  - Numeric literals (integer, float, scientific, fixed-point)
  - Annotations
  - Comments
  - Edge cases

- [ ] Unit tests for parser (30 tests)
  - Module hierarchy
  - Struct definitions
  - Constant modules
  - Sequences (bounded/unbounded)
  - Arrays (fixed-size)
  - Annotations
  - Complex nested structures
  - Parse `MyMessage.idl` from rclrs_example_msgs

- [ ] Integration tests
  - Parse real ROS 2 IDL files from `/opt/ros/jazzy/share/*/msg/*.idl`
  - Verify AST correctness
  - Test error handling for invalid IDL

**Acceptance**:
```bash
cargo test --package rosidl-parser -- idl
# → All IDL parser tests pass
```

---

---

### Subphase 5.2: IDL Code Generation (2 weeks)

**Objective**: Generate Rust bindings from IDL AST with full support for IDL features.

#### Initialization Steps

**Step 1: Extend rosidl-codegen types**

```bash
cd build-tools/rosidl-codegen/src
# Edit types.rs to add IDL-specific type mappings
```

Add to `types.rs`:

```rust
// IDL-specific type mappings
pub fn idl_primitive_to_rust(idl_type: &str) -> &str {
    match idl_type {
        "short" => "i16",
        "unsigned short" => "u16",
        "long" => "i32",
        "unsigned long" => "u32",
        "long long" => "i64",
        "unsigned long long" => "u64",
        "octet" => "u8",
        "char" => "u8",
        "wchar" => "u16",
        "boolean" => "bool",
        "float" => "f32",
        "double" => "f64",
        _ => panic!("Unknown IDL primitive type: {}", idl_type),
    }
}

// Wide string handling
pub fn is_wide_string(type_name: &str) -> bool {
    type_name.starts_with("wstring")
}
```

**Step 2: Create template helpers for IDL features**

Add to `generator.rs`:

```rust
// Helper for constant module generation
pub fn generate_constant_module(module_name: &str, constants: &[Constant]) -> String {
    // Template rendering logic
}

// Helper for enum generation
pub fn generate_enum(enum_def: &EnumDef) -> String {
    // Template rendering logic
}

// Helper for default value generation
pub fn format_default_value(value: &DefaultValue, field_type: &Type) -> String {
    // Format default values for different types
}
```

**Step 3: Create IDL templates**

Create `templates/constants.rs.jinja`:

```jinja
// Constant module for {{ message_name }}
pub mod {{ module_name }} {
    {% for constant in constants %}
    pub const {{ constant.name }}: {{ constant.rust_type }} = {{ constant.value }};
    {% endfor %}
}
```

Create `templates/enum.rs.jinja`:

```jinja
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum {{ enum_name }} {
    {% for variant in variants %}
    {{ variant }},
    {% endfor %}
}
```

**Step 4: Update existing templates for IDL support**

Edit `templates/msg.rs.jinja` to support:
- Wide strings (`wstring`)
- Default values from `@default`
- Documentation from `@verbatim`

**Step 5: Add IDL code generation tests**

```bash
cd build-tools/rosidl-codegen
mkdir -p tests/idl
touch tests/idl/test_constant_generation.rs
touch tests/idl/test_enum_generation.rs
touch tests/idl/test_default_values.rs
touch tests/idl/test_wstring.rs
```

#### Tasks

**Template Updates**:
- [ ] Extend existing templates to handle IDL-specific features
- [ ] Generate code for constant modules (nested module structure)
- [ ] Handle annotations in generated code
  - `@key` → Mark fields for DDS keyed topics
  - `@default` → Generate Default trait with specified values
  - `@verbatim` → Generate doc comments
  - `@range`, `@transfer_mode` → Preserve in comments for future use
- [ ] Support wide strings (`wstring`, `wstring<N>`)
- [ ] Generate enums from IDL enum definitions
- [ ] Handle multiple members per line (`short a, b, c;`)

**Type Mapping**:
- [ ] Map IDL primitives to Rust types
  - `short` → `i16`
  - `unsigned short` → `u16`
  - `long` → `i32`
  - `unsigned long` → `u32`
  - `long long` → `i64`
  - `unsigned long long` → `u64`
  - `octet` → `u8`
  - `char` → `u8`
  - `wchar` → `u16`
  - `boolean` → `bool`
  - `float` → `f32`
  - `double` → `f64`
  - `long double` → Not supported (comment in generated code)
- [ ] Handle bounded sequences and strings
- [ ] Generate array types (fixed-size)
- [ ] Support nested types and imports

**Default Value Generation**:
- [ ] Parse `@default` annotation values
- [ ] Generate Default trait implementations with specified values
- [ ] Handle different literal types (integer, float, scientific, fixed-point, boolean, string)
- [ ] Validate default values match field types

**Constant Module Generation**:
- [ ] Generate nested constant modules (e.g., `MyMessage_Constants`)
- [ ] Support all constant types (numeric, boolean, string, wstring)
- [ ] Handle unicode in wide string constants
- [ ] Generate proper Rust constant syntax

#### Example Generated Code

From IDL:
```idl
module package_name {
  module msg {
    module MyMessage_Constants {
      const short SHORT_CONSTANT = -23;
      const string STRING_CONSTANT = "value";
    };

    struct MyMessage {
      @key
      long id;
      @default(value=123)
      unsigned short count;
      string<10> name;
    };
  };
};
```

Generate:
```rust
// Constant module
pub mod my_message_constants {
    pub const SHORT_CONSTANT: i16 = -23;
    pub const STRING_CONSTANT: &'static str = "value";
}

// Message struct
#[derive(Debug, Clone, PartialEq)]
pub struct MyMessage {
    pub id: i32,        // @key field
    pub count: u16,
    pub name: rosidl_runtime_rs::BoundedString<10>,
}

impl Default for MyMessage {
    fn default() -> Self {
        Self {
            id: 0,
            count: 123,  // From @default annotation
            name: rosidl_runtime_rs::BoundedString::default(),
        }
    }
}
```

#### Testing

- [ ] Unit tests for code generation (25 tests)
  - Primitive types
  - Sequences (bounded/unbounded)
  - Arrays
  - Constant modules
  - Default values
  - Annotations
  - Wide strings

- [ ] Integration tests (10 tests)
  - Generate bindings for `MyMessage.idl`
  - Compile generated code
  - Verify Default trait with custom values
  - Test constant module access
  - Verify FFI compatibility

**Acceptance**:
```bash
cargo test --package rosidl-codegen -- idl
# → All IDL codegen tests pass
```

---

---

### Subphase 5.3: IDL Integration and Testing (1 week)

**Objective**: Integrate IDL support into `cargo-ros2-bindgen` and test with real packages.

#### Initialization Steps

**Step 1: Update rosidl-bindgen for IDL file discovery**

Edit `build-tools/rosidl-bindgen/src/lib.rs`:

```rust
// Add IDL file discovery
pub fn discover_idl_files(package_path: &Path) -> Vec<PathBuf> {
    let mut idl_files = Vec::new();

    for dir in &["msg", "srv", "action"] {
        let idl_dir = package_path.join(dir);
        if idl_dir.exists() {
            for entry in std::fs::read_dir(&idl_dir)? {
                let path = entry?.path();
                if path.extension().and_then(|s| s.to_str()) == Some("idl") {
                    idl_files.push(path);
                }
            }
        }
    }

    idl_files
}
```

**Step 2: Update binding generator to handle IDL files**

Edit `build-tools/rosidl-bindgen/src/lib.rs`:

```rust
pub fn generate_package_bindings(package_path: &Path, output_dir: &Path) -> Result<()> {
    // Discover all interface files
    let msg_files = discover_msg_files(package_path);  // Existing
    let srv_files = discover_srv_files(package_path);  // Existing
    let action_files = discover_action_files(package_path);  // Existing
    let idl_files = discover_idl_files(package_path);  // NEW

    // Generate bindings for each format
    for msg_file in msg_files {
        generate_msg_bindings(&msg_file, output_dir)?;
    }

    // NEW: Generate bindings for IDL files
    for idl_file in idl_files {
        generate_idl_bindings(&idl_file, output_dir)?;
    }

    Ok(())
}

fn generate_idl_bindings(idl_file: &Path, output_dir: &Path) -> Result<()> {
    use rosidl_parser::idl::parse_idl_file;
    use rosidl_codegen::generate_idl_code;

    let idl_ast = parse_idl_file(idl_file)?;
    let rust_code = generate_idl_code(&idl_ast)?;

    // Write generated code
    let output_file = output_dir.join(format!("{}.rs", idl_ast.name));
    std::fs::write(&output_file, rust_code)?;

    Ok(())
}
```

**Step 3: Update workspace binding generator**

Edit `build-tools/colcon-cargo-ros2/colcon_cargo_ros2/workspace_bindgen.py`:

```python
def _generate_bindings(self, ros_packages: Dict[str, Path], verbose: bool):
    """Generate Rust bindings for all ROS packages."""

    for pkg_name, pkg_share in ros_packages.items():
        # Check if package has interfaces (msg/, srv/, action/ directories)
        has_interfaces = any([
            (pkg_share / 'msg').exists(),
            (pkg_share / 'srv').exists(),
            (pkg_share / 'action').exists(),
        ])

        if not has_interfaces:
            continue

        # Check for .idl files in addition to .msg/.srv/.action
        has_idl = False
        for interface_dir in ['msg', 'srv', 'action']:
            dir_path = pkg_share / interface_dir
            if dir_path.exists():
                idl_files = list(dir_path.glob('*.idl'))
                if idl_files:
                    has_idl = True
                    logger.info(f"Found {len(idl_files)} .idl files in {pkg_name}/{interface_dir}/")

        # Generate bindings (now handles both .msg and .idl)
        self._run_bindgen(pkg_name, pkg_share, self.bindings_dir, verbose)
```

**Step 4: Create test workspace enhancements**

```bash
# Add IDL files to complex_workspace
cd testing_workspaces/complex_workspace/src/robot_interfaces

# Create directories if they don't exist
mkdir -p msg srv action

# We'll add the IDL files manually following the examples in Phase 5 doc
```

**Step 5: Set up integration tests**

```bash
cd build-tools/rosidl-bindgen
mkdir -p tests/integration/idl
touch tests/integration/idl/test_idl_package.rs
touch tests/integration/idl/test_mixed_package.rs  # .msg + .idl
touch tests/integration/idl/test_builtin_idl.rs    # System packages
```

#### Tasks

**Integration**:
- [ ] Update `rosidl-bindgen` to detect `.idl` files in package directories
- [ ] Add IDL file discovery to package scanning
  - Check `msg/*.idl`, `srv/*.idl`, `action/*.idl`
  - Include IDL files in dependency graph
- [ ] Update workspace binding generator to discover IDL files
- [ ] Ensure proper mixing of `.msg` and `.idl` files in same package
- [ ] Update file enumeration logic in binding generator

**Testing**:
- [ ] Test with `rclrs_example_msgs` package (contains `MyMessage.idl`)
- [ ] Generate bindings for all `.idl` files in `/opt/ros/jazzy/share/*/msg/`
- [ ] Verify mixed packages (both `.msg` and `.idl` files)
- [ ] Test workspace-level binding generation with IDL files
- [ ] Ensure backward compatibility with `.msg`-only packages

**Examples**:
- [ ] Create example package using IDL features
  - Keyed messages (`@key` annotation)
  - Default values (`@default` annotation)
  - Wide strings
  - Constant modules
- [ ] Document IDL-specific features in examples

**Bug Fixes**:
- [ ] Fix any issues discovered during integration
- [ ] Ensure proper error messages for invalid IDL
- [ ] Handle edge cases (empty modules, nested imports, etc.)

#### Test Cases

- [ ] Build `ros2_rust_examples` workspace (includes `MyMessage.idl`)
  - Should succeed without errors
  - `examples_rclrs_message_demo` should compile
  - Verify `MyMessage` is accessible in Rust code

- [ ] Revise and enhance `complex_workspace` with IDL files
  - **Goal**: Create comprehensive test suite for IDL support
  - **Location**: `testing_workspaces/complex_workspace/`

  **Tasks**:
  - [ ] Add custom IDL files to `robot_interfaces` package
    - Create `robot_interfaces/msg/DiagnosticInfo.idl` with annotations
      - Use `@key` annotation for keyed topics
      - Use `@default` annotation for default values
      - Include constant module (e.g., `DiagnosticInfo_Constants`)
      - Use wide strings (`wstring`)
      - Include enums for diagnostic levels
    - Create `robot_interfaces/srv/CalibrateSensor.idl`
      - Service with IDL-specific features
      - Bounded sequences and strings
      - Multiple annotations
    - Create `robot_interfaces/action/Dock.idl`
      - Action with goal/result/feedback using IDL
      - Use `@verbatim` for documentation
      - Include scientific notation in default values

  - [ ] Use ROS builtin IDL packages in `robot_controller`
    - Update `robot_controller/Cargo.toml` dependencies
      - Add dependency on a ROS package that uses `.idl` format
      - Example: Use `sensor_msgs` IDL files if available
      - Example: Use `diagnostic_msgs` IDL files
    - Update `robot_controller/src/main.rs` to use IDL messages
      - Create publisher for custom IDL message (`DiagnosticInfo`)
      - Call custom IDL service (`CalibrateSensor`)
      - Use action client with custom IDL action (`Dock`)
      - Access constant module values
      - Demonstrate default value usage

  - [ ] Mix `.msg` and `.idl` files in same package
    - Keep existing `.msg` files in `robot_interfaces`
    - Add new `.idl` files alongside them
    - Verify both formats are discovered and generated
    - Test cross-references (`.msg` → `.idl` and vice versa)

  - [ ] Create comprehensive test scenarios
    - **Scenario 1**: Keyed topics with `@key` annotation
      - Use `DiagnosticInfo` with keyed field (robot_id)
      - Test multiple instances per topic
    - **Scenario 2**: Default values from `@default`
      - Instantiate messages without setting all fields
      - Verify defaults are applied correctly
    - **Scenario 3**: Wide string support
      - Send messages with unicode characters in wstring fields
      - Verify proper encoding/decoding
    - **Scenario 4**: Constant module access
      - Use constants from `DiagnosticInfo_Constants`
      - Verify they're accessible in Rust code
    - **Scenario 5**: Enums from IDL
      - Use diagnostic level enum
      - Pattern match on enum values

  - [ ] Document IDL usage patterns
    - Create `complex_workspace/README.md` section on IDL
    - Explain each custom IDL file and its purpose
    - Provide code examples showing IDL feature usage
    - Document differences between `.msg` and `.idl` approach

  **File Structure**:
  ```
  complex_workspace/
  ├── src/
  │   ├── robot_interfaces/
  │   │   ├── msg/
  │   │   │   ├── RobotStatus.msg        # Existing
  │   │   │   ├── SensorReading.msg      # Existing
  │   │   │   └── DiagnosticInfo.idl     # NEW - Custom IDL with annotations
  │   │   ├── srv/
  │   │   │   ├── SetMode.srv            # Existing
  │   │   │   └── CalibrateSensor.idl    # NEW - Custom IDL service
  │   │   ├── action/
  │   │   │   ├── Navigate.action        # Existing
  │   │   │   └── Dock.idl               # NEW - Custom IDL action
  │   │   ├── CMakeLists.txt
  │   │   └── package.xml
  │   └── robot_controller/
  │       ├── src/
  │       │   ├── main.rs                # Updated to use IDL features
  │       │   ├── idl_examples.rs        # NEW - IDL feature demonstrations
  │       │   └── lib.rs
  │       ├── Cargo.toml                 # Add IDL package dependencies
  │       └── package.xml
  └── README.md                          # Document IDL test cases
  ```

  **Expected Outcomes**:
  - All packages build successfully
  - Custom IDL messages/services/actions are usable
  - Annotations work correctly (keyed topics, defaults, etc.)
  - Mixed `.msg` and `.idl` packages work seamlessly
  - Provides comprehensive test coverage for IDL implementation

- [ ] System test: Generate bindings for all ROS 2 packages
  ```bash
  # Should handle all standard ROS 2 packages with IDL files
  cargo ros2 build
  ```

#### Example IDL Files for complex_workspace

**DiagnosticInfo.idl** (Message with annotations):
```idl
module robot_interfaces {
  module msg {
    // Constant module for diagnostic levels
    module DiagnosticInfo_Constants {
      const octet LEVEL_OK = 0;
      const octet LEVEL_WARN = 1;
      const octet LEVEL_ERROR = 2;
      const octet LEVEL_FATAL = 3;
      const string STATUS_OK = "OK";
      const string STATUS_ERROR = "ERROR";
    };

    // Enum for diagnostic levels
    enum DiagnosticLevel {
      OK,
      WARN,
      ERROR,
      FATAL
    };

    @verbatim(language="comment", text="Diagnostic information with keyed topics support")
    struct DiagnosticInfo {
      // Key field for multi-robot scenarios
      @key
      @range(min=0, max=255)
      unsigned long robot_id;

      // Diagnostic level with default
      @default(value=0)
      DiagnosticLevel level;

      // Unicode-capable message
      wstring<256> message;

      // Timestamp with scientific notation default
      @default(value=1.0e9)
      double timestamp;

      // Hardware ID with bounded string
      string<32> hardware_id;

      // Sequence of error codes
      sequence<unsigned short, 10> error_codes;

      // Values array
      float values[8];
    };
  };
};
```

**CalibrateSensor.idl** (Service):
```idl
module robot_interfaces {
  module srv {
    @verbatim(language="comment", text="Calibrate a robot sensor")
    struct CalibrateSensor_Request {
      string<32> sensor_name;

      @default(value=100)
      unsigned short sample_count;

      @default(value=1.0)
      float timeout_seconds;

      sequence<float, 16> calibration_params;
    };

    struct CalibrateSensor_Response {
      boolean success;
      wstring<128> message;

      @default(value=0.0)
      float accuracy;

      sequence<float> measured_values;
    };
  };
};
```

**Dock.idl** (Action):
```idl
module robot_interfaces {
  module action {
    module Dock_Constants {
      const float DEFAULT_APPROACH_SPEED = 0.1;
      const float DEFAULT_ALIGNMENT_THRESHOLD = 0.01;
    };

    @verbatim(language="comment", text="Dock the robot at a charging station")
    struct Dock_Goal {
      unsigned long station_id;

      @default(value=0.1)
      float approach_speed;

      @default(value=true)
      boolean use_vision;
    };

    struct Dock_Result {
      boolean success;
      wstring message;

      @default(value=0.0)
      float final_alignment_error;

      @default(value=0.0)
      double docking_duration;
    };

    struct Dock_Feedback {
      float distance_to_station;
      float alignment_error;
      unsigned short progress_percent;
      wstring<64> current_phase;
    };
  };
};
```

**Acceptance**:
```bash
cd testing_workspaces/ros2_rust_examples
colcon build
# → All packages build successfully, including examples_rclrs_message_demo ✅
```

---

### Subphase 5.4: Documentation and Polish (1 week)

**Objective**: Document IDL support and ensure production quality.

#### Tasks

**Documentation**:
- [ ] Update `DESIGN.md` with IDL parser architecture
- [ ] Document IDL feature support in README
- [ ] Create IDL usage guide
  - When to use `.idl` vs `.msg`
  - How to write IDL files for ROS 2
  - Supported annotations and their effects
  - Examples of advanced features
- [ ] Update CLI reference with IDL-related flags/options
- [ ] Add IDL examples to example projects

**Code Quality**:
- [ ] Ensure all IDL code passes clippy
- [ ] Add inline documentation for IDL parser
- [ ] Improve error messages for IDL parsing
- [ ] Add helpful hints for common IDL mistakes

**Testing**:
- [ ] Achieve >80% test coverage for IDL code
- [ ] Add regression tests for known issues
- [ ] Performance testing (IDL vs MSG parsing speed)

**Benchmarks**:
- [ ] Compare parsing performance: `.idl` vs `.msg`
- [ ] Measure code generation time for complex IDL files
- [ ] Ensure no significant performance regression

**Acceptance**:
```bash
just quality
# → All checks pass, including IDL code ✅

cargo test --package rosidl-parser --package rosidl-codegen
# → 150+ tests pass (including 50+ IDL tests) ✅
```

---

## Success Criteria

### Technical Requirements

- ✅ Parse all OMG IDL 4.2 features used by ROS 2
- ✅ Generate correct Rust bindings from IDL files
- ✅ Support all annotations: `@key`, `@default`, `@verbatim`, `@range`, `@transfer_mode`
- ✅ Handle constant modules
- ✅ Support wide strings (wstring)
- ✅ Mix `.msg` and `.idl` files in same package
- ✅ Backward compatible with existing `.msg`-only packages

### Quality Requirements

- ✅ All tests pass (50+ new tests for IDL)
- ✅ Zero clippy warnings
- ✅ Test coverage >80%
- ✅ Performance: IDL parsing <10% slower than MSG
- ✅ Comprehensive documentation

### Real-World Validation

- ✅ `rclrs_example_msgs` builds successfully
- ✅ `examples_rclrs_message_demo` compiles and runs
- ✅ Can process all IDL files in `/opt/ros/jazzy/share/`
- ✅ Advanced features (keyed topics, defaults) work correctly

---

## Dependencies

**Requires**:
- Phase 1 complete (parser and codegen infrastructure)
- Phase 2 complete (cargo-ros2 tools)
- Phase 4.1.3 complete (workspace package discovery)

**Enables**:
- Full ROS 2 interface compatibility
- DDS interoperability
- Advanced messaging features (keyed topics)
- User-defined default values

---

## Risks and Mitigations

**Risk**: IDL grammar is complex and may have edge cases
- **Mitigation**: Start with ROS 2 subset, expand incrementally
- **Mitigation**: Test with real ROS 2 IDL files from system packages

**Risk**: Maintaining two parsers (.msg and .idl) increases complexity
- **Mitigation**: Share code generation infrastructure
- **Mitigation**: Consider unified AST representation

**Risk**: IDL features may not map cleanly to Rust
- **Mitigation**: Document limitations clearly
- **Mitigation**: Use comments to preserve unsupported annotations

---

## Future Enhancements (Post-Phase 5)

- Support full OMG IDL 4.2 (beyond ROS 2 subset)
- IDL → MSG converter tool
- Automatic migration guide for .msg to .idl
- Support for unions and other advanced IDL constructs
- Integration with DDS-specific features (QoS policies in IDL)

---

## References

- [ROS 2 IDL Specification](https://design.ros2.org/articles/idl_interface_definition.html)
- [OMG IDL 4.2 Spec](https://www.omg.org/spec/IDL/4.2/)
- [Legacy Interface Definition](https://design.ros2.org/articles/legacy_interface_definition.html)
- [Example: MyMessage.idl](../../testing_workspaces/ros2_rust_examples/rclrs/rclrs_example_msgs/msg/MyMessage.idl)
