macro_rules! allocate_size {
    ($file: ty) => {
        pub async fn allocated_size(file: &$file) -> Result<u64> {
            unsafe {
                let mut info: FILE_STANDARD_INFO = mem::zeroed();

                let ret = GetFileInformationByHandleEx(
                    file.as_raw_handle() as HANDLE,
                    FileStandardInfo,
                    &mut info as *mut _ as *mut _,
                    mem::size_of::<FILE_STANDARD_INFO>() as u32,
                );

                if ret == 0 {
                    Err(Error::last_os_error())
                } else {
                    Ok(info.AllocationSize as u64)
                }
            }
        }
    };
}

macro_rules! allocate {
    ($file: ty) => {
        pub async fn allocate(file: &$file, len: u64) -> Result<()> {
            if allocated_size(file).await? < len {
                unsafe {
                    if SetFilePointerEx(
                        file.as_raw_handle() as HANDLE,
                        len as i64,
                        ptr::null_mut(),
                        FILE_BEGIN,
                    ) == 0
                    {
                        return Err(Error::last_os_error());
                    }

                    if SetEndOfFile(file.as_raw_handle() as HANDLE) == 0 {
                        return Err(Error::last_os_error());
                    }
                }
            }
            if file.metadata().await?.len() < len {
                file.set_len(len).await
            } else {
                Ok(())
            }
        }
    };
}

cfg_async_std! {
    pub(crate) mod async_std_impl;
}

cfg_smol! {
    pub(crate) mod smol_impl;
}

cfg_tokio! {
    pub(crate) mod tokio_impl;
}
