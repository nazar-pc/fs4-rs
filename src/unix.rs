macro_rules! duplicate {
    ($file: ty) => {
        pub fn duplicate(file: &$file) -> std::io::Result<$file> {
            unsafe {
                let fd = libc::dup(file.as_raw_fd());

                if fd < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(<$file>::from_raw_fd(fd))
                }
            }
        }
    };
}

macro_rules! lock_impl {
    ($file: ty) => {
        pub fn lock_shared(file: &$file) -> std::io::Result<()> {
            flock(file, libc::LOCK_SH)
        }

        pub fn lock_exclusive(file: &$file) -> std::io::Result<()> {
            flock(file, libc::LOCK_EX)
        }

        pub fn try_lock_shared(file: &$file) -> std::io::Result<()> {
            flock(file, libc::LOCK_SH | libc::LOCK_NB)
        }

        pub fn try_lock_exclusive(file: &$file) -> std::io::Result<()> {
            flock(file, libc::LOCK_EX | libc::LOCK_NB)
        }

        pub fn unlock(file: &$file) -> std::io::Result<()> {
            flock(file, libc::LOCK_UN)
        }

        /// Simulate flock() using fcntl(); primarily for Oracle Solaris.
        #[cfg(target_os = "solaris")]
        fn flock(file: &$file, flag: libc::c_int) -> std::io::Result<()> {
            let mut fl = libc::flock {
                l_whence: 0,
                l_start: 0,
                l_len: 0,
                l_type: 0,
                l_pad: [0; 4],
                l_pid: 0,
                l_sysid: 0,
            };

            // In non-blocking mode, use F_SETLK for cmd, F_SETLKW otherwise, and don't forget to clear
            // LOCK_NB.
            let (cmd, operation) = match flag & libc::LOCK_NB {
                0 => (libc::F_SETLKW, flag),
                _ => (libc::F_SETLK, flag & !libc::LOCK_NB),
            };

            match operation {
                libc::LOCK_SH => fl.l_type |= libc::F_RDLCK,
                libc::LOCK_EX => fl.l_type |= libc::F_WRLCK,
                libc::LOCK_UN => fl.l_type |= libc::F_UNLCK,
                _ => return Err(Error::from_raw_os_error(libc::EINVAL)),
            }

            let ret = unsafe { libc::fcntl(file.as_raw_fd(), cmd, &fl) };
            match ret {
                // Translate EACCES to EWOULDBLOCK
                -1 => match std::io::Error::last_os_error().raw_os_error() {
                    Some(libc::EACCES) => return Err(lock_error()),
                    _ => return Err(std::io::Error::last_os_error()),
                },
                _ => Ok(()),
            }
        }

        #[cfg(not(target_os = "solaris"))]
        fn flock(file: &$file, flag: libc::c_int) -> std::io::Result<()> {
            let ret = unsafe { libc::flock(file.as_raw_fd(), flag) };
            if ret < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    };
}

#[cfg(any(feature = "smol-async", feature = "std-async", feature = "tokio-async"))]
pub(crate) mod async_impl;
#[cfg(feature = "sync")]
pub(crate) mod sync_impl;

use crate::FsStats;
use std::ffi::CString;
use std::io::{Error, ErrorKind, Result};
use std::mem;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

pub fn lock_error() -> Error {
    Error::from_raw_os_error(libc::EWOULDBLOCK)
}

pub fn statvfs(path: &Path) -> Result<FsStats> {
    let cstr = match CString::new(path.as_os_str().as_bytes()) {
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
