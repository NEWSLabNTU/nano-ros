// RMW (Raw Message Wrapper) for String message
#[repr(C)]
pub struct String {
    pub data: *const std::os::raw::c_char,
}
