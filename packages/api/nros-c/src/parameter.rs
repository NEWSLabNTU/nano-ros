//! The C parameter server — rclc's `rclc_parameter_server_t` API over the
//! executor's parameter store (RFC-0105, phase-491).
//!
//! ONE family. It replaces three: the caller-storage `nros_parameter_server_t`
//! (a second store no `ros2 param` service read — issue 0793), the
//! `nros_executor_*_param*` family (served, but with none of rclc's names), and
//! C's use of the C++ `nros_cpp_*_param*` shim.
//!
//! The server is a HANDLE, not storage: every parameter lives in the executor's
//! store keyed by the node, which is the store the six `rcl_interfaces`
//! services read. Nothing here allocates (RFC-0105 D10): text and arrays cross
//! the boundary through caller buffers, and the callback's `Parameter` views
//! are built on this frame and point into the store for the duration of the
//! call.
//!
//! The declarations are hand-written in `<rclc_parameter/rclc_parameter.h>`,
//! not emitted by cbindgen, because the whole surface exists only with
//! `param-services`: a header that declared it unconditionally is issue 0865.

#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
pub use imp::*;

/// cbindgen:ignore
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
mod imp {
    use core::ffi::{c_char, c_void};

    use nros_node::{ParameterError, ParameterValue, executor::NodeId};

    use crate::{
        error::*,
        executor::{get_executor, rclc_executor_t},
        node::{rcl_node_t, rclc_node_state_t},
    };

    // ------------------------------------------------------------------------
    // Return codes rclc adds on top of rcl's. Upstream's values, including the
    // fifth digit of DISABLED_ON_CALLBACK (sic, `40004` in rclc).
    // ------------------------------------------------------------------------

    /// The on-set callback returned `false`.
    pub const RCLC_PARAMETER_MODIFICATION_REJECTED: rcl_ret_t = 4001;
    /// A typed set of a parameter declared with another type.
    pub const RCLC_PARAMETER_TYPE_MISMATCH: rcl_ret_t = 4002;
    /// A mutating call made from inside the on-set callback.
    pub const RCLC_PARAMETER_DISABLED_ON_CALLBACK: rcl_ret_t = 40004;

    // ------------------------------------------------------------------------
    // Types
    // ------------------------------------------------------------------------

    /// rclc's `rclc_parameter_type_t`, carried as its integer so a value C
    /// passes that names no variant cannot be undefined behaviour here.
    /// 0..3 are rclc's; 4..9 are `rcl_interfaces/msg/ParameterType`'s, which
    /// rclc does not serve and this server does.
    pub type rclc_parameter_type_t = u32;
    const T_NOT_SET: u32 = 0;
    const T_BOOL: u32 = 1;
    const T_INT: u32 = 2;
    const T_DOUBLE: u32 = 3;
    const T_STRING: u32 = 4;
    const T_BYTE_ARRAY: u32 = 5;
    const T_BOOL_ARRAY: u32 = 6;
    const T_INT_ARRAY: u32 = 7;
    const T_DOUBLE_ARRAY: u32 = 8;
    const T_STRING_ARRAY: u32 = 9;

    /// rclc's `rclc_parameter_options_t`, field for field.
    #[repr(C)]
    pub struct rclc_parameter_options_t {
        /// Publish `/parameter_events` (phase-491 W3, RFC-0105 D8): when set,
        /// `rclc_parameter_server_init_with_option` gives the node one
        /// `/parameter_events` publisher and every accepted change is
        /// published on the executor's next flush.
        pub notify_changed_over_dds: bool,
        /// A per-node QUOTA inside the build-time store, not an allocation.
        pub max_params: usize,
        /// Let a remote set declare a name this node never declared.
        pub allow_undeclared_parameters: bool,
        /// Accepted; the services always stream, so it changes nothing.
        pub low_mem_mode: bool,
    }

    /// rclc's callback: `old` NULL when the write creates the parameter, `new`
    /// NULL when it deletes it; `false` refuses.
    pub type rclc_parameter_callback_t = Option<
        unsafe extern "C" fn(
            old_param: *const rcl_interfaces__msg__Parameter,
            new_param: *const rcl_interfaces__msg__Parameter,
            context: *mut c_void,
        ) -> bool,
    >;

    /// The server HANDLE. Its members are ours, not rclc's (rclc's hold the
    /// parameter list itself), so code reaching into them fails to compile
    /// rather than reading a different layout.
    #[repr(C)]
    pub struct rclc_parameter_server_t {
        executor: *mut rclc_executor_t,
        max_params: usize,
        on_modification: rclc_parameter_callback_t,
        context: *mut c_void,
        node_id: u8,
        initialized: bool,
        on_callback: bool,
        has_hook: bool,
        hook: u16,
        // `low_mem_mode` changes nothing (the services always stream); the
        // events flag is acted on at init. Both are kept so the handle states
        // what it was given.
        #[allow(dead_code)]
        notify_changed_over_dds: bool,
        #[allow(dead_code)]
        low_mem_mode: bool,
    }

    // rosidl's C runtime shapes, so a callback reads `p->name.data`,
    // `p->value.type`, `p->value.integer_value` as a ported rclc file does.
    // They are layout-compatible with rosidl_runtime_c's and point INTO the
    // store; nothing here owns or frees them.

    /// `rosidl_runtime_c__String`.
    #[repr(C)]
    pub struct rosidl_runtime_c__String {
        pub data: *mut c_char,
        pub size: usize,
        pub capacity: usize,
    }

