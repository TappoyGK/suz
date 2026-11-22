//! 出力を一時ディレクトリに準備し、既存の出力先を上書きせずに公開する。失敗時には一時領域を片付ける。
//!
//! 利用元: [`crate::cli::emit`]、[`crate::cli::native`]。

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub struct StagedDirectory(Option<PathBuf>);

impl StagedDirectory {
    pub fn new(parent: &Path) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let path = parent.join(format!(
                ".suz-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Ok(Self(Some(path))),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "cannot allocate a staging directory",
        ))
    }

    pub fn path(&self) -> &Path {
        self.0
            .as_deref()
            .expect("staging directory exists until publication")
    }

    pub fn publish(mut self, destination: &Path) -> io::Result<()> {
        rename_new(self.path(), destination)?;
        self.0 = None;
        Ok(())
    }
}

impl Drop for StagedDirectory {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = fs::remove_dir_all(path);
        }
    }
}

#[cfg(target_os = "macos")]
pub fn rename_new(source: &Path, destination: &Path) -> io::Result<()> {
    use std::{
        ffi::{CString, c_char, c_int, c_uint},
        os::unix::ffi::OsStrExt,
    };
    unsafe extern "C" {
        fn renamex_np(from: *const c_char, to: *const c_char, flags: c_uint) -> c_int;
    }
    let from = CString::new(source.as_os_str().as_bytes())?;
    let to = CString::new(destination.as_os_str().as_bytes())?;
    // sys/stdio.hのRENAME_EXCLは、出力先が既にあれば不可分に拒否する。
    // 最初の確認後に作られた空ディレクトリも上書きしない。
    let result = unsafe { renamex_np(from.as_ptr(), to.as_ptr(), 0x00000004) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "macos"))]
pub fn rename_new(_source: &Path, _destination: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic directory publication is unsupported on this platform",
    ))
}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use super::*;

    #[test]
    fn publication_refuses_an_empty_directory_created_after_staging() {
        let root = StagedDirectory::new(&std::env::temp_dir()).unwrap();
        let staged = StagedDirectory::new(root.path()).unwrap();
        fs::write(staged.path().join("complete"), b"contents").unwrap();
        let temporary = staged.path().to_owned();
        let destination = root.path().join("output");
        fs::create_dir(&destination).unwrap();
        assert!(staged.publish(&destination).is_err());
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
        assert!(!temporary.exists());
    }
}
