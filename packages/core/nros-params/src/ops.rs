//! `ParameterServer`'s operations, behind a sealed trait (phase-515 R10 stage 2).
//!
//! The node is the user-facing parameter receiver, as in rclrs (maintainer
//! decision 2026-10-27): a Rust user declares, reads and writes parameters
//! through the `Node` that `create_node` / `node_mut(id)` returns. The store
//! itself is the executor's one table. The code that implements those verbs
//! (`nros-node`, the C and C++ shims), and a caller holding a standalone store,
//! reach it through [`ParameterServerOps`]. `nros` never re-exports the trait,
//! so a `nros` user does not see these methods.
//!
//! Each method is a crate-private inherent method of [`ParameterServer`]; the
//! trait forwards to it, and nothing else implements it.
#![allow(clippy::too_many_arguments)]

use crate::*;

mod sealed {
    pub trait Sealed {}
}
impl sealed::Sealed for ParameterServer<'_> {}

/// `ParameterServer`'s operations, for the crates that implement the
/// parameter verbs. Sealed.
pub trait ParameterServerOps<'s>: sealed::Sealed + Sized {
    /// See `ParameterServer::take_truncated_descriptions` in `server.rs`.
    fn take_truncated_descriptions(&mut self, report: impl FnMut(NodeKey, &str)) -> usize;

    /// See `ParameterServer::new_in` in `server.rs`.
    fn new_in(table: ParameterTable<'s>) -> Self;

    /// See `ParameterServer::set_allow_undeclared` in `server.rs`.
    fn set_allow_undeclared(&mut self, node: NodeKey, allow: bool);

    /// See `ParameterServer::allows_undeclared` in `server.rs`.
    fn allows_undeclared(&self, node: NodeKey) -> bool;

    /// See `ParameterServer::set_parameter_events` in `server.rs`.
    fn set_parameter_events(&mut self, node: NodeKey, on: bool);

    /// See `ParameterServer::parameter_events_enabled` in `server.rs`.
    fn parameter_events_enabled(&self, node: NodeKey) -> bool;

    /// See `ParameterServer::pop_initial_parameter_event` in `server.rs`.
    fn pop_initial_parameter_event(&mut self) -> Option<PendingParameterEvent>;

    /// See `ParameterServer::pop_parameter_event` in `server.rs`.
    fn pop_parameter_event(&mut self) -> Option<PendingParameterEvent>;

    /// See `ParameterServer::take_parameter_events_dropped` in `server.rs`.
    fn take_parameter_events_dropped(&mut self) -> u32;

    /// See `ParameterServer::set_delete_on_not_set` in `server.rs`.
    fn set_delete_on_not_set(&mut self, node: NodeKey, on: bool);

    /// See `ParameterServer::len` in `server.rs`.
    fn len(&self, node: NodeKey) -> usize;

    /// See `ParameterServer::is_empty` in `server.rs`.
    fn is_empty(&self, node: NodeKey) -> bool;

    /// See `ParameterServer::total_len` in `server.rs`.
    fn total_len(&self) -> usize;

    /// See `ParameterServer::capacity` in `server.rs`.
    fn capacity(&self) -> usize;

    /// See `ParameterServer::is_full` in `server.rs`.
    fn is_full(&self) -> bool;

    /// See `ParameterServer::declare` in `server.rs`.
    fn declare(&mut self, node: NodeKey, name: &str, value: ParameterValue) -> bool;

    /// See `ParameterServer::declare_ref` in `server.rs`.
    fn declare_ref(&mut self, node: NodeKey, name: &str, value: &ParameterValue) -> bool;

    /// See `ParameterServer::declare_with_descriptor` in `server.rs`.
    fn declare_with_descriptor(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
        descriptor: Option<ParameterDescriptor>,
    ) -> bool;

    /// See `ParameterServer::try_declare` in `server.rs`.
    fn try_declare(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
    ) -> Result<(), DeclarationError>;

    /// See `ParameterServer::try_declare_with_descriptor` in `server.rs`.
    fn try_declare_with_descriptor(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
        descriptor: Option<ParameterDescriptor>,
    ) -> Result<(), DeclarationError>;

    /// See `ParameterServer::try_declare_ref` in `server.rs`.
    fn try_declare_ref(
        &mut self,
        node: NodeKey,
        name: &str,
        value: &ParameterValue,
        descriptor: Option<ParameterDescriptor>,
    ) -> Result<(), DeclarationError>;

    /// See `ParameterServer::check_apply` in `server.rs`.
    fn check_apply(
        &self,
        node: NodeKey,
        name: &str,
        value: &ParameterValue,
    ) -> Result<(), ParameterError>;

    /// See `ParameterServer::apply` in `server.rs`.
    fn apply(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
    ) -> Result<(), ParameterError>;

    /// See `ParameterServer::apply_ref` in `server.rs`.
    fn apply_ref(
        &mut self,
        node: NodeKey,
        name: &str,
        value: &ParameterValue,
    ) -> Result<(), ParameterError>;

    /// See `ParameterServer::apply_staged` in `server.rs`.
    fn apply_staged(
        &mut self,
        node: NodeKey,
        name: &str,
        staged: &mut ParameterValue,
    ) -> Result<(), ParameterError>;

    /// See `ParameterServer::set_local` in `server.rs`.
    fn set_local(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
    ) -> Result<(), ParameterError>;

    /// See `ParameterServer::add_on_set_parameters_callback` in `server.rs`.
    fn add_on_set_parameters_callback(
        &mut self,
        node: NodeKey,
        callback: OnSetParameterFn,
        ctx: OnSetContext,
    ) -> Option<OnSetParameterHandle>;

    /// See `ParameterServer::remove_on_set_parameters_callback` in `server.rs`.
    fn remove_on_set_parameters_callback(&mut self, handle: OnSetParameterHandle) -> bool;

    /// See `ParameterServer::on_set_callbacks_free` in `server.rs`.
    fn on_set_callbacks_free(&self) -> usize;

    /// See `ParameterServer::set_parameter_description` in `server.rs`.
    fn set_parameter_description(
        &mut self,
        node: NodeKey,
        name: &str,
        description: &str,
        additional_constraints: &str,
    ) -> bool;

    /// See `ParameterServer::set_parameter_read_only` in `server.rs`.
    fn set_parameter_read_only(&mut self, node: NodeKey, name: &str, read_only: bool) -> bool;

    /// See `ParameterServer::set_parameter_integer_range` in `server.rs`.
    fn set_parameter_integer_range(
        &mut self,
        node: NodeKey,
        name: &str,
        min: i64,
        max: i64,
        step: i64,
    ) -> bool;

    /// See `ParameterServer::set_parameter_float_range` in `server.rs`.
    fn set_parameter_float_range(
        &mut self,
        node: NodeKey,
        name: &str,
        min: f64,
        max: f64,
        step: f64,
    ) -> bool;

    /// See `ParameterServer::apply_atomically` in `server.rs`.
    fn apply_atomically<'x, I>(&mut self, node: NodeKey, items: I) -> Result<(), ParameterError>
    where
        I: Iterator<Item = Option<(&'x str, ParameterValue)>> + Clone;

    /// See `ParameterServer::apply_atomically_with` in `server.rs`.
    fn apply_atomically_with<'x>(
        &mut self,
        node: NodeKey,
        count: usize,
        fill: impl FnMut(usize, &mut ParameterValue) -> Option<&'x str>,
    ) -> Result<(), ParameterError>;

    /// See `ParameterServer::get` in `server.rs`.
    fn get(&self, node: NodeKey, name: &str) -> Option<&ParameterValue>;

    /// See `ParameterServer::get_parameter` in `server.rs`.
    fn get_parameter(&self, node: NodeKey, name: &str) -> Option<&Parameter>;

    /// See `ParameterServer::get_descriptor` in `server.rs`.
    fn get_descriptor(&self, node: NodeKey, name: &str) -> Option<&ParameterDescriptor>;

    /// See `ParameterServer::set` in `server.rs`.
    fn set(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
    ) -> Result<(), ParameterError>;

    /// See `ParameterServer::set_ref` in `server.rs`.
    fn set_ref(
        &mut self,
        node: NodeKey,
        name: &str,
        value: &ParameterValue,
    ) -> Result<(), ParameterError>;

    /// See `ParameterServer::unset` in `server.rs`.
    fn unset(&mut self, node: NodeKey, name: &str) -> Result<(), ParameterError>;

    /// See `ParameterServer::set_or_declare` in `server.rs`.
    fn set_or_declare(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
    ) -> Result<(), ParameterError>;

    /// See `ParameterServer::has` in `server.rs`.
    fn has(&self, node: NodeKey, name: &str) -> bool;

    /// See `ParameterServer::remove` in `server.rs`.
    fn remove(&mut self, node: NodeKey, name: &str) -> bool;

    /// See `ParameterServer::get_type` in `server.rs`.
    fn get_type(&self, node: NodeKey, name: &str) -> Option<ParameterType>;

    /// See `ParameterServer::iter` in `server.rs`.
    fn iter(&self, node: NodeKey) -> impl Iterator<Item = &Parameter>;

    /// See `ParameterServer::list_names` in `server.rs`.
    fn list_names(&self, node: NodeKey) -> impl Iterator<Item = &str>;

    /// See `ParameterServer::list_with_prefix` in `server.rs`.
    fn list_with_prefix<'a>(
        &'a self,
        node: NodeKey,
        prefix: &'a str,
    ) -> impl Iterator<Item = &'a str>;

    /// See `ParameterServer::get_bool` in `server.rs`.
    fn get_bool(&self, node: NodeKey, name: &str) -> Option<bool>;

    /// See `ParameterServer::get_integer` in `server.rs`.
    fn get_integer(&self, node: NodeKey, name: &str) -> Option<i64>;

    /// See `ParameterServer::get_double` in `server.rs`.
    fn get_double(&self, node: NodeKey, name: &str) -> Option<f64>;

    /// See `ParameterServer::get_string` in `server.rs`.
    fn get_string(&self, node: NodeKey, name: &str) -> Option<&str>;

    /// See `ParameterServer::set_bool` in `server.rs`.
    fn set_bool(&mut self, node: NodeKey, name: &str, value: bool) -> Result<(), ParameterError>;

    /// See `ParameterServer::set_integer` in `server.rs`.
    fn set_integer(&mut self, node: NodeKey, name: &str, value: i64) -> Result<(), ParameterError>;

    /// See `ParameterServer::set_double` in `server.rs`.
    fn set_double(&mut self, node: NodeKey, name: &str, value: f64) -> Result<(), ParameterError>;

    /// See `ParameterServer::set_string` in `server.rs`.
    fn set_string(&mut self, node: NodeKey, name: &str, value: &str) -> Result<(), ParameterError>;

    /// See `ParameterServer::declare_parameter` in `server.rs`.
    fn declare_parameter(
        &mut self,
        node: NodeKey,
        descriptor: ParameterDescriptor,
        initial_value: Option<&ParameterValue>,
    ) -> Result<(), ParameterError>;

    /// See `ParameterServer::get_parameter_value` in `server.rs`.
    fn get_parameter_value(&self, node: NodeKey, name: &str) -> Option<ParameterValue>;

    /// See `ParameterServer::set_parameter_value` in `server.rs`.
    fn set_parameter_value(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
    ) -> Result<(), ParameterError>;
}

