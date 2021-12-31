use std::os::unix::fs::MetadataExt;
use std::os::unix::io::{AsRawFd, FromRawFd};
use smol::fs::File;

duplicate!(File);
lock_impl!(File);
allocate!(File);
allocate_size!(File);

#[cfg(test)]
mod test {
    extern crate tempdir;
    extern crate libc;

    use smol::fs::{self, File};
    use std::os::unix::io::AsRawFd;

    use crate::{lock_contended_error, smol::AsyncFileExt};

    /// The duplicate method returns a file with a new file descriptor.
    #[smol_potat::test]
    async fn duplicate_new_fd() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file1 = fs::OpenOptions::new().write(true).create(true).open(&path).await.unwrap();
        let file2 = file1.duplicate().unwrap();
        assert_ne!(file1.as_raw_fd(), file2.as_raw_fd());
    }

    /// The duplicate method should preservesthe close on exec flag.
    #[smol_potat::test]
    async fn duplicate_cloexec() {

        fn flags(file: &File) -> libc::c_int {
            unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL, 0) }
        }

        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file1 = fs::OpenOptions::new().write(true).create(true).open(&path).await.unwrap();
        let file2 = file1.duplicate().unwrap();

        assert_eq!(flags(&file1), flags(&file2));
    }

    /// Tests that locking a file descriptor will replace any existing locks
    /// held on the file descriptor.
    #[smol_potat::test]
    async fn lock_replace() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file1 = fs::OpenOptions::new().write(true).create(true).open(&path).await.unwrap();
        let file2 = fs::OpenOptions::new().write(true).create(true).open(&path).await.unwrap();

        // Creating a shared lock will drop an exclusive lock.
        file1.lock_exclusive().unwrap();
        file1.lock_shared().unwrap();
        file2.lock_shared().unwrap();

        // Attempting to replace a shared lock with an exclusive lock will fail
        // with multiple lock holders, and remove the original shared lock.
        assert_eq!(file2.try_lock_exclusive().unwrap_err().raw_os_error(),
                   lock_contended_error().raw_os_error());
        file1.lock_shared().unwrap();
    }

    /// Tests that locks are shared among duplicated file descriptors.
    #[smol_potat::test]
    async fn lock_duplicate() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file1 = fs::OpenOptions::new().write(true).create(true).open(&path).await.unwrap();
        let file2 = file1.duplicate().unwrap();
        let file3 = fs::OpenOptions::new().write(true).create(true).open(&path).await.unwrap();

        // Create a lock through fd1, then replace it through fd2.
        file1.lock_shared().unwrap();
        file2.lock_exclusive().unwrap();
        assert_eq!(file3.try_lock_shared().unwrap_err().raw_os_error(),
                   lock_contended_error().raw_os_error());

        // Either of the file descriptors should be able to unlock.
        file1.unlock().unwrap();
        file3.lock_shared().unwrap();
    }
}