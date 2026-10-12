//! phase-492 W7/W8 (RFC-0089, 2026-10-12) — what a ported C file must NOT be
//! able to compile against nano-ros's rosidl-shaped message surface.
//!
//! RFC-0089 adopts `ROSIDL_GET_MSG_TYPE_SUPPORT` (it yields nano-ros's
//! descriptor) and rosidl's message names and member shape, and REFUSES the
//! parts that would compile and differ: the `rosidl_message_type_support_t`
//! struct and its members, a pre-allocation object for `rcl_publish`, and
//! rosidl's allocating `<Msg>__init`. Each refusal is a `cxx-syntax-verdict`
//! row (issue 1620's shape for a compile that must FAIL): the build stage runs
//! the C11 compile of `fixtures/cpp_compat_snippets/<id>.c` against the
//! committed rosidl-laid-out codegen goldens and records its verdict; this test
//! asserts it.
//!
//! Every assertion is TWO-sided: the compile failed, AND it failed for the
//! stated reason. A snippet that stopped compiling because a header moved
//! would otherwise read as the refusal holding.

use nros_tests::TestResult;

fn assert_refused(id: &str, because: &[&str], what: &str) -> TestResult<()> {
    let v = nros_tests::fixtures::require_compile_verdict(id)?;
    assert!(
        !v.outcome.success(),
        "{id} COMPILED — {what} is reachable from a C translation unit, which \
         RFC-0089 refuses"
    );
    for needle in because {
        assert!(
            v.outcome.stderr.contains(needle),
            "{id} failed, but not on `{needle}` — fix the snippet or the include \
             set before believing the refusal:\n{}",
            v.outcome.stderr
        );
    }
    Ok(())
}

/// `const rosidl_message_type_support_t *ts = ROSIDL_GET_MSG_TYPE_SUPPORT(...)`
/// — the macro is adopted, the struct is not declared.
#[test]
fn rosidl_typesupport_struct_is_undeclared() -> TestResult<()> {
    assert_refused(
        "c_rosidl_typesupport_struct_refused",
        &["rosidl_message_type_support_t"],
        "rosidl_message_type_support_t",
    )
}

/// `ROSIDL_GET_MSG_TYPE_SUPPORT(...)->typesupport_identifier` — the descriptor
/// carries no rosidl dispatcher members.
#[test]
fn rosidl_typesupport_members_are_absent() -> TestResult<()> {
    assert_refused(
        "c_rosidl_typesupport_member_refused",
        &["typesupport_identifier"],
        "a rosidl typesupport member",
    )
}

/// `rmw_publisher_allocation_t allocation;` — the type is incomplete, so only
/// a NULL pointer to one can be passed to `rcl_publish`.
#[test]
fn a_stack_publisher_allocation_is_refused() -> TestResult<()> {
    assert_refused(
        "c_rosidl_stack_publisher_allocation_refused",
        &["allocation"],
        "a complete rmw_publisher_allocation_t",
    )
}

/// `std_msgs__msg__String__init(&msg)` — rosidl's allocating init is not
/// declared, and the diagnostic names the caller-owned alternative.
#[test]
fn rosidl_allocating_init_names_init_in() -> TestResult<()> {
    assert_refused(
        "c_rosidl_allocating_init_refused",
        &["std_msgs__msg__String__init_in"],
        "rosidl's allocating std_msgs__msg__String__init",
    )
}
