/// Maximum number of parameters the server can store (set via NROS_MAX_PARAMETERS, default 32).
pub const MAX_PARAMETERS: usize = 32;

/// Maximum length for parameter names (set via NROS_MAX_PARAM_NAME_LEN, default 64).
pub const MAX_PARAM_NAME_LEN: usize = 64;

/// Maximum length for parameter string values (set via NROS_MAX_STRING_VALUE_LEN, default 256).
pub const MAX_STRING_VALUE_LEN: usize = 256;

/// Maximum length for array parameters (set via NROS_MAX_ARRAY_LEN, default 32).
pub const MAX_ARRAY_LEN: usize = 32;

/// Maximum length for byte array parameters (set via NROS_MAX_BYTE_ARRAY_LEN, default 256).
pub const MAX_BYTE_ARRAY_LEN: usize = 256;
