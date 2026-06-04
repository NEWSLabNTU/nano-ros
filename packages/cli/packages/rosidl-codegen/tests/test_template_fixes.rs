use rosidl_codegen::{GeneratorError, generate_message_package};
use rosidl_parser::parse_message;
use std::collections::HashSet;

#[test]
fn test_lib_rs_has_clone_trait_bounds() -> Result<(), GeneratorError> {
    let msg_def = "int32 x\n";
    let msg = parse_message(msg_def).unwrap();
    let deps = HashSet::new();
    let result = generate_message_package("test_msgs", "Simple", &msg, &deps)?;

    // Verify Clone trait bounds are present in Message trait
    assert!(
        result.lib_rs.contains("Self: Sized + Clone"),
        "lib.rs should contain 'Self: Sized + Clone' trait bound"
    );
    assert!(
        result.lib_rs.contains("Self::RmwMsg: Clone"),
        "lib.rs should contain 'Self::RmwMsg: Clone' trait bound"
    );

    Ok(())
}

#[test]
fn test_idiomatic_uses_snake_case_modules() -> Result<(), GeneratorError> {
    let msg_def = "int32 x\n";
    let msg = parse_message(msg_def).unwrap();
    let deps = HashSet::new();
    let result = generate_message_package("test_msgs", "Duration", &msg, &deps)?;

    // Verify RMW layer uses msg::rmw module path
    assert!(
        result
            .message_idiomatic
            .contains("crate::msg::rmw::Duration"),
        "Idiomatic layer should reference RMW types via msg::rmw"
    );
    assert!(
        !result.message_idiomatic.contains("crate::msg::Duration::"),
        "Idiomatic layer should not use direct msg::Type paths for RMW references"
    );

    Ok(())
}
