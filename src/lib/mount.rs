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
    // Later entries are mounted on top of earlier ones (e.g. an automounted
    // /boot is listed as autofs first, then the real filesystem), so the
    // last /boot entry is the effective one.
    let mut boot_rw = None;
    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 && parts.get(1) == Some(&"/boot") {
            // Match whole options; "errors=remount-ro" or "minproto=5" are not "ro"
            boot_rw = Some(parts[3].split(',').any(|option| option == "rw"));
        }
    }
    boot_rw.ok_or(MountError::MountInfoError)
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
                .arg("remount,bind,ro")
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

    #[test]
    fn test_is_boot_rw_without_separate_boot() {
        // /boot on the root filesystem is bind-mounted into the ostree
        // deployment, so it shares its source device with /sysroot.
        // The ESP has the opposite state to show it is not taken for /boot.
        let rw_path = create_mock_file(
            "/dev/vda3 /sysroot xfs ro,relatime 0 0\n\
             /dev/vda3 /boot xfs rw,relatime 0 0\n\
             /dev/vda2 /boot/efi vfat ro,relatime 0 0\n",
        );
        assert!(is_boot_rw_at(&rw_path).unwrap());

        let ro_path = create_mock_file(
            "/dev/vda3 /sysroot xfs ro,relatime 0 0\n\
             /dev/vda3 /boot xfs ro,relatime 0 0\n\
             /dev/vda2 /boot/efi vfat rw,relatime 0 0\n",
        );
        assert!(!is_boot_rw_at(&ro_path).unwrap());

        // Only the ESP mounted, no /boot entry - /boot/efi must not be
        // matched as /boot, so this should error
        let esp_only_path = create_mock_file("/dev/vda2 /boot/efi vfat rw,relatime 0 0\n");
        assert!(is_boot_rw_at(&esp_only_path).is_err());
    }

    #[test]
    fn test_is_boot_rw_with_automounted_boot() {
        // gpt-auto mounts /boot on demand: an autofs entry (always rw) is
        // listed first and the real filesystem is mounted on top of it.
        let autofs = "systemd-1 /boot autofs rw,relatime,fd=57,pgrp=1,timeout=120,\
                      minproto=5,maxproto=5,direct,pipe_ino=3391 0 0\n";

        // Not accessed yet - only the autofs entry exists. This reports the
        // autofs entry's own rw flag, so remount_boot_rw() skips the remount
        // and the first write to /boot triggers the automount. That relies
        // on gpt-auto mounting the real filesystem read-write.
        let idle_path = create_mock_file(autofs);
        assert!(is_boot_rw_at(&idle_path).unwrap());

        let rw_path = create_mock_file(&format!(
            "{autofs}/dev/vda2 /boot vfat rw,relatime,errors=remount-ro 0 0\n"
        ));
        assert!(is_boot_rw_at(&rw_path).unwrap());

        let ro_path = create_mock_file(&format!(
            "{autofs}/dev/vda2 /boot vfat ro,relatime,errors=remount-ro 0 0\n"
        ));
        assert!(!is_boot_rw_at(&ro_path).unwrap());
    }
}
