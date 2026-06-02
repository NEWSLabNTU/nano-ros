//! Proc macros for nros message type generation
//!
//! Provides `#[derive(RosMessage)]` and `#[derive(RosService)]` macros.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, Fields, LitStr, Path, parse_macro_input};

/// Sanitise a cargo package name into a C-identifier-safe symbol component.
///
/// Cargo allows `-` in package names; C identifiers don't. Each non
/// `[A-Za-z0-9_]` byte is replaced with `_` so the result is a valid
/// suffix for the per-pkg register symbol emitted by [`component!`].
///
/// Crate-private (proc-macro crates can't export non-macro items); the
/// `sanitize_tests` module exercises it directly.
fn sanitize_pkg_name_for_symbol(pkg: &str) -> String {
    let mut out = String::with_capacity(pkg.len());
    for c in pkg.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}

#[cfg(test)]
mod sanitize_tests {
    use super::sanitize_pkg_name_for_symbol;

    #[test]
    fn plain_pkg_name_is_passthrough() {
        assert_eq!(sanitize_pkg_name_for_symbol("talker_pkg"), "talker_pkg");
    }

    #[test]
    fn hyphens_become_underscores() {
        assert_eq!(sanitize_pkg_name_for_symbol("my-cool-pkg"), "my_cool_pkg");
    }

    #[test]
    fn mixed_specials_become_underscores() {
        assert_eq!(sanitize_pkg_name_for_symbol("a.b+c-d"), "a_b_c_d");
    }

    #[test]
    fn empty_is_empty() {
        assert_eq!(sanitize_pkg_name_for_symbol(""), "");
    }
}

/// Derive macro for ROS message types
///
/// Generates `Serialize`, `Deserialize`, and `RosMessage` implementations.
///
/// # Attributes
///
/// - `#[ros(type_name = "...")]` - Full ROS type name (required)
/// - `#[ros(hash = "...")]` - RIHS type hash (required)
///
/// # Example
///
/// ```ignore
/// use nros_macros::RosMessage;
///
/// #[derive(RosMessage)]
/// #[ros(type_name = "std_msgs::msg::dds_::String_")]
/// #[ros(hash = "abc123...")]
/// pub struct StringMsg {
///     pub data: heapless::String<256>,
/// }
/// ```
#[proc_macro_derive(RosMessage, attributes(ros))]
pub fn derive_ros_message(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Extract attributes
    let mut type_name: Option<String> = None;
    let mut type_hash: Option<String> = None;

    for attr in &input.attrs {
        if attr.path().is_ident("ros") {
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("type_name") {
                    let value: LitStr = meta.value()?.parse()?;
                    type_name = Some(value.value());
                } else if meta.path.is_ident("hash") {
                    let value: LitStr = meta.value()?.parse()?;
                    type_hash = Some(value.value());
                }
                Ok(())
            });
        }
    }

    let type_name = type_name.unwrap_or_else(|| format!("{}::msg::dds_::{}_", "unknown", name));
    let type_hash = type_hash.unwrap_or_else(|| "0".repeat(64));

    // Get fields
    let fields = match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            Fields::Unit => {
                // Unit struct (no fields)
                return generate_unit_struct_impl(
                    name,
                    &impl_generics,
                    &ty_generics,
                    where_clause,
                    &type_name,
                    &type_hash,
                );
            }
            _ => {
                return syn::Error::new_spanned(&input, "RosMessage only supports named fields")
                    .to_compile_error()
                    .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "RosMessage can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    // Generate serialize calls for each field
    let serialize_fields = fields.iter().map(|f| {
        let field_name = &f.ident;
        quote! {
            self.#field_name.serialize(writer)?;
        }
    });

    // Generate deserialize calls for each field
    let deserialize_fields = fields.iter().map(|f| {
        let field_name = &f.ident;
        quote! {
            #field_name: Deserialize::deserialize(reader)?,
        }
    });

    let expanded = quote! {
        impl #impl_generics nros_serdes::Serialize for #name #ty_generics #where_clause {
            fn serialize(&self, writer: &mut nros_serdes::CdrWriter) -> Result<(), nros_serdes::SerError> {
                use nros_serdes::Serialize;
                #(#serialize_fields)*
                Ok(())
            }
        }

        impl #impl_generics nros_serdes::Deserialize for #name #ty_generics #where_clause {
            fn deserialize(reader: &mut nros_serdes::CdrReader) -> Result<Self, nros_serdes::DeserError> {
                use nros_serdes::Deserialize;
                Ok(Self {
                    #(#deserialize_fields)*
                })
            }
        }

        impl #impl_generics nros_core::RosMessage for #name #ty_generics #where_clause {
            const TYPE_NAME: &'static str = #type_name;
            const TYPE_HASH: &'static str = #type_hash;
        }
    };

    TokenStream::from(expanded)
}

