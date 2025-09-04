// SPDX-License-Identifier: BSD-3-Clause

use log::{info, warn};
use std::fs;
use std::path::Path;
#[cfg(not(feature = "test-remount"))]
use std::process::{Command, Stdio};
use thiserror::Error;

/// Shared path to mount info used by default helpers
static MOUNT_INFO_PATH: &str = "/proc/mounts";

#[derive(Debug, Error)]
pub enum MountError {
    #[error("Failed to remount /boot: {0}")]
    RemountFailed(String),
    #[error("Failed to read mount info")]
    MountInfoError,
}

fn is_boot_rw_at(mounts_path: &Path) -> Result<bool, MountError> {
    let mounts = fs::read_to_string(mounts_path).map_err(|_| MountError::MountInfoError)?;
    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 && parts.get(1) == Some(&"/boot") {
            let options = parts[3];
            return Ok(options.contains("rw") && !options.contains("ro"));
        }
    }
    Err(MountError::MountInfoError)
}

/// Default helper: check /boot RW state using shared MOUNT_INFO_PATH
pub fn is_boot_rw() -> Result<bool, MountError> {
    is_boot_rw_at(Path::new(MOUNT_INFO_PATH))
}

#[cfg(not(feature = "test-remount"))]
fn remount_boot_ro_at(mounts_path: &Path) -> Result<(), MountError> {
    match is_boot_rw_at(mounts_path)? {
        true => {
            let output = Command::new("mount")
                .arg("-o")
                .arg("remount,ro")
                .arg("/boot")
                .stderr(Stdio::piped()) // Capture stderr for error handling
                .output();

            match output {
                Ok(output) => {
                    if output.status.success() {
                        Ok(())
                    } else {
                        let error_message = String::from_utf8_lossy(&output.stderr);
                        warn!("Failed to remount /boot as RO using shell: {error_message}");
                        Err(MountError::RemountFailed(error_message.to_string()))
                    }
                }
                Err(e) => {
                    warn!("Failed to execute mount command: {e}");
                    Err(MountError::RemountFailed(format!(
                        "Failed to execute mount: {e}"
                    )))
                }
            }
        }
        false => {
            info!("/boot is already read-only");
            Ok(())
        }
    }
}

#[cfg(not(feature = "test-remount"))]
fn remount_boot_rw_at(mounts_path: &Path) -> Result<(), MountError> {
    match is_boot_rw_at(mounts_path)? {
        false => {
            let output = Command::new("mount")
                .arg("-o")
                .arg("remount,rw")
                .arg("/boot")
                .stderr(Stdio::piped()) // Capture stderr for error handling
                .output();

            match output {
                Ok(output) => {
                    if output.status.success() {
                        Ok(())
                    } else {
                        let error_message = String::from_utf8_lossy(&output.stderr);
                        warn!("Failed to remount /boot as RW using shell: {error_message}");
                        Err(MountError::RemountFailed(error_message.to_string()))
                    }
                }
                Err(e) => {
                    warn!("Failed to execute mount command: {e}");
                    Err(MountError::RemountFailed(format!(
                        "Failed to execute mount: {e}"
                    )))
                }
            }
        }
        true => {
            info!("/boot is already read-write");
            Ok(())
        }
    }
}

/// Default helper: remount /boot RO using shared MOUNT_INFO_PATH
#[cfg(not(feature = "test-remount"))]
pub fn remount_boot_ro() -> Result<(), MountError> {
    remount_boot_ro_at(Path::new(MOUNT_INFO_PATH))
}

/// Default helper: remount /boot RW using shared MOUNT_INFO_PATH
#[cfg(not(feature = "test-remount"))]
pub fn remount_boot_rw() -> Result<(), MountError> {
    remount_boot_rw_at(Path::new(MOUNT_INFO_PATH))
}

/// For testing without actually remounting /mount
#[cfg(feature = "test-remount")]
fn remount_boot_rw_at(_mounts_path: &Path) -> Result<(), MountError> {
    Ok(())
}
/// For testing without actually remounting /mount
#[cfg(feature = "test-remount")]
fn remount_boot_ro_at(_mounts_path: &Path) -> Result<(), MountError> {
    Ok(())
}

/// For testing feature: default helpers no-op
#[cfg(feature = "test-remount")]
pub fn remount_boot_rw() -> Result<(), MountError> {
    Ok(())
}
#[cfg(feature = "test-remount")]
pub fn remount_boot_ro() -> Result<(), MountError> {
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    fn create_mock_file(content: &str) -> std::path::PathBuf {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mounts");
        std::fs::write(&path, content).unwrap();
        Box::leak(Box::new(dir));
        path
    }

    #[cfg(not(feature = "test-remount"))]
    #[test]
    fn test_remount_boot_ro_when_already_ro() {
        let mounts_content = "rootfs / rootfs rw 0 0\n\
                             none /boot tmpfs ro 0 0\n";
        let mounts_path = create_mock_file(mounts_content);

        let result = remount_boot_ro_at(&mounts_path);
        assert!(result.is_ok());
    }

    #[cfg(not(feature = "test-remount"))]
    #[test]
    fn test_remount_boot_rw_when_already_rw() {
        let mounts_content = "rootfs / rootfs rw 0 0\n\
                             none /boot tmpfs rw 0 0\n";
        let mounts_path = create_mock_file(mounts_content);

        let result = remount_boot_rw_at(&mounts_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_is_boot_rw_detection() {
        // Test RW case - should return true
        let rw_path = create_mock_file("device /boot ext4 rw,relatime 0 0");
        assert!(is_boot_rw_at(&rw_path).unwrap());

        // Test RO case - should return false
        let ro_path = create_mock_file("device /boot ext4 ro,relatime 0 0");
        assert!(!is_boot_rw_at(&ro_path).unwrap());

        // Test missing /boot - should error
        let missing_path = create_mock_file("device /other ext4 rw 0 0");
        assert!(is_boot_rw_at(&missing_path).is_err());

        // Test malformed line - should error
        let malformed_path = create_mock_file("incomplete fields");
        assert!(is_boot_rw_at(&malformed_path).is_err());
    }
}
