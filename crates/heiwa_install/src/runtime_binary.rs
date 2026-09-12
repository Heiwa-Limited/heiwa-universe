//! The desktop and CLI install the same portable runtime into each user's root.
use anyhow::{bail, Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Copy a packaged runtime without truncating the executable a shell may be using.
/// No checkout, compiler, package manager, or administrator access is required.
pub fn install_runtime_binary(root: &Path, source: &Path) -> Result<PathBuf> {
    // The optional Apple reader shares the runtime protocol. Stage it before
    // publishing the CLI that can call it; old CLIs do not consume this helper.
    let helper = source.with_file_name("heiwa-apple-resources");
    if helper.is_file() {
        install_binary(root, &helper, "heiwa-apple-resources")?;
    }
    install_binary(
        root,
        source,
        if cfg!(windows) { "heiwa.exe" } else { "heiwa" },
    )
}

fn install_binary(root: &Path, source: &Path, name: &str) -> Result<PathBuf> {
    let metadata = fs::metadata(source).context("read bundled runtime")?;
    if !metadata.is_file() || metadata.len() == 0 {
        bail!("The bundled Heiwa runtime is missing or empty.");
    }
    let bin = root.join("bin");
    fs::create_dir_all(&bin)?;
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options.open(bin.join(".runtime-install.lock"))?;
    lock.lock().context("lock runtime installation")?;
    let target = bin.join(name);
    if source == target || files_match(source, &target)? {
        return Ok(target);
    }
    let mut staged = tempfile::NamedTempFile::new_in(&bin)?;
    std::io::copy(&mut File::open(source)?, &mut staged)?;
    staged.flush()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        staged
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o755))?;
    }
    staged.as_file().sync_all()?;
    // Keep one coherent predecessor. A failed copy or promotion leaves the
    // installed executable intact; the rename never exposes a partial binary.
    if target.is_file() {
        let mut backup = tempfile::NamedTempFile::new_in(&bin)?;
        std::io::copy(&mut File::open(&target)?, &mut backup)?;
        backup
            .as_file()
            .set_permissions(fs::metadata(&target)?.permissions())?;
        backup.as_file().sync_all()?;
        backup
            .persist(bin.join(format!("{name}.previous")))
            .context("preserve previous runtime")?;
    }
    staged.persist(&target).context("install bundled runtime")?;
    #[cfg(unix)]
    File::open(&bin)?.sync_all()?;
    Ok(target)
}

fn files_match(source: &Path, target: &Path) -> Result<bool> {
    let mut target = match File::open(target) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let mut source = File::open(source)?;
    if source.metadata()?.len() != target.metadata()?.len() {
        return Ok(false);
    }
    let (mut left, mut right) = ([0_u8; 64 * 1024], [0_u8; 64 * 1024]);
    loop {
        let n = source.read(&mut left)?;
        if n == 0 {
            return Ok(true);
        }
        target.read_exact(&mut right[..n])?;
        if left[..n] != right[..n] {
            return Ok(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_launch_and_update_are_portable_and_preserve_previous_bytes() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let source = temp.path().join("bundle/heiwa");
        fs::create_dir_all(source.parent().unwrap())?;
        fs::write(&source, "runtime one")?;
        let root = temp.path().join("first user");
        let target = install_runtime_binary(&root, &source)?;
        assert_eq!(fs::read(&target)?, b"runtime one");
        let modified = fs::metadata(&target)?.modified()?;
        install_runtime_binary(&root, &source)?;
        assert_eq!(fs::metadata(&target)?.modified()?, modified);
        fs::write(&source, "runtime two")?;
        install_runtime_binary(&root, &source)?;
        assert_eq!(fs::read(&target)?, b"runtime two");
        assert_eq!(fs::read(root.join("bin/heiwa.previous"))?, b"runtime one");
        let second = install_runtime_binary(&temp.path().join("second user"), &source)?;
        assert_ne!(target, second);
        fs::write(&source, "")?;
        assert!(install_runtime_binary(&root, &source).is_err());
        assert_eq!(fs::read(&target)?, b"runtime two");
        Ok(())
    }
}