/// Export a Rust type implementing `nros::Component` as the package component.
///
/// # Example
///
/// ```ignore
/// struct Talker;
///
/// impl nros::Component for Talker {
///     const NAME: &'static str = "talker";
///
///     fn register(ctx: &mut nros::ComponentContext<'_>) -> nros::ComponentResult<()> {
///         let mut node = ctx.create_node(
///             nros::NodeId::new("node"),
///             nros::NodeOptions::new("talker"),
///         )?;
///         let _pub = node.create_publisher::<std_msgs::msg::String>(
///             nros::EntityId::new("pub_chatter"),
///             "chatter",
///         )?;
///         Ok(())
///     }
/// }
///
/// nros::component!(Talker);
/// ```
#[proc_macro]
pub fn component(input: TokenStream) -> TokenStream {
    let component_ty = parse_macro_input!(input as Path);

    // Phase 212.M.5.a.1 — mangle the exported register symbol by package
    // so multiple Component pkg crates can link into one binary without
    // duplicate-symbol errors.
    //
    // `proc_macro::tracked_env::var` is still unstable, so we use plain
    // `std::env::var`. Cargo sets `CARGO_PKG_NAME` for every compilation
    // (proc-macro crates inherit the parent crate's env at expansion).
    // The fallback "unknown" only triggers in toolchains that don't set
    // it (none today); it keeps the macro robust against future hosts.
    let pkg_raw = std::env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "unknown".to_string());
    let pkg_sym = sanitize_pkg_name_for_symbol(&pkg_raw);
    let register_ident = format_ident!("__nros_component_{}_register", pkg_sym);
    let init_ident = format_ident!("__nros_component_{}_init", pkg_sym);
    let dispatch_ident = format_ident!("__nros_component_{}_dispatch", pkg_sym);
    let tick_ident = format_ident!("__nros_component_{}_tick", pkg_sym);
    let present_ident = format_ident!("__NROS_COMPONENT_{}_EXPORT_PRESENT", pkg_sym.to_uppercase());

    // Phase 212.N.7 step-3.4 — the package-name string handed to
    // `register_dispatch_slot_dyn` (diagnostics) + `RuntimeError::ComponentRegister`
    // is the sanitised pkg-name used for symbol mangling. The codegen-emitted
    // `run_plan` body references each Component pkg by its Cargo `name`
    // (snake-cased the same way), so the two strings round-trip 1:1.
    let pkg_name_lit = pkg_sym.clone();

    // Phase 212.M.5.a.4 — parallel `init` / `dispatch` / `tick` symbols so
    // the BSP path can fire `ExecutableComponent::on_callback` /
    // `ExecutableComponent::tick` bodies without knowing the concrete
    // component type. State is `Box`-erased to `*mut ()` at the FFI
    // boundary; `init` returns a leaked Box pointer that the BSP keeps
    // alive for the lifetime of the firmware (no `drop` symbol because
    // embedded slots never deallocate). The dispatch / tick fns cast
    // back to the typed `State` inside the macro emit, where the type
    // information is still in scope.
    let expanded = quote! {
        #[unsafe(no_mangle)]
        pub extern "Rust" fn #register_ident(
            context: &mut nros::ComponentContext<'_>
        ) -> nros::ComponentResult<()> {
            <#component_ty as nros::Component>::register(context)
        }

        #[unsafe(no_mangle)]
        pub extern "Rust" fn #init_ident() -> *mut () {
            let state: <#component_ty as nros::ExecutableComponent>::State =
                <#component_ty as nros::ExecutableComponent>::init();
            ::nros::__private_component_state_into_raw::<#component_ty>(state)
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn #dispatch_ident(
            state: *mut (),
            callback: ::nros::CallbackId<'_>,
            ctx: &mut ::nros::CallbackCtx<'_>,
        ) {
            // SAFETY: `state` came from `#init_ident` and is the only
            // pointer to this `State`; the runtime never dispatches
            // concurrently against the same slot.
            let s = unsafe {
                &mut *(state as *mut <#component_ty as nros::ExecutableComponent>::State)
            };
            <#component_ty as nros::ExecutableComponent>::on_callback(s, callback, ctx);
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn #tick_ident(
            state: *mut (),
            ctx: &mut ::nros::TickCtx<'_>,
        ) {
            // SAFETY: same provenance as `#dispatch_ident`.
            let s = unsafe {
                &mut *(state as *mut <#component_ty as nros::ExecutableComponent>::State)
            };
            <#component_ty as nros::ExecutableComponent>::tick(s, ctx);
        }

        #[used]
        #[unsafe(no_mangle)]
        pub static #present_ident: u8 = 1;

        // Phase 212.N.7 step-3.4 — Entry-pkg-callable `register(runtime)`
        // wrapper. The codegen-emitted `run_plan(runtime)` body
        // (`nros-build::generate_run_plan`) dispatches one
        // `<pkg>::register(runtime)?` call per launch-XML `<node>` entry,
        // so every Component pkg whose `lib.rs` invokes `nros::component!()`
        // gets a stable per-pkg API here.
        //
        // The four typed fn-pointers (`__nros_component_<pkg>_*`) are
        // transmuted into the opaque `extern "Rust" fn()` aliases the
        // platform-side trait surface holds; the impl side in
        // `nros::component_runtime` transmutes them back to the typed
        // signatures (`ComponentRegisterFn` etc., defined in `nros`).
        //
        // SAFETY: typed `extern "Rust" fn(args...) -> ret` and the
        // zero-arg `extern "Rust" fn()` alias share the same ABI
        // representation (one pointer); the transmute is purely a
        // type-level reinterpretation. The impl-side transmute on the
        // other side recovers the same typed signature before invoking
        // — the round-trip is type-preserving so long as both sides agree
        // on the typed signature, which they do (both live in `nros`).
        pub fn register(
            runtime: &mut ::nros_platform::RuntimeCtx<'_>,
        ) -> ::core::result::Result<(), ::nros_platform::RuntimeError> {
            let register_opaque: ::nros_platform::ComponentRegisterFn = unsafe {
                ::core::mem::transmute(
                    #register_ident as fn(&mut ::nros::ComponentContext<'_>) -> ::nros::ComponentResult<()>,
                )
            };
            let init_opaque: ::nros_platform::ComponentInitFn =
                unsafe { ::core::mem::transmute(#init_ident as fn() -> *mut ()) };
            let dispatch_opaque: ::nros_platform::ComponentDispatchFn = unsafe {
                ::core::mem::transmute(
                    #dispatch_ident
                        as unsafe fn(*mut (), ::nros::CallbackId<'_>, &mut ::nros::CallbackCtx<'_>),
                )
            };
            let tick_opaque: ::nros_platform::ComponentTickFn = unsafe {
                ::core::mem::transmute(
                    #tick_ident as unsafe fn(*mut (), &mut ::nros::TickCtx<'_>),
                )
            };
            runtime
                .runtime
                .register_dispatch_slot_dyn(
                    register_opaque,
                    init_opaque,
                    dispatch_opaque,
                    tick_opaque,
                    #pkg_name_lit,
                )
                .map_err(|_| ::nros_platform::RuntimeError::ComponentRegister(#pkg_name_lit))
        }
    };

    TokenStream::from(expanded)
}

fn generate_unit_struct_impl(
    name: &syn::Ident,
    impl_generics: &syn::ImplGenerics,
    ty_generics: &syn::TypeGenerics,
    where_clause: Option<&syn::WhereClause>,
    type_name: &str,
    type_hash: &str,
) -> TokenStream {
    let expanded = quote! {
        impl #impl_generics nros_serdes::Serialize for #name #ty_generics #where_clause {
            fn serialize(&self, _writer: &mut nros_serdes::CdrWriter) -> Result<(), nros_serdes::SerError> {
                Ok(())
            }
        }

        impl #impl_generics nros_serdes::Deserialize for #name #ty_generics #where_clause {
            fn deserialize(_reader: &mut nros_serdes::CdrReader) -> Result<Self, nros_serdes::DeserError> {
                Ok(Self {})
            }
        }

        impl #impl_generics nros_core::RosMessage for #name #ty_generics #where_clause {
            const TYPE_NAME: &'static str = #type_name;
            const TYPE_HASH: &'static str = #type_hash;
        }
    };
    TokenStream::from(expanded)
}

/// Derive macro for ROS service types
///
/// # Attributes
///
/// - `#[ros(service_name = "...")]` - Full ROS service name (required)
/// - `#[ros(hash = "...")]` - RIHS service hash (required)
/// - `#[ros(request = "RequestType")]` - Request type name (required)
/// - `#[ros(reply = "ReplyType")]` - Reply type name (required)
///
/// # Example
///
/// ```ignore
/// use nros_macros::RosService;
///
/// #[derive(RosService)]
/// #[ros(service_name = "std_srvs::srv::dds_::Empty_")]
/// #[ros(hash = "abc123...")]
/// #[ros(request = "EmptyRequest")]
/// #[ros(reply = "EmptyReply")]
/// pub struct Empty;
/// ```
#[proc_macro_derive(RosService, attributes(ros))]
pub fn derive_ros_service(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Extract attributes
    let mut service_name: Option<String> = None;
    let mut service_hash: Option<String> = None;
    let mut request_type: Option<syn::Ident> = None;
    let mut reply_type: Option<syn::Ident> = None;

    for attr in &input.attrs {
        if attr.path().is_ident("ros") {
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("service_name") {
                    let value: LitStr = meta.value()?.parse()?;
                    service_name = Some(value.value());
                } else if meta.path.is_ident("hash") {
                    let value: LitStr = meta.value()?.parse()?;
                    service_hash = Some(value.value());
                } else if meta.path.is_ident("request") {
                    let value: LitStr = meta.value()?.parse()?;
                    request_type = Some(syn::Ident::new(
                        &value.value(),
                        proc_macro2::Span::call_site(),
                    ));
                } else if meta.path.is_ident("reply") {
                    let value: LitStr = meta.value()?.parse()?;
                    reply_type = Some(syn::Ident::new(
                        &value.value(),
                        proc_macro2::Span::call_site(),
                    ));
                }
                Ok(())
            });
        }
    }

    let service_name =
        service_name.unwrap_or_else(|| format!("{}::srv::dds_::{}_", "unknown", name));
    let service_hash = service_hash.unwrap_or_else(|| "0".repeat(64));

    let request_type = match request_type {
        Some(t) => t,
        None => {
            return syn::Error::new_spanned(
                &input,
                "RosService requires #[ros(request = \"...\")]",
            )
            .to_compile_error()
            .into();
        }
    };

    let reply_type = match reply_type {
        Some(t) => t,
        None => {
            return syn::Error::new_spanned(&input, "RosService requires #[ros(reply = \"...\")]")
                .to_compile_error()
                .into();
        }
    };

    let expanded = quote! {
        impl nros_core::RosService for #name {
            type Request = #request_type;
            type Reply = #reply_type;

            const SERVICE_NAME: &'static str = #service_name;
            const SERVICE_HASH: &'static str = #service_hash;
        }
    };

    TokenStream::from(expanded)
}