    macro_rules! sequence {
        ($name:ident, $t:ty) => {
            #[repr(C)]
            pub struct $name {
                pub data: *mut $t,
                pub size: usize,
                pub capacity: usize,
            }
        };
    }
    sequence!(rosidl_runtime_c__octet__Sequence, u8);
    sequence!(rosidl_runtime_c__boolean__Sequence, bool);
    sequence!(rosidl_runtime_c__int64__Sequence, i64);
    sequence!(rosidl_runtime_c__double__Sequence, f64);
    sequence!(rosidl_runtime_c__String__Sequence, rosidl_runtime_c__String);

    /// `rcl_interfaces__msg__ParameterValue`.
    #[repr(C)]
    pub struct rcl_interfaces__msg__ParameterValue {
        pub r#type: u8,
        pub bool_value: bool,
        pub integer_value: i64,
        pub double_value: f64,
        pub string_value: rosidl_runtime_c__String,
        pub byte_array_value: rosidl_runtime_c__octet__Sequence,
        pub bool_array_value: rosidl_runtime_c__boolean__Sequence,
        pub integer_array_value: rosidl_runtime_c__int64__Sequence,
        pub double_array_value: rosidl_runtime_c__double__Sequence,
        pub string_array_value: rosidl_runtime_c__String__Sequence,
    }

    /// `rcl_interfaces__msg__Parameter`.
    #[repr(C)]
    pub struct rcl_interfaces__msg__Parameter {
        pub name: rosidl_runtime_c__String,
        pub value: rcl_interfaces__msg__ParameterValue,
    }

    // ------------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------------

    /// Read a NUL-terminated C string of at most `max` bytes as `&str`.
    unsafe fn cstr<'a>(p: *const c_char, max: usize) -> Option<&'a str> {
        if p.is_null() {
            return None;
        }
        let mut len = 0usize;
        while *p.add(len) != 0 {
            len += 1;
            if len > max {
                return None;
            }
        }
        core::str::from_utf8(core::slice::from_raw_parts(p.cast::<u8>(), len)).ok()
    }

    unsafe fn name_str<'a>(p: *const c_char) -> Option<&'a str> {
        cstr(p, nros_node::MAX_PARAM_NAME_LEN)
    }

