use crate::sys;

/// Why a device operation failed. Maps one to one onto the port ABI's return
/// codes, plus the refusals that happen at registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceError {
    /// The device refused the job (bad parameters, not ready).
    Rejected,
    /// The device reported a fault while running the job.
    Fault,
    /// The job did not complete in the time the port allows.
    Timeout,
    /// The operation is not implemented by this port.
    Unsupported,
    /// The port's operation table is a version this runtime does not speak,
    /// or lacks an operation every port must have.
    AbiMismatch,
    /// The worker is running a job; submit again after its result arrives.
    Busy,
    /// The device is already registered; a device has one owner.
    AlreadyClaimed,
    /// The executor refused the registration: its callback table is full, or
    /// it has already started spinning.
    Registration,
}

impl DeviceError {
    /// The port ABI's code for this error.
    pub const fn to_ret(self) -> sys::nros_device_ret_t {
        match self {
            Self::Rejected | Self::Busy | Self::AlreadyClaimed | Self::Registration => {
                sys::NROS_DEVICE_RET_REJECTED
            }
            Self::Fault => sys::NROS_DEVICE_RET_FAULT,
            Self::Timeout => sys::NROS_DEVICE_RET_TIMEOUT,
            Self::Unsupported => sys::NROS_DEVICE_RET_UNSUPPORTED,
            Self::AbiMismatch => sys::NROS_DEVICE_RET_ABI_MISMATCH,
        }
    }

    /// The error a port ABI code names. `OK` and `PENDING` are not errors, so
    /// they read as `None`; an unknown negative code reads as [`Fault`].
    ///
    /// [`Fault`]: Self::Fault
    pub const fn from_ret(ret: sys::nros_device_ret_t) -> Option<Self> {
        match ret {
            sys::NROS_DEVICE_RET_OK | sys::NROS_DEVICE_RET_PENDING => None,
            sys::NROS_DEVICE_RET_REJECTED => Some(Self::Rejected),
            sys::NROS_DEVICE_RET_TIMEOUT => Some(Self::Timeout),
            sys::NROS_DEVICE_RET_UNSUPPORTED => Some(Self::Unsupported),
            sys::NROS_DEVICE_RET_ABI_MISMATCH => Some(Self::AbiMismatch),
            _ => Some(Self::Fault),
        }
    }

    /// `Ok(())` for `OK`, the error otherwise. `PENDING` reads as `Ok(())`:
    /// callers that care about it check it first.
    pub(crate) const fn check(ret: sys::nros_device_ret_t) -> Result<(), Self> {
        match Self::from_ret(ret) {
            None => Ok(()),
            Some(e) => Err(e),
        }
    }
}
