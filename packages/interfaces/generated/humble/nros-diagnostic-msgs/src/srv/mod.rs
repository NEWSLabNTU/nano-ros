//! Service types for this package

mod add_diagnostics;
pub use add_diagnostics::{AddDiagnostics, AddDiagnosticsRequest, AddDiagnosticsResponse};

mod self_test;
pub use self_test::{SelfTest, SelfTestRequest, SelfTestResponse};