    /// The executor and node key a READY server names.
    unsafe fn target<'a>(
        server: *const rclc_parameter_server_t,
    ) -> Result<(crate::executor::CExecRef<'a>, NodeId), rcl_ret_t> {
        let Some(s) = server.as_ref() else {
            return Err(RCL_RET_INVALID_ARGUMENT);
        };
        if !s.initialized || s.executor.is_null() {
            return Err(RCL_RET_NOT_INIT);
        }
        Ok((
            get_executor(&mut (*s.executor)),
            NodeId::from_raw(s.node_id),
        ))
    }

    /// [`target`] for a call that MUTATES: refused while the callback runs, as
    /// rclc refuses it.
    unsafe fn target_mut<'a>(
        server: *mut rclc_parameter_server_t,
    ) -> Result<(crate::executor::CExecRef<'a>, NodeId), rcl_ret_t> {
        if let Some(s) = server.as_ref()
            && s.on_callback
        {
            return Err(RCLC_PARAMETER_DISABLED_ON_CALLBACK);
        }
        target(server)
    }

    macro_rules! tri {
        ($e:expr) => {
            match $e {
                Ok(v) => v,
                Err(r) => return r,
            }
        };
    }

    fn set_result(r: Result<(), ParameterError>) -> rcl_ret_t {
        match r {
            Ok(()) => RCL_RET_OK,
            Err(ParameterError::TypeMismatch) => RCLC_PARAMETER_TYPE_MISMATCH,
            Err(ParameterError::Rejected) => RCLC_PARAMETER_MODIFICATION_REJECTED,
            Err(ParameterError::NotFound) | Err(ParameterError::Undeclared) => RCL_RET_ERROR,
            _ => RCL_RET_INVALID_ARGUMENT,
        }
    }

    fn type_code(t: Option<nros_node::ParameterType>) -> rclc_parameter_type_t {
        use nros_node::ParameterType as P;
        match t {
            None | Some(P::NotSet) => T_NOT_SET,
            Some(P::Bool) => T_BOOL,
            Some(P::Integer) => T_INT,
            Some(P::Double) => T_DOUBLE,
            Some(P::String) => T_STRING,
            Some(P::ByteArray) => T_BYTE_ARRAY,
            Some(P::BoolArray) => T_BOOL_ARRAY,
            Some(P::IntegerArray) => T_INT_ARRAY,
            Some(P::DoubleArray) => T_DOUBLE_ARRAY,
            Some(P::StringArray) => T_STRING_ARRAY,
        }
    }

    /// The zero value rclc's `rclc_add_parameter` declares.
    fn zero_of(t: rclc_parameter_type_t) -> Option<ParameterValue> {
        Some(match t {
            T_BOOL => ParameterValue::Bool(false),
            T_INT => ParameterValue::Integer(0),
            T_DOUBLE => ParameterValue::Double(0.0),
            T_STRING => ParameterValue::String(Default::default()),
            T_BYTE_ARRAY => ParameterValue::ByteArray(Default::default()),
            T_BOOL_ARRAY => ParameterValue::BoolArray(Default::default()),
            T_INT_ARRAY => ParameterValue::IntegerArray(Default::default()),
            T_DOUBLE_ARRAY => ParameterValue::DoubleArray(Default::default()),
            T_STRING_ARRAY => ParameterValue::StringArray(Default::default()),
            _ => return None,
        })
    }

    /// Declare `value` on the server's node. A name already declared WITH THE
    /// SAME TYPE is accepted and keeps its value: that is how a launch value
    /// baked into the image survives the application's own declaration.
    unsafe fn declare(
        server: *mut rclc_parameter_server_t,
        name: *const c_char,
        value: ParameterValue,
    ) -> rcl_ret_t {
        let (mut exec, id) = tri!(target_mut(server));
        let Some(n) = name_str(name) else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        nros::census_hooks::on_param_declare(n, &value);
        if let Some(t) = exec.get_parameter_type_on(id, n) {
            return if t == value.param_type() {
                RCL_RET_OK
            } else {
                RCL_RET_ERROR
            };
        }
        if exec.list_parameters_on(id, "").count() >= (*server).max_params {
            return RCL_RET_ERROR;
        }
        if exec.declare_parameter_on(id, n, value) {
            exec.flush_parameter_events();
            RCL_RET_OK
        } else {
            RCL_RET_ERROR
        }
    }

    unsafe fn set_local(
        server: *mut rclc_parameter_server_t,
        name: *const c_char,
        value: ParameterValue,
    ) -> rcl_ret_t {
        let (mut exec, id) = tri!(target_mut(server));
        let Some(n) = name_str(name) else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        let r = set_result(exec.set_parameter_local_on(id, n, value));
        exec.flush_parameter_events();
        r
    }

    unsafe fn get_value<'a>(
        server: *mut rclc_parameter_server_t,
        name: *const c_char,
    ) -> Result<crate::executor::CLocked<'a, ParameterValue>, rcl_ret_t> {
        let (exec, id) = target(server)?;
        let n = name_str(name).ok_or(RCL_RET_INVALID_ARGUMENT)?;
        let v: *const ParameterValue = exec.get_parameter_on(id, n).ok_or(RCL_RET_ERROR)?;
        // SAFETY (phase-509 P9b): the value lives in the executor's parameter
        // store and is read only while the returned `CLocked` holds the lock.
        Ok(crate::executor::CLocked::new(v.cast_mut(), Some(exec)))
    }

    // ------------------------------------------------------------------------
    // The callback: the store's hook, unpacked into rclc's `(old, new, ctx)`
    // ------------------------------------------------------------------------

    /// Upper bound of this frame: two values' string buffers plus two
    /// string-array view tables plus the name. Stated so RFC-0105 D10.2's
    /// "fixed, documented stack bound" has a number to hold.
    const NAME_BUF: usize = nros_node::MAX_PARAM_NAME_LEN + 1;
    const STR_BUF: usize = nros_node::MAX_STRING_VALUE_LEN + 1;

    struct ParamView {
        msg: rcl_interfaces__msg__Parameter,
        text: [u8; STR_BUF],
        strings: [rosidl_runtime_c__String; nros_node::MAX_ARRAY_LEN],
    }

    const NULL_STR: rosidl_runtime_c__String = rosidl_runtime_c__String {
        data: core::ptr::null_mut(),
        size: 0,
        capacity: 0,
    };

    fn empty_value() -> rcl_interfaces__msg__ParameterValue {
        macro_rules! z {
            ($t:ident) => {
                $t {
                    data: core::ptr::null_mut(),
                    size: 0,
                    capacity: 0,
                }
            };
        }
        rcl_interfaces__msg__ParameterValue {
            r#type: 0,
            bool_value: false,
            integer_value: 0,
            double_value: 0.0,
            string_value: NULL_STR,
            byte_array_value: z!(rosidl_runtime_c__octet__Sequence),
            bool_array_value: z!(rosidl_runtime_c__boolean__Sequence),
            integer_array_value: z!(rosidl_runtime_c__int64__Sequence),
            double_array_value: z!(rosidl_runtime_c__double__Sequence),
            string_array_value: z!(rosidl_runtime_c__String__Sequence),
        }
    }

    /// Fill `view` from `value`. A scalar string is copied so it can carry a
    /// NUL terminator; array elements, including a string array's, point into
    /// the store and are NOT NUL-terminated — read `size`.
    fn fill(view: &mut ParamView, name: &rosidl_runtime_c__String, value: &ParameterValue) {
        let v = &mut view.msg.value;
        view.msg.name = rosidl_runtime_c__String {
            data: name.data,
            size: name.size,
            capacity: name.capacity,
        };
        v.r#type = type_code(Some(value.param_type())) as u8;
        macro_rules! seq {
            ($field:ident, $slice:expr) => {{
                let s = $slice;
                v.$field.data = s.as_ptr() as *mut _;
                v.$field.size = s.len();
                v.$field.capacity = s.len();
            }};
        }
        match value {
            ParameterValue::NotSet => {}
            ParameterValue::Bool(b) => v.bool_value = *b,
            ParameterValue::Integer(i) => v.integer_value = *i,
            ParameterValue::Double(d) => v.double_value = *d,
            ParameterValue::String(s) => {
                let n = s.len().min(STR_BUF - 1);
                view.text[..n].copy_from_slice(&s.as_bytes()[..n]);
                view.text[n] = 0;
                v.string_value = rosidl_runtime_c__String {
                    data: view.text.as_mut_ptr().cast(),
                    size: n,
                    capacity: n + 1,
                };
            }
            ParameterValue::ByteArray(a) => seq!(byte_array_value, a.as_slice()),
            ParameterValue::BoolArray(a) => seq!(bool_array_value, a.as_slice()),
            ParameterValue::IntegerArray(a) => seq!(integer_array_value, a.as_slice()),
            ParameterValue::DoubleArray(a) => seq!(double_array_value, a.as_slice()),
            ParameterValue::StringArray(a) => {
                for (slot, s) in view.strings.iter_mut().zip(a.iter()) {
                    *slot = rosidl_runtime_c__String {
                        data: s.as_ptr() as *mut c_char,
                        size: s.len(),
                        capacity: s.len(),
                    };
                }
                v.string_array_value.data = view.strings.as_mut_ptr();
                v.string_array_value.size = a.len();
                v.string_array_value.capacity = a.len();
            }
        }
    }

    fn new_view() -> ParamView {
        ParamView {
            msg: rcl_interfaces__msg__Parameter {
                name: NULL_STR,
                value: empty_value(),
            },
            text: [0; STR_BUF],
            strings: [NULL_STR; nros_node::MAX_ARRAY_LEN],
        }
    }

    /// The store's hook. `ctx.a` is the server handle, whose address is stable
    /// from `rclc_executor_add_parameter_server` to `fini` (rclc's rule too).
    fn trampoline(
        _node: nros_node::ParameterNodeKey,
        name: &str,
        old: Option<&ParameterValue>,
        new: Option<&ParameterValue>,
        ctx: nros_node::OnSetContext,
    ) -> bool {
        // SAFETY: `a` is the `rclc_parameter_server_t*` registered with this
        // hook, and `fini` unregisters it before the handle can go away.
        let server = unsafe { &mut *(ctx.a as *mut rclc_parameter_server_t) };
        let Some(cb) = server.on_modification else {
            return true;
        };
        let mut name_buf = [0u8; NAME_BUF];
        let n = name.len().min(NAME_BUF - 1);
        name_buf[..n].copy_from_slice(&name.as_bytes()[..n]);
        let name_s = rosidl_runtime_c__String {
            data: name_buf.as_mut_ptr().cast(),
            size: n,
            capacity: n + 1,
        };
        let mut old_view = new_view();
        let mut new_view_ = new_view();
        let old_ptr = match old {
            Some(v) => {
                fill(&mut old_view, &name_s, v);
                &raw const old_view.msg
            }
            None => core::ptr::null(),
        };
        let new_ptr = match new {
            Some(v) => {
                fill(&mut new_view_, &name_s, v);
                &raw const new_view_.msg
            }
            None => core::ptr::null(),
        };
        server.on_callback = true;
        // SAFETY: the caller handed us a valid callback and a context that
        // outlives the registration.
        let accepted = unsafe { cb(old_ptr, new_ptr, server.context) };
        server.on_callback = false;
        accepted
    }

    // ------------------------------------------------------------------------
    // Lifecycle
    // ------------------------------------------------------------------------

    /// rclc's defaults: events on, 4 parameters, no undeclared sets.
    const DEFAULT_OPTIONS: rclc_parameter_options_t = rclc_parameter_options_t {
        notify_changed_over_dds: true,
        max_params: 4,
        allow_undeclared_parameters: false,
        low_mem_mode: false,
    };

    /// rclc's `rclc_parameter_server_init_default`.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_parameter_server_init_default(
        server: *mut rclc_parameter_server_t,
        node: *mut rcl_node_t,
    ) -> rcl_ret_t {
        rclc_parameter_server_init_with_option(server, node, &DEFAULT_OPTIONS)
    }

    /// rclc's `rclc_parameter_server_init_with_option`.
    ///
    /// RFC-0105 D3: `RCL_RET_NOT_INIT` (rcl's `RCL_RET_NOT_INIT`) when the
    /// node is not bound to an executor — the store is the executor's, so
    /// there is nothing to serve from yet. Bind it with
    /// `rclc_executor_node_init` first. `RCLC_RET_FULL` when `max_params`
    /// exceeds the store's build-time capacity (`NROS_MAX_PARAMETERS`).
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_parameter_server_init_with_option(
        server: *mut rclc_parameter_server_t,
        node: *mut rcl_node_t,
        options: *const rclc_parameter_options_t,
    ) -> rcl_ret_t {
        let (Some(s), Some(node), Some(opts)) = (server.as_mut(), node.as_ref(), options.as_ref())
        else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        if s.initialized {
            return RCLC_RET_ALREADY_EXISTS;
        }
        if node.state != rclc_node_state_t::RCLC_NODE_STATE_INITIALIZED || !node.is_executor_bound()
        {
            return RCL_RET_NOT_INIT;
        }
        let executor = node.executor.cast_mut();
        let mut exec = get_executor(&mut (*executor));
        let id = NodeId::from_raw(node.node_id);
        // Creates the store if this is its first use, and sets the node's
        // undeclared-set policy.
        exec.allow_undeclared_parameters_on(id, opts.allow_undeclared_parameters);
        let Some(capacity) = exec.params().map(|p| p.capacity()) else {
            return RCLC_RET_FULL;
        };
        if opts.max_params > capacity {
            return RCLC_RET_FULL;
        }
        // rclc's set service deletes on NOT_SET; rclcpp's refuses.
        exec.delete_parameters_on_not_set_on(id, true);
        // rclc's `notify_changed_over_dds` (default on): one
        // `/parameter_events` publisher for the node (RFC-0105 D8).
        // Issue 2023 — OFF is now an explicit opt-out: the services this
        // server registers (`register_parameter_services`) bring the events up
        // for every node that did not opt out, which is rclcpp's default.
        if opts.notify_changed_over_dds {
            if exec.enable_parameter_events_on(id).is_err() {
                return RCL_RET_ERROR;
            }
        } else {
            exec.disable_parameter_events_on(id);
        }
        *s = rclc_parameter_server_t {
            executor,
            max_params: opts.max_params,
            on_modification: None,
            context: core::ptr::null_mut(),
            node_id: node.node_id,
            initialized: true,
            on_callback: false,
            has_hook: false,
            hook: 0,
            notify_changed_over_dds: opts.notify_changed_over_dds,
            low_mem_mode: opts.low_mem_mode,
        };
        RCL_RET_OK
    }

    /// rclc's `rclc_parameter_server_fini`: unregisters the callback and
    /// deletes every parameter of the node, as rclc's server takes its list
    /// with it.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_parameter_server_fini(
        server: *mut rclc_parameter_server_t,
        _node: *mut rcl_node_t,
    ) -> rcl_ret_t {
        let (mut exec, id) = tri!(target_mut(server));
        let s = &mut *server;
        if s.has_hook {
            exec.remove_on_set_parameters_callback(nros_node::OnSetParameterHandle::from_raw(
                s.hook,
            ));
        }
        let mut name = [0u8; NAME_BUF];
        loop {
            let Some(first) = exec.list_parameters_on(id, "").next() else {
                break;
            };
            let n = first.len().min(NAME_BUF - 1);
            name[..n].copy_from_slice(&first.as_bytes()[..n]);
            let Ok(owned) = core::str::from_utf8(&name[..n]) else {
                break;
            };
            if !exec.undeclare_parameter_on(id, owned) {
                break;
            }
        }
        exec.flush_parameter_events();
        exec.delete_parameters_on_not_set_on(id, false);
        s.initialized = false;
        s.has_hook = false;
        s.on_modification = None;
        RCL_RET_OK
    }

    /// rclc's `rclc_executor_add_parameter_server`.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_executor_add_parameter_server(
        executor: *mut rclc_executor_t,
        server: *mut rclc_parameter_server_t,
        on_modification: rclc_parameter_callback_t,
    ) -> rcl_ret_t {
        rclc_executor_add_parameter_server_with_context(
            executor,
            server,
            on_modification,
            core::ptr::null_mut(),
        )
    }

    /// rclc's `rclc_executor_add_parameter_server_with_context`: registers the
    /// six `rcl_interfaces` services and installs the callback.
    /// `RCL_RET_INVALID_ARGUMENT` when `executor` is not the one the server's
    /// node is bound to.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_executor_add_parameter_server_with_context(
        executor: *mut rclc_executor_t,
        server: *mut rclc_parameter_server_t,
        on_modification: rclc_parameter_callback_t,
        context: *mut c_void,
    ) -> rcl_ret_t {
        let (mut exec, id) = tri!(target_mut(server));
        let s = &mut *server;
        if executor.is_null() || executor != s.executor {
            return RCL_RET_INVALID_ARGUMENT;
        }
        if exec.register_parameter_services().is_err() {
            return RCL_RET_ERROR;
        }
        s.on_modification = on_modification;
        s.context = context;
        if on_modification.is_some() && !s.has_hook {
            let ctx = nros_node::OnSetContext::new(server as usize);
            match exec.add_on_set_parameters_callback_on(id, trampoline, ctx) {
                Some(h) => {
                    s.hook = h.raw();
                    s.has_hook = true;
                }
                None => return RCLC_RET_FULL,
            }
        }
        RCL_RET_OK
    }

    // ------------------------------------------------------------------------
    // rclc's declare / delete / typed get and set
    // ------------------------------------------------------------------------

    /// rclc's `rclc_add_parameter`: declares `type`'s zero value. A launch
    /// value already baked under this name with this type is kept.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_add_parameter(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
        r#type: rclc_parameter_type_t,
    ) -> rcl_ret_t {
        let Some(v) = zero_of(r#type) else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        declare(server, parameter_name, v)
    }

    /// rclc's `rclc_delete_parameter`. Fires no callback, as rclc's local
    /// delete does not.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_delete_parameter(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
    ) -> rcl_ret_t {
        let (mut exec, id) = tri!(target_mut(server));
        let Some(n) = name_str(parameter_name) else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        if exec.undeclare_parameter_on(id, n) {
            exec.flush_parameter_events();
            RCL_RET_OK
        } else {
            RCL_RET_ERROR
        }
    }

    macro_rules! scalar {
        ($set:ident, $get:ident, $declare:ident, $t:ty, $from:path, $as:ident) => {
            /// rclc's typed local set: type, then the callback with
            /// `(old, new)`, then the write. Read-only and range are not
            /// checked, as rclc does not check them locally (RFC-0105 D7).
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $set(
                server: *mut rclc_parameter_server_t,
                parameter_name: *const c_char,
                value: $t,
            ) -> rcl_ret_t {
                set_local(server, parameter_name, $from(value))
            }

            /// rclc's typed get: `RCL_RET_ERROR` when undeclared,
            /// `RCL_RET_INVALID_ARGUMENT` when declared with another type.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $get(
                server: *mut rclc_parameter_server_t,
                parameter_name: *const c_char,
                output: *mut $t,
            ) -> rcl_ret_t {
                if output.is_null() {
                    return RCL_RET_INVALID_ARGUMENT;
                }
                let value = tri!(get_value(server, parameter_name));
                match value.$as() {
                    Some(v) => {
                        *output = v;
                        RCL_RET_OK
                    }
                    None => RCL_RET_INVALID_ARGUMENT,
                }
            }

            /// Extension: declare with a default, keeping a launch value of
            /// the same type (rclcpp's `declare_parameter`).
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $declare(
                server: *mut rclc_parameter_server_t,
                parameter_name: *const c_char,
                default_value: $t,
            ) -> rcl_ret_t {
                declare(server, parameter_name, $from(default_value))
            }
        };
    }
    scalar!(
        rclc_parameter_set_bool,
        rclc_parameter_get_bool,
        rclc_parameter_declare_bool,
        bool,
        ParameterValue::from_bool,
        as_bool
    );
    scalar!(
        rclc_parameter_set_int,
        rclc_parameter_get_int,
        rclc_parameter_declare_int,
        i64,
        ParameterValue::from_integer,
        as_integer
    );
    scalar!(
        rclc_parameter_set_double,
        rclc_parameter_get_double,
        rclc_parameter_declare_double,
        f64,
        ParameterValue::from_double,
        as_double
    );

    // ------------------------------------------------------------------------
    // Extensions: strings and arrays (rclc serves bool/int/double only)
    // ------------------------------------------------------------------------

    unsafe fn string_value(p: *const c_char) -> Option<ParameterValue> {
        ParameterValue::from_string(cstr(p, nros_node::MAX_STRING_VALUE_LEN)?)
    }

    /// Extension: declare a string parameter. Over-length text is refused.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_parameter_declare_string(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
        default_value: *const c_char,
    ) -> rcl_ret_t {
        match string_value(default_value) {
            Some(v) => declare(server, parameter_name, v),
            None => RCL_RET_INVALID_ARGUMENT,
        }
    }

    /// Extension: rclc-style local set of a string parameter.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_parameter_set_string(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
        value: *const c_char,
    ) -> rcl_ret_t {
        match string_value(value) {
            Some(v) => set_local(server, parameter_name, v),
            None => RCL_RET_INVALID_ARGUMENT,
        }
    }

    /// Extension: copy a string parameter into `output` (`capacity` bytes,
    /// NUL included). `RCLC_RET_FULL` when it does not fit; nothing is
    /// written then.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_parameter_get_string(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
        output: *mut c_char,
        capacity: usize,
    ) -> rcl_ret_t {
        if output.is_null() {
            return RCL_RET_INVALID_ARGUMENT;
        }
        let value = tri!(get_value(server, parameter_name));
        let Some(s) = value.as_string() else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        if s.len() + 1 > capacity {
            return RCLC_RET_FULL;
        }
        core::ptr::copy_nonoverlapping(s.as_ptr(), output.cast::<u8>(), s.len());
        *output.add(s.len()) = 0;
        RCL_RET_OK
    }

    macro_rules! array {
        ($declare:ident, $set:ident, $get:ident, $t:ty, $from:path, $as:ident) => {
            /// Extension: declare an array parameter (bounded by
            /// `NROS_MAX_ARRAY_LEN`; longer is refused).
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $declare(
                server: *mut rclc_parameter_server_t,
                parameter_name: *const c_char,
                data: *const $t,
                len: usize,
            ) -> rcl_ret_t {
                match array_slice(data, len).and_then($from) {
                    Some(v) => declare(server, parameter_name, v),
                    None => RCL_RET_INVALID_ARGUMENT,
                }
            }

            /// Extension: rclc-style local set of an array parameter.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $set(
                server: *mut rclc_parameter_server_t,
                parameter_name: *const c_char,
                data: *const $t,
                len: usize,
            ) -> rcl_ret_t {
                match array_slice(data, len).and_then($from) {
                    Some(v) => set_local(server, parameter_name, v),
                    None => RCL_RET_INVALID_ARGUMENT,
                }
            }

            /// Extension: copy an array parameter into `output` (`capacity`
            /// elements). `*out_len` always receives the stored length;
            /// `RCLC_RET_FULL` when it does not fit, and nothing is copied.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $get(
                server: *mut rclc_parameter_server_t,
                parameter_name: *const c_char,
                output: *mut $t,
                capacity: usize,
                out_len: *mut usize,
            ) -> rcl_ret_t {
                let value = tri!(get_value(server, parameter_name));
                let Some(a) = value.$as() else {
                    return RCL_RET_INVALID_ARGUMENT;
                };
                if !out_len.is_null() {
                    *out_len = a.len();
                }
                if a.len() > capacity {
                    return RCLC_RET_FULL;
                }
                if !a.is_empty() {
                    if output.is_null() {
                        return RCL_RET_INVALID_ARGUMENT;
                    }
                    core::ptr::copy_nonoverlapping(a.as_ptr(), output, a.len());
                }
                RCL_RET_OK
            }
        };
    }

    unsafe fn array_slice<'a, T>(data: *const T, len: usize) -> Option<&'a [T]> {
        if len == 0 {
            return Some(&[]);
        }
        if data.is_null() {
            return None;
        }
        Some(core::slice::from_raw_parts(data, len))
    }

    fn from_bytes(v: &[u8]) -> Option<ParameterValue> {
        let mut out = ParameterValue::ByteArray(Default::default());
        if let ParameterValue::ByteArray(a) = &mut out {
            a.extend_from_slice(v).ok()?;
        }
        Some(out)
    }

    array!(
        rclc_parameter_declare_bool_array,
        rclc_parameter_set_bool_array,
        rclc_parameter_get_bool_array,
        bool,
        ParameterValue::from_bool_array,
        as_bool_array
    );
    array!(
        rclc_parameter_declare_int_array,
        rclc_parameter_set_int_array,
        rclc_parameter_get_int_array,
        i64,
        ParameterValue::from_integer_array,
        as_integer_array
    );
    array!(
        rclc_parameter_declare_double_array,
        rclc_parameter_set_double_array,
        rclc_parameter_get_double_array,
        f64,
        ParameterValue::from_double_array,
        as_double_array
    );
    array!(
        rclc_parameter_declare_byte_array,
        rclc_parameter_set_byte_array,
        rclc_parameter_get_byte_array,
        u8,
        from_bytes,
        as_byte_array
    );

    /// A string array from `len` C strings. Over-long elements or too many
    /// elements are refused, never truncated.
    unsafe fn string_array_value(data: *const *const c_char, len: usize) -> Option<ParameterValue> {
        let mut out = ParameterValue::StringArray(Default::default());
        if let ParameterValue::StringArray(a) = &mut out {
            for p in array_slice(data, len)? {
                // `from_string` is the one bounded-string constructor; take its
                // payload rather than naming `heapless` from this crate.
                let ParameterValue::String(s) =
                    ParameterValue::from_string(cstr(*p, nros_node::MAX_STRING_VALUE_LEN)?)?
                else {
                    return None;
                };
                a.push(s).ok()?;
            }
        }
        Some(out)
    }

    /// Extension: declare a string-array parameter.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_parameter_declare_string_array(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
        data: *const *const c_char,
        len: usize,
    ) -> rcl_ret_t {
        match string_array_value(data, len) {
            Some(v) => declare(server, parameter_name, v),
            None => RCL_RET_INVALID_ARGUMENT,
        }
    }

    /// Extension: rclc-style local set of a string-array parameter.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_parameter_set_string_array(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
        data: *const *const c_char,
        len: usize,
    ) -> rcl_ret_t {
        match string_array_value(data, len) {
            Some(v) => set_local(server, parameter_name, v),
            None => RCL_RET_INVALID_ARGUMENT,
        }
    }

    /// Extension: copy a string-array parameter into a caller rectangle of
    /// `capacity` rows × `stride` bytes, each NUL-terminated. `*out_len`
    /// always receives the stored length; `RCLC_RET_FULL` when there are more
    /// elements than rows or an element does not fit its row, and nothing is
    /// written then.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_parameter_get_string_array(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
        output: *mut c_char,
        stride: usize,
        capacity: usize,
        out_len: *mut usize,
    ) -> rcl_ret_t {
        let value = tri!(get_value(server, parameter_name));
        let ParameterValue::StringArray(a) = &*value else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        if !out_len.is_null() {
            *out_len = a.len();
        }
        if a.len() > capacity || a.iter().any(|s| s.len() + 1 > stride) {
            return RCLC_RET_FULL;
        }
        if !a.is_empty() && output.is_null() {
            return RCL_RET_INVALID_ARGUMENT;
        }
        for (i, s) in a.iter().enumerate() {
            let row = output.add(i * stride);
            core::ptr::copy_nonoverlapping(s.as_ptr(), row.cast::<u8>(), s.len());
            *row.add(s.len()) = 0;
        }
        RCL_RET_OK
    }

    // ------------------------------------------------------------------------
    // rclc's descriptor verbs
    // ------------------------------------------------------------------------

    /// rclc's `rclc_add_parameter_description`. NULL text clears it.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_add_parameter_description(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
        parameter_description: *const c_char,
        additional_constraints: *const c_char,
    ) -> rcl_ret_t {
        let (mut exec, id) = tri!(target_mut(server));
        let text = |p: *const c_char| {
            if p.is_null() {
                Some("")
            } else {
                cstr(p, usize::MAX >> 1)
            }
        };
        let (Some(n), Some(d), Some(c)) = (
            name_str(parameter_name),
            text(parameter_description),
            text(additional_constraints),
        ) else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        if exec.set_parameter_description_on(id, n, d, c) {
            RCL_RET_OK
        } else {
            RCL_RET_ERROR
        }
    }

    /// rclc's `rclc_set_parameter_read_only`. Refuses REMOTE writes only; the
    /// owner's local sets still go through (RFC-0105 D7).
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_set_parameter_read_only(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
        read_only: bool,
    ) -> rcl_ret_t {
        let (mut exec, id) = tri!(target_mut(server));
        let Some(n) = name_str(parameter_name) else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        if exec.set_parameter_read_only_on(id, n, read_only) {
            RCL_RET_OK
        } else {
            RCL_RET_ERROR
        }
    }

    /// rclc's `rclc_add_parameter_constraint_integer`. Enforced on REMOTE
    /// writes (rclc never enforces it; rclcpp does).
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_add_parameter_constraint_integer(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
        from_value: i64,
        to_value: i64,
        step: u64,
    ) -> rcl_ret_t {
        let (mut exec, id) = tri!(target_mut(server));
        let Some(n) = name_str(parameter_name) else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        let Ok(step) = i64::try_from(step) else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        if exec.set_parameter_integer_range_on(id, n, from_value, to_value, step) {
            RCL_RET_OK
        } else {
            RCL_RET_ERROR
        }
    }

    /// rclc's `rclc_add_parameter_constraint_double`.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_add_parameter_constraint_double(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
        from_value: f64,
        to_value: f64,
        step: f64,
    ) -> rcl_ret_t {
        let (mut exec, id) = tri!(target_mut(server));
        let Some(n) = name_str(parameter_name) else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        if exec.set_parameter_float_range_on(id, n, from_value, to_value, step) {
            RCL_RET_OK
        } else {
            RCL_RET_ERROR
        }
    }

    // ------------------------------------------------------------------------
    // Extensions: read side
    // ------------------------------------------------------------------------

    /// Extension: the declared type, `RCLC_PARAMETER_NOT_SET` when undeclared.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_parameter_get_type(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
    ) -> rclc_parameter_type_t {
        let Ok((exec, id)) = target(server) else {
            return T_NOT_SET;
        };
        match name_str(parameter_name) {
            Some(n) => type_code(exec.get_parameter_type_on(id, n)),
            None => T_NOT_SET,
        }
    }

    /// Copy text into a caller buffer, truncating at a character boundary.
    /// `RCLC_RET_FULL` when it was truncated.
    unsafe fn put_text(src: &str, dst: *mut c_char, cap: usize) -> rcl_ret_t {
        if dst.is_null() {
            return RCL_RET_OK;
        }
        if cap == 0 {
            return RCLC_RET_FULL;
        }
        let mut cut = src.len().min(cap - 1);
        while !src.is_char_boundary(cut) {
            cut -= 1;
        }
        core::ptr::copy_nonoverlapping(src.as_ptr(), dst.cast::<u8>(), cut);
        *dst.add(cut) = 0;
        if cut < src.len() {
            RCLC_RET_FULL
        } else {
            RCL_RET_OK
        }
    }

    /// Extension: read a parameter's descriptor into caller storage. Any out
    /// pointer may be NULL. `RCLC_RET_FULL` when a text was truncated (it is
    /// still written).
    #[allow(clippy::too_many_arguments)]
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_parameter_describe(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
        out_description: *mut c_char,
        description_capacity: usize,
        out_constraints: *mut c_char,
        constraints_capacity: usize,
        out_read_only: *mut bool,
    ) -> rcl_ret_t {
        let (exec, id) = tri!(target(server));
        let Some(n) = name_str(parameter_name) else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        if exec.get_parameter_type_on(id, n).is_none() {
            return RCL_RET_ERROR;
        }
        let desc = exec.describe_parameter_on(id, n);
        if !out_read_only.is_null() {
            *out_read_only = desc.is_some_and(|d| d.read_only);
        }
        let a = put_text(
            desc.map_or("", |d| d.description.as_str()),
            out_description,
            description_capacity,
        );
        let b = put_text(
            desc.map_or("", |d| d.additional_constraints.as_str()),
            out_constraints,
            constraints_capacity,
        );
        if a == RCLC_RET_FULL || b == RCLC_RET_FULL {
            RCLC_RET_FULL
        } else {
            RCL_RET_OK
        }
    }

    /// Extension: the integer range attached to a parameter.
    /// `RCL_RET_ERROR` when it has none.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_parameter_get_integer_range(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
        out_from: *mut i64,
        out_to: *mut i64,
        out_step: *mut i64,
    ) -> rcl_ret_t {
        let (exec, id) = tri!(target(server));
        let Some(n) = name_str(parameter_name) else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        let Some(nros_node::ParameterRange::Integer(r)) =
            exec.describe_parameter_on(id, n).map(|d| d.range)
        else {
            return RCL_RET_ERROR;
        };
        for (p, v) in [(out_from, r.min), (out_to, r.max), (out_step, r.step)] {
            if !p.is_null() {
                *p = v;
            }
        }
        RCL_RET_OK
    }

    /// Extension: the floating-point range attached to a parameter.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_parameter_get_double_range(
        server: *mut rclc_parameter_server_t,
        parameter_name: *const c_char,
        out_from: *mut f64,
        out_to: *mut f64,
        out_step: *mut f64,
    ) -> rcl_ret_t {
        let (exec, id) = tri!(target(server));
        let Some(n) = name_str(parameter_name) else {
            return RCL_RET_INVALID_ARGUMENT;
        };
        let Some(nros_node::ParameterRange::FloatingPoint(r)) =
            exec.describe_parameter_on(id, n).map(|d| d.range)
        else {
            return RCL_RET_ERROR;
        };
        for (p, v) in [(out_from, r.min), (out_to, r.max), (out_step, r.step)] {
            if !p.is_null() {
                *p = v;
            }
        }
        RCL_RET_OK
    }

    /// Extension: the node's parameter names starting with `prefix` (NULL =
    /// all), into a caller rectangle of `max_names` rows × `name_stride`
    /// bytes. `*out_count` always gets the TOTAL; `RCLC_RET_FULL` when more
    /// matched than fit or a name was truncated.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn rclc_parameter_list(
        server: *mut rclc_parameter_server_t,
        prefix: *const c_char,
        out_names: *mut c_char,
        name_stride: usize,
        max_names: usize,
        out_count: *mut usize,
    ) -> rcl_ret_t {
        let (exec, id) = tri!(target(server));
        let p = if prefix.is_null() {
            ""
        } else {
            match name_str(prefix) {
                Some(p) => p,
                None => return RCL_RET_INVALID_ARGUMENT,
            }
        };
        if max_names > 0 && (out_names.is_null() || name_stride == 0) {
            return RCL_RET_INVALID_ARGUMENT;
        }
        let mut total = 0usize;
        let mut ret = RCL_RET_OK;
        for name in exec.list_parameters_on(id, p) {
            if total < max_names
                && put_text(name, out_names.add(total * name_stride), name_stride) == RCLC_RET_FULL
            {
                ret = RCLC_RET_FULL;
            }
            total += 1;
        }
        if total > max_names {
            ret = RCLC_RET_FULL;
        }
        if !out_count.is_null() {
            *out_count = total;
        }
        ret
    }
}
