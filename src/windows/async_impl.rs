macro_rules! allocate_size {
    ($file: ty) => {
        pub async fn allocated_size(file: &$file) -> Result<u64> {
            unsafe {
                let mut info: FILE_STANDARD_INFO = mem::zeroed();

                let ret = GetFileInformationByHandleEx(
                    file.as_raw_handle(),
                    FileStandardInfo,
                    &mut info as *mut _ as *mut _,
                    mem::size_of::<FILE_STANDARD_INFO>() as DWORD,
                );

                if ret == 0 {
                    Err(Error::last_os_error())
                } else {
                    Ok(*info.AllocationSize.QuadPart() as u64)
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
                    let mut info: FILE_ALLOCATION_INFO = mem::zeroed();
                    *info.AllocationSize.QuadPart_mut() = len as i64;
                    let ret = SetFileInformationByHandle(
                        file.as_raw_handle(),
                        FileAllocationInfo,
                        &mut info as *mut _ as *mut _,
                        mem::size_of::<FILE_ALLOCATION_INFO>() as DWORD,
                    );
                    if ret == 0 {
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
