use std::io::{Error, Result};
use std::mem;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::ptr;

use winapi::shared::minwindef::{BOOL, DWORD};
use winapi::um::fileapi::{FILE_ALLOCATION_INFO, FILE_STANDARD_INFO};
use winapi::um::fileapi::{LockFileEx, UnlockFile, SetFileInformationByHandle};
use winapi::um::handleapi::DuplicateHandle;
use winapi::um::minwinbase::{FileAllocationInfo, FileStandardInfo};
use winapi::um::minwinbase::{LOCKFILE_FAIL_IMMEDIATELY, LOCKFILE_EXCLUSIVE_LOCK};
use winapi::um::processthreadsapi::GetCurrentProcess;
use winapi::um::winbase::GetFileInformationByHandleEx;
use winapi::um::winnt::DUPLICATE_SAME_ACCESS;

use async_std::fs::File;

duplicate!(File);
lock_impl!(File);
allocate!(File);
allocate_size!(File);

#[cfg(test)]
mod test {

    extern crate tempdir;

    use async_std::fs;
    use std::os::windows::io::AsRawHandle;

    use crate::{lock_contended_error, async_std::AsyncFileExt, FsStats};

    /// The duplicate method returns a file with a new file handle.
    #[async_std::test]
    async fn duplicate_new_handle() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file1 = fs::OpenOptions::new().write(true).create(true).open(&path).await.unwrap();
        let file2 = file1.duplicate().unwrap();
        assert!(file1.as_raw_handle() != file2.as_raw_handle());
    }

    /// A duplicated file handle does not have access to the original handle's locks.
    #[async_std::test]
    async fn lock_duplicate_handle_independence() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file1 = fs::OpenOptions::new().read(true).write(true).create(true).open(&path).await.unwrap();
        let file2 = file1.duplicate().unwrap();

        // Locking the original file handle will block the duplicate file handle from opening a lock.
        file1.lock_shared().unwrap();
        assert_eq!(file2.try_lock_exclusive().unwrap_err().raw_os_error(),
                   lock_contended_error().raw_os_error());

        // Once the original file handle is unlocked, the duplicate handle can proceed with a lock.
        file1.unlock().unwrap();
        file2.lock_exclusive().unwrap();
    }

    /// A file handle may not be exclusively locked multiple times, or exclusively locked and then
    /// shared locked.
    #[async_std::test]
    async fn lock_non_reentrant() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file = fs::OpenOptions::new().read(true).write(true).create(true).open(&path).await.unwrap();

        // Multiple exclusive locks fails.
        file.lock_exclusive().unwrap();
        assert_eq!(file.try_lock_exclusive().unwrap_err().raw_os_error(),
                   lock_contended_error().raw_os_error());
        file.unlock().unwrap();

        // Shared then Exclusive locks fails.
        file.lock_shared().unwrap();
        assert_eq!(file.try_lock_exclusive().unwrap_err().raw_os_error(),
                   lock_contended_error().raw_os_error());
    }

    /// A file handle can hold an exclusive lock and any number of shared locks, all of which must
    /// be unlocked independently.
    #[async_std::test]
    async fn lock_layering() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file = fs::OpenOptions::new().read(true).write(true).create(true).open(&path).await.unwrap();

        // Open two shared locks on the file, and then try and fail to open an exclusive lock.
        file.lock_exclusive().unwrap();
        file.lock_shared().unwrap();
        file.lock_shared().unwrap();
        assert_eq!(file.try_lock_exclusive().unwrap_err().raw_os_error(),
                   lock_contended_error().raw_os_error());

        // Pop one of the shared locks and try again.
        file.unlock().unwrap();
        assert_eq!(file.try_lock_exclusive().unwrap_err().raw_os_error(),
                   lock_contended_error().raw_os_error());

        // Pop the second shared lock and try again.
        file.unlock().unwrap();
        assert_eq!(file.try_lock_exclusive().unwrap_err().raw_os_error(),
                   lock_contended_error().raw_os_error());

        // Pop the exclusive lock and finally succeed.
        file.unlock().unwrap();
        file.lock_exclusive().unwrap();
    }

    /// A file handle with multiple open locks will have all locks closed on drop.
    #[async_std::test]
    async fn lock_layering_cleanup() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file1 = fs::OpenOptions::new().read(true).write(true).create(true).open(&path).await.unwrap();
        let file2 = fs::OpenOptions::new().read(true).write(true).create(true).open(&path).await.unwrap();

        // Open two shared locks on the file, and then try and fail to open an exclusive lock.
        file1.lock_shared().unwrap();
        assert_eq!(file2.try_lock_exclusive().unwrap_err().raw_os_error(),
                   lock_contended_error().raw_os_error());

        drop(file1);
        file2.lock_exclusive().unwrap();
    }

    /// A file handle's locks will not be released until the original handle and all of its
    /// duplicates have been closed. This on really smells like a bug in Windows.
    #[async_std::test]
    async fn lock_duplicate_cleanup() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file1 = fs::OpenOptions::new().read(true).write(true).create(true).open(&path).await.unwrap();
        let file2 = file1.duplicate().unwrap();

        // Open a lock on the original handle, then close it.
        file1.lock_shared().unwrap();
        drop(file1);

        // Attempting to create a lock on the file with the duplicate handle will fail.
        assert_eq!(file2.try_lock_exclusive().unwrap_err().raw_os_error(),
                   lock_contended_error().raw_os_error());
    }
}