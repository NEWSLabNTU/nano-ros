//! smoltcp TCP/UDP networking via nros-smoltcp.
//!
//! All five impl blocks (TCP, UDP, socket helpers, multicast stubs)
//! come from `nros_smoltcp::define_smoltcp_platform!`. The 502-line
//! body that used to live here is now in the macro definition; this
//! module only names the platform ZST.

nros_smoltcp::define_smoltcp_platform!(Esp32QemuPlatform);