impl<'s> ParameterServerOps<'s> for ParameterServer<'s> {
    #[inline]
    fn take_truncated_descriptions(&mut self, report: impl FnMut(NodeKey, &str)) -> usize {
        ParameterServer::take_truncated_descriptions(self, report)
    }

    #[inline]
    fn new_in(table: ParameterTable<'s>) -> Self {
        ParameterServer::new_in(table)
    }

    #[inline]
    fn set_allow_undeclared(&mut self, node: NodeKey, allow: bool) {
        ParameterServer::set_allow_undeclared(self, node, allow)
    }

    #[inline]
    fn allows_undeclared(&self, node: NodeKey) -> bool {
        ParameterServer::allows_undeclared(self, node)
    }

    #[inline]
    fn set_parameter_events(&mut self, node: NodeKey, on: bool) {
        ParameterServer::set_parameter_events(self, node, on)
    }

    #[inline]
    fn parameter_events_enabled(&self, node: NodeKey) -> bool {
        ParameterServer::parameter_events_enabled(self, node)
    }

    #[inline]
    fn pop_initial_parameter_event(&mut self) -> Option<PendingParameterEvent> {
        ParameterServer::pop_initial_parameter_event(self)
    }

    #[inline]
    fn pop_parameter_event(&mut self) -> Option<PendingParameterEvent> {
        ParameterServer::pop_parameter_event(self)
    }

    #[inline]
    fn take_parameter_events_dropped(&mut self) -> u32 {
        ParameterServer::take_parameter_events_dropped(self)
    }

    #[inline]
    fn set_delete_on_not_set(&mut self, node: NodeKey, on: bool) {
        ParameterServer::set_delete_on_not_set(self, node, on)
    }

    #[inline]
    fn len(&self, node: NodeKey) -> usize {
        ParameterServer::len(self, node)
    }

    #[inline]
    fn is_empty(&self, node: NodeKey) -> bool {
        ParameterServer::is_empty(self, node)
    }

    #[inline]
    fn total_len(&self) -> usize {
        ParameterServer::total_len(self)
    }

    #[inline]
    fn capacity(&self) -> usize {
        ParameterServer::capacity(self)
    }

    #[inline]
    fn is_full(&self) -> bool {
        ParameterServer::is_full(self)
    }

    #[inline]
    fn declare(&mut self, node: NodeKey, name: &str, value: ParameterValue) -> bool {
        ParameterServer::declare(self, node, name, value)
    }

    #[inline]
    fn declare_ref(&mut self, node: NodeKey, name: &str, value: &ParameterValue) -> bool {
        ParameterServer::declare_ref(self, node, name, value)
    }

    #[inline]
    fn declare_with_descriptor(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
        descriptor: Option<ParameterDescriptor>,
    ) -> bool {
        ParameterServer::declare_with_descriptor(self, node, name, value, descriptor)
    }

    #[inline]
    fn try_declare(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
    ) -> Result<(), DeclarationError> {
        ParameterServer::try_declare(self, node, name, value)
    }

    #[inline]
    fn try_declare_with_descriptor(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
        descriptor: Option<ParameterDescriptor>,
    ) -> Result<(), DeclarationError> {
        ParameterServer::try_declare_with_descriptor(self, node, name, value, descriptor)
    }

    #[inline]
    fn try_declare_ref(
        &mut self,
        node: NodeKey,
        name: &str,
        value: &ParameterValue,
        descriptor: Option<ParameterDescriptor>,
    ) -> Result<(), DeclarationError> {
        ParameterServer::try_declare_ref(self, node, name, value, descriptor)
    }

    #[inline]
    fn check_apply(
        &self,
        node: NodeKey,
        name: &str,
        value: &ParameterValue,
    ) -> Result<(), ParameterError> {
        ParameterServer::check_apply(self, node, name, value)
    }

    #[inline]
    fn apply(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
    ) -> Result<(), ParameterError> {
        ParameterServer::apply(self, node, name, value)
    }

    #[inline]
    fn apply_ref(
        &mut self,
        node: NodeKey,
        name: &str,
        value: &ParameterValue,
    ) -> Result<(), ParameterError> {
        ParameterServer::apply_ref(self, node, name, value)
    }

    #[inline]
    fn apply_staged(
        &mut self,
        node: NodeKey,
        name: &str,
        staged: &mut ParameterValue,
    ) -> Result<(), ParameterError> {
        ParameterServer::apply_staged(self, node, name, staged)
    }

    #[inline]
    fn set_local(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
    ) -> Result<(), ParameterError> {
        ParameterServer::set_local(self, node, name, value)
    }

    #[inline]
    fn add_on_set_parameters_callback(
        &mut self,
        node: NodeKey,
        callback: OnSetParameterFn,
        ctx: OnSetContext,
    ) -> Option<OnSetParameterHandle> {
        ParameterServer::add_on_set_parameters_callback(self, node, callback, ctx)
    }

    #[inline]
    fn remove_on_set_parameters_callback(&mut self, handle: OnSetParameterHandle) -> bool {
        ParameterServer::remove_on_set_parameters_callback(self, handle)
    }

    #[inline]
    fn on_set_callbacks_free(&self) -> usize {
        ParameterServer::on_set_callbacks_free(self)
    }

    #[inline]
    fn set_parameter_description(
        &mut self,
        node: NodeKey,
        name: &str,
        description: &str,
        additional_constraints: &str,
    ) -> bool {
        ParameterServer::set_parameter_description(
            self,
            node,
            name,
            description,
            additional_constraints,
        )
    }

    #[inline]
    fn set_parameter_read_only(&mut self, node: NodeKey, name: &str, read_only: bool) -> bool {
        ParameterServer::set_parameter_read_only(self, node, name, read_only)
    }

    #[inline]
    fn set_parameter_integer_range(
        &mut self,
        node: NodeKey,
        name: &str,
        min: i64,
        max: i64,
        step: i64,
    ) -> bool {
        ParameterServer::set_parameter_integer_range(self, node, name, min, max, step)
    }

    #[inline]
    fn set_parameter_float_range(
        &mut self,
        node: NodeKey,
        name: &str,
        min: f64,
        max: f64,
        step: f64,
    ) -> bool {
        ParameterServer::set_parameter_float_range(self, node, name, min, max, step)
    }

    #[inline]
    fn apply_atomically<'x, I>(&mut self, node: NodeKey, items: I) -> Result<(), ParameterError>
    where
        I: Iterator<Item = Option<(&'x str, ParameterValue)>> + Clone,
    {
        ParameterServer::apply_atomically::<I>(self, node, items)
    }

    #[inline]
    fn apply_atomically_with<'x>(
        &mut self,
        node: NodeKey,
        count: usize,
        fill: impl FnMut(usize, &mut ParameterValue) -> Option<&'x str>,
    ) -> Result<(), ParameterError> {
        ParameterServer::apply_atomically_with(self, node, count, fill)
    }

    #[inline]
    fn get(&self, node: NodeKey, name: &str) -> Option<&ParameterValue> {
        ParameterServer::get(self, node, name)
    }

    #[inline]
    fn get_parameter(&self, node: NodeKey, name: &str) -> Option<&Parameter> {
        ParameterServer::get_parameter(self, node, name)
    }

    #[inline]
    fn get_descriptor(&self, node: NodeKey, name: &str) -> Option<&ParameterDescriptor> {
        ParameterServer::get_descriptor(self, node, name)
    }

    #[inline]
    fn set(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
    ) -> Result<(), ParameterError> {
        ParameterServer::set(self, node, name, value)
    }

    #[inline]
    fn set_ref(
        &mut self,
        node: NodeKey,
        name: &str,
        value: &ParameterValue,
    ) -> Result<(), ParameterError> {
        ParameterServer::set_ref(self, node, name, value)
    }

    #[inline]
    fn unset(&mut self, node: NodeKey, name: &str) -> Result<(), ParameterError> {
        ParameterServer::unset(self, node, name)
    }

    #[inline]
    fn set_or_declare(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
    ) -> Result<(), ParameterError> {
        ParameterServer::set_or_declare(self, node, name, value)
    }

    #[inline]
    fn has(&self, node: NodeKey, name: &str) -> bool {
        ParameterServer::has(self, node, name)
    }

    #[inline]
    fn remove(&mut self, node: NodeKey, name: &str) -> bool {
        ParameterServer::remove(self, node, name)
    }

    #[inline]
    fn get_type(&self, node: NodeKey, name: &str) -> Option<ParameterType> {
        ParameterServer::get_type(self, node, name)
    }

    #[inline]
    fn iter(&self, node: NodeKey) -> impl Iterator<Item = &Parameter> {
        ParameterServer::iter(self, node)
    }

    #[inline]
    fn list_names(&self, node: NodeKey) -> impl Iterator<Item = &str> {
        ParameterServer::list_names(self, node)
    }

    #[inline]
    fn list_with_prefix<'a>(
        &'a self,
        node: NodeKey,
        prefix: &'a str,
    ) -> impl Iterator<Item = &'a str> {
        ParameterServer::list_with_prefix(self, node, prefix)
    }

    #[inline]
    fn get_bool(&self, node: NodeKey, name: &str) -> Option<bool> {
        ParameterServer::get_bool(self, node, name)
    }

    #[inline]
    fn get_integer(&self, node: NodeKey, name: &str) -> Option<i64> {
        ParameterServer::get_integer(self, node, name)
    }

    #[inline]
    fn get_double(&self, node: NodeKey, name: &str) -> Option<f64> {
        ParameterServer::get_double(self, node, name)
    }

    #[inline]
    fn get_string(&self, node: NodeKey, name: &str) -> Option<&str> {
        ParameterServer::get_string(self, node, name)
    }

    #[inline]
    fn set_bool(&mut self, node: NodeKey, name: &str, value: bool) -> Result<(), ParameterError> {
        ParameterServer::set_bool(self, node, name, value)
    }

    #[inline]
    fn set_integer(&mut self, node: NodeKey, name: &str, value: i64) -> Result<(), ParameterError> {
        ParameterServer::set_integer(self, node, name, value)
    }

    #[inline]
    fn set_double(&mut self, node: NodeKey, name: &str, value: f64) -> Result<(), ParameterError> {
        ParameterServer::set_double(self, node, name, value)
    }

    #[inline]
    fn set_string(&mut self, node: NodeKey, name: &str, value: &str) -> Result<(), ParameterError> {
        ParameterServer::set_string(self, node, name, value)
    }

    #[inline]
    fn declare_parameter(
        &mut self,
        node: NodeKey,
        descriptor: ParameterDescriptor,
        initial_value: Option<&ParameterValue>,
    ) -> Result<(), ParameterError> {
        ParameterServer::declare_parameter(self, node, descriptor, initial_value)
    }

    #[inline]
    fn get_parameter_value(&self, node: NodeKey, name: &str) -> Option<ParameterValue> {
        ParameterServer::get_parameter_value(self, node, name)
    }

    #[inline]
    fn set_parameter_value(
        &mut self,
        node: NodeKey,
        name: &str,
        value: ParameterValue,
    ) -> Result<(), ParameterError> {
        ParameterServer::set_parameter_value(self, node, name, value)
    }
}
