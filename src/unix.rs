macro_rules! lock_impl {
    ($file: ty) => {
        #[cfg(not(target_os = "wasi"))]
        pub fn lock_shared(file: &$file) -> std::io::Result<()> {
            flock(file, rustix::fs::FlockOperation::LockShared)
        }

        #[cfg(not(target_os = "wasi"))]
        pub fn lock_exclusive(file: &$file) -> std::io::Result<()> {
            flock(file, rustix::fs::FlockOperation::LockExclusive)
        }

        #[cfg(not(target_os = "wasi"))]
        pub fn try_lock_shared(file: &$file) -> std::io::Result<()> {
            flock(file, rustix::fs::FlockOperation::NonBlockingLockShared)
        }

        #[cfg(not(target_os = "wasi"))]
        pub fn try_lock_exclusive(file: &$file) -> std::io::Result<()> {
            flock(file, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        }

        #[cfg(not(target_os = "wasi"))]
        pub fn unlock(file: &$file) -> std::io::Result<()> {
            flock(file, rustix::fs::FlockOperation::Unlock)
        }

        #[cfg(not(target_os = "wasi"))]
        fn flock(file: &$file, flag: rustix::fs::FlockOperation) -> std::io::Result<()> {
            let borrowed_fd = unsafe { rustix::fd::BorrowedFd::borrow_raw(file.as_raw_fd()) };

            match rustix::fs::flock(borrowed_fd, flag) {
                Ok(_) => Ok(()),
                Err(e) => Err(std::io::Error::from_raw_os_error(e.raw_os_error())),
            }
        }
    };
}

#[cfg(any(feature = "smol-async", feature = "std-async", feature = "tokio-async"))]
pub(crate) mod async_impl;
#[cfg(feature = "sync")]
pub(crate) mod sync_impl;

use crate::FsStats;

use std::io::{Error, Result};
use std::path::Path;

pub fn lock_error() -> Error {
    Error::from_raw_os_error(rustix::io::Errno::WOULDBLOCK.raw_os_error())
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub fn statvfs(path: impl AsRef<Path>) -> Result<FsStats> {
    use std::ffi::CString;
    use std::io::ErrorKind;
    use std::mem;
    use std::os::unix::ffi::OsStrExt;

    let cstr = match CString::new(path.as_ref().as_os_str().as_bytes()) {
        Ok(cstr) => cstr,
        Err(..) => return Err(Error::new(ErrorKind::InvalidInput, "path contained a null")),
    };
    unsafe {
        let mut stat: libc::statvfs = mem::zeroed();
        // danburkert/fs2-rs#1: cast is necessary for platforms where c_char != u8.
        if libc::statvfs(cstr.as_ptr() as *const _, &mut stat) != 0 {
            Err(Error::last_os_error())
        } else {
            Ok(FsStats {
                free_space: stat.f_frsize as u64 * stat.f_bfree as u64,
                available_space: stat.f_frsize as u64 * stat.f_bavail as u64,
                total_space: stat.f_frsize as u64 * stat.f_blocks as u64,
                allocation_granularity: stat.f_frsize as u64,
            })
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
pub fn statvfs(path: impl AsRef<Path>) -> Result<FsStats> {
    match rustix::fs::statfs(path.as_ref()) {
        Ok(stat) => Ok(FsStats {
            free_space: stat.f_frsize as u64 * stat.f_bfree as u64,
            available_space: stat.f_frsize as u64 * stat.f_bavail as u64,
            total_space: stat.f_frsize as u64 * stat.f_blocks as u64,
            allocation_granularity: stat.f_frsize as u64,
        }),
        Err(e) => Err(std::io::Error::from_raw_os_error(e.raw_os_error())),
    }
}
