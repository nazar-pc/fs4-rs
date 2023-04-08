use std::fs::File;
use std::os::unix::fs::MetadataExt;
use std::os::unix::io::AsRawFd;

lock_impl!(File);

pub fn allocated_size(file: &File) -> std::io::Result<u64> {
    file.metadata().map(|m| m.blocks() as u64 * 512)
}

#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "android",
    target_os = "emscripten",
    target_os = "nacl"
))]
pub fn allocate(file: &File, len: u64) -> std::io::Result<()> {
    use rustix::{
        fd::BorrowedFd,
        fs::{fallocate, FallocateFlags},
    };
    unsafe {
        let borrowed_fd = BorrowedFd::borrow_raw(file.as_raw_fd());
        match fallocate(borrowed_fd, FallocateFlags::from_bits_unchecked(0), 0, len) {
            Ok(_) => Ok(()),
            Err(e) => Err(std::io::Error::from_raw_os_error(e.raw_os_error())),
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub fn allocate(file: &File, len: u64) -> std::io::Result<()> {
    let stat = file.metadata()?;

    if len > stat.blocks() as u64 * 512 {
        let mut fstore = libc::fstore_t {
            fst_flags: libc::F_ALLOCATECONTIG,
            fst_posmode: libc::F_PEOFPOSMODE,
            fst_offset: 0,
            fst_length: len as libc::off_t,
            fst_bytesalloc: 0,
        };

        let ret = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_PREALLOCATE, &fstore) };
        if ret == -1 {
            // Unable to allocate contiguous disk space; attempt to allocate non-contiguously.
            fstore.fst_flags = libc::F_ALLOCATEALL;
            let ret = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_PREALLOCATE, &fstore) };
            if ret == -1 {
                return Err(std::io::Error::last_os_error());
            }
        }
    }

    if len > stat.size() as u64 {
        file.set_len(len)
    } else {
        Ok(())
    }
}

#[cfg(any(
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly",
    target_os = "solaris",
    target_os = "illumos",
    target_os = "haiku",
))]
pub fn allocate(file: &File, len: u64) -> std::io::Result<()> {
    // No file allocation API available, just set the length if necessary.
    if len > file.metadata()?.len() as u64 {
        file.set_len(len)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod test {
    extern crate tempdir;

    use std::fs;

    use crate::{lock_contended_error, FileExt};

    /// Tests that locking a file descriptor will replace any existing locks
    /// held on the file descriptor.
    #[test]
    fn lock_replace() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file1 = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&path)
            .unwrap();
        let file2 = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&path)
            .unwrap();

        // Creating a shared lock will drop an exclusive lock.
        file1.lock_exclusive().unwrap();
        file1.lock_shared().unwrap();
        file2.lock_shared().unwrap();

        // Attempting to replace a shared lock with an exclusive lock will fail
        // with multiple lock holders, and remove the original shared lock.
        assert_eq!(
            file2.try_lock_exclusive().unwrap_err().raw_os_error(),
            lock_contended_error().raw_os_error()
        );
        file1.lock_shared().unwrap();
    }
}
