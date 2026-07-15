// SPDX-License-Identifier: BSD-3-Clause

#[cfg(not(feature = "test-remount"))]
use log::info;
#[cfg(not(feature = "test-remount"))]
use std::fs;
use std::path::Path;
#[cfg(not(feature = "test-remount"))]
use std::process::{Command, Stdio};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MountError {
    #[error("Failed to remount /boot: {0}")]
    RemountFailed(String),
    #[error("Failed to create mount namespace: {0}")]
    NamespaceError(String),
    #[error("Failed to check /boot writable state: {0}")]
    BootCheckError(String),
}

/// Check if the process is already in a private mount namespace by comparing
/// the mount namespace of PID 1 with the current process.
#[cfg(not(feature = "test-remount"))]
fn is_in_private_namespace() -> Result<bool, MountError> {
    let pid1_ns = fs::read_link("/proc/1/ns/mnt")
        .map_err(|e| MountError::NamespaceError(format!("Failed to read /proc/1/ns/mnt: {e}")))?;
    let self_ns = fs::read_link("/proc/self/ns/mnt").map_err(|e| {
        MountError::NamespaceError(format!("Failed to read /proc/self/ns/mnt: {e}"))
    })?;
    Ok(pid1_ns != self_ns)
}

/// Ensure the process is running inside a private mount namespace.
/// If already in a private namespace (e.g. via systemd PrivateMounts=yes), this is a no-op.
/// Otherwise, calls unshare(CLONE_NEWNS) to create one.
#[cfg(not(feature = "test-remount"))]
pub fn ensure_mount_namespace() -> Result<(), MountError> {
    if is_in_private_namespace()? {
        info!("Already in a private mount namespace; skipping unshare");
        return Ok(());
    }
    info!("Not in a private mount namespace; creating one via unshare(CLONE_NEWNS)");
    nix::sched::unshare(nix::sched::CloneFlags::CLONE_NEWNS).map_err(|e| {
        MountError::NamespaceError(format!("Failed to create mount namespace: {e}"))
    })?;
    info!("Mount namespace created successfully");
    Ok(())
}

#[cfg(feature = "test-remount")]
pub fn ensure_mount_namespace() -> Result<(), MountError> {
    Ok(())
}

/// Check if a path is writable using POSIX access(2) with W_OK.
/// This correctly detects read-only mounts, unlike metadata mode bits
/// which only reflect inode permissions. Equivalent to shell `test -w`.
fn is_path_writable_at(path: &Path) -> Result<bool, MountError> {
    match nix::unistd::access(path, nix::unistd::AccessFlags::W_OK) {
        Ok(()) => Ok(true),
        Err(nix::errno::Errno::EACCES | nix::errno::Errno::EROFS) => Ok(false),
        Err(e) => Err(MountError::BootCheckError(format!(
            "{}: {e}",
            path.display()
        ))),
    }
}

/// Check if /boot is currently writable.
pub fn is_boot_writable() -> Result<bool, MountError> {
    is_boot_writable_at(Path::new("/boot"))
}

/// Check if a given path is currently writable. Testable variant of `is_boot_writable`.
pub fn is_boot_writable_at(path: &Path) -> Result<bool, MountError> {
    is_path_writable_at(path)
}

/// Remount /boot as read-write. With mount namespaces, there is no need to
/// restore read-only state afterward — the namespace handles cleanup on exit.
#[cfg(not(feature = "test-remount"))]
pub fn remount_boot_rw() -> Result<(), MountError> {
    if is_boot_writable()? {
        info!("/boot is already writable; no remount needed");
        return Ok(());
    }

    info!("Remounting /boot as read-write");
    let output = Command::new("mount")
        .arg("-o")
        .arg("remount,rw")
        .arg("/boot")
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                let error_message = String::from_utf8_lossy(&output.stderr);
                Err(MountError::RemountFailed(error_message.to_string()))
            }
        }
        Err(e) => Err(MountError::RemountFailed(format!(
            "Failed to execute mount: {e}"
        ))),
    }
}

#[cfg(feature = "test-remount")]
pub fn remount_boot_rw() -> Result<(), MountError> {
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn test_is_boot_writable_at_on_writable_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(is_boot_writable_at(dir.path()).unwrap());
    }

    #[test]
    fn test_is_boot_writable_at_on_readonly_dir() {
        if nix::unistd::geteuid().is_root() {
            eprintln!("Skipping: root bypasses permission bits in access(W_OK)");
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let mut perms = fs::metadata(dir.path()).unwrap().permissions();
        perms.set_mode(0o555);
        fs::set_permissions(dir.path(), perms).unwrap();

        assert!(!is_boot_writable_at(dir.path()).unwrap());

        let mut perms = fs::metadata(dir.path()).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(dir.path(), perms).unwrap();
    }

    #[test]
    fn test_is_boot_writable_at_nonexistent_path() {
        let result = is_boot_writable_at(Path::new("/nonexistent/path/boot"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("/nonexistent/path/boot"));
    }
}
