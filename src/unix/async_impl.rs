macro_rules! allocate {
    ($file: ty) => {
        #[cfg(any(target_os = "linux",
        target_os = "freebsd",
        target_os = "android",
        target_os = "emscripten",
        target_os = "nacl"))]
        pub async fn allocate(file: &$file, len: u64) -> std::io::Result<()> {
            let ret = unsafe { libc::posix_fallocate(file.as_raw_fd(), 0, len as libc::off_t) };
            if ret == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
        }

        #[cfg(any(target_os = "macos", target_os = "ios"))]
        pub async fn allocate(file: &$file, len: u64) -> std::io::Result<()> {
            let stat = file.metadata().await?;

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
                file.set_len(len).await
            } else {
                Ok(())
            }
        }

        #[cfg(any(target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly",
        target_os = "solaris",
        target_os = "haiku"))]
        pub async fn allocate(file: &$file, len: u64) -> std::io::Result<()> {
            // No file allocation API available, just set the length if necessary.
            if len > file.metadata().await?.len() as u64 {
                file.set_len(len).await
            } else {
                Ok(())
            }
        }
    };
}

macro_rules! allocate_size {
    ($file: ty) => {
        pub async fn allocated_size(file: &$file) -> std::io::Result<u64> {
            file.metadata().await.map(|m| m.blocks() as u64 * 512)
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









