use std::fs::File;
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

duplicate!(File);
lock_impl!(File);

pub fn allocated_size(file: &File) -> Result<u64> {
    unsafe {
        let mut info: FILE_STANDARD_INFO = mem::zeroed();

        let ret = GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileStandardInfo,
            &mut info as *mut _ as *mut _,
            mem::size_of::<FILE_STANDARD_INFO>() as DWORD);

        if ret == 0 {
            Err(Error::last_os_error())
        } else {
            Ok(*info.AllocationSize.QuadPart() as u64)
        }
    }
}

pub fn allocate(file: &File, len: u64) -> Result<()> {
    if allocated_size(file)? < len {
        unsafe {
            let mut info: FILE_ALLOCATION_INFO = mem::zeroed();
            *info.AllocationSize.QuadPart_mut() = len as i64;
            let ret = SetFileInformationByHandle(
                file.as_raw_handle(),
                FileAllocationInfo,
                &mut info as *mut _ as *mut _,
                mem::size_of::<FILE_ALLOCATION_INFO>() as DWORD);
            if ret == 0 {
                return Err(Error::last_os_error());
            }
        }
    }
    if file.metadata()?.len() < len {
        file.set_len(len)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod test {

    extern crate tempdir;

    use std::fs;
    use std::os::windows::io::AsRawHandle;

    use crate::{lock_contended_error, FileExt, FsStats};
    
    /// The duplicate method returns a file with a new file handle.
    #[test]
    fn duplicate_new_handle() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file1 = fs::OpenOptions::new().write(true).create(true).open(&path).unwrap();
        let file2 = file1.duplicate().unwrap();
        assert!(file1.as_raw_handle() != file2.as_raw_handle());
    }

    /// A duplicated file handle does not have access to the original handle's locks.
    #[test]
    fn lock_duplicate_handle_independence() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file1 = fs::OpenOptions::new().read(true).write(true).create(true).open(&path).unwrap();
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
    #[test]
    fn lock_non_reentrant() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file = fs::OpenOptions::new().read(true).write(true).create(true).open(&path).unwrap();

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
    #[test]
    fn lock_layering() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file = fs::OpenOptions::new().read(true).write(true).create(true).open(&path).unwrap();

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
    #[test]
    fn lock_layering_cleanup() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file1 = fs::OpenOptions::new().read(true).write(true).create(true).open(&path).unwrap();
        let file2 = fs::OpenOptions::new().read(true).write(true).create(true).open(&path).unwrap();

        // Open two shared locks on the file, and then try and fail to open an exclusive lock.
        file1.lock_shared().unwrap();
        assert_eq!(file2.try_lock_exclusive().unwrap_err().raw_os_error(),
                   lock_contended_error().raw_os_error());

        drop(file1);
        file2.lock_exclusive().unwrap();
    }

    /// A file handle's locks will not be released until the original handle and all of its
    /// duplicates have been closed. This on really smells like a bug in Windows.
    #[test]
    fn lock_duplicate_cleanup() {
        let tempdir = tempdir::TempDir::new("fs4").unwrap();
        let path = tempdir.path().join("fs4");
        let file1 = fs::OpenOptions::new().read(true).write(true).create(true).open(&path).unwrap();
        let file2 = file1.duplicate().unwrap();

        // Open a lock on the original handle, then close it.
        file1.lock_shared().unwrap();
        drop(file1);

        // Attempting to create a lock on the file with the duplicate handle will fail.
        assert_eq!(file2.try_lock_exclusive().unwrap_err().raw_os_error(),
                   lock_contended_error().raw_os_error());
    }
}
