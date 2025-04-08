use log::{info, warn};
use nix::mount::{mount, MsFlags};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;

static BOOT_WAS_RO: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Error)]
pub enum MountError {
    #[error("Failed to remount /boot: {0}")]
    RemountFailed(String),
    #[error("Failed to read mount info")]
    MountInfoError,
}

fn is_boot_rw() -> Result<bool, MountError> {
    let mounts = fs::read_to_string("/proc/mounts").map_err(|_| MountError::MountInfoError)?;

    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.get(1) == Some(&"/boot") {
            let options = parts.get(3).unwrap_or(&"");
            return Ok(options.contains("rw") && !options.contains("ro"));
        }
    }
    Err(MountError::MountInfoError)
}

pub fn remount_boot_ro() -> Result<(), MountError> {
    match is_boot_rw()? {
        true => {
            info!("Remounting /boot as read-only");
            mount(
                None::<&str>,
                Path::new("/boot"),
                None::<&str>,
                MsFlags::MS_REMOUNT | MsFlags::MS_RDONLY,
                None::<&str>,
            )
            .map_err(|e| {
                warn!("Failed to remount /boot as RO: {}", e);
                MountError::RemountFailed(e.to_string())
            })?;
            BOOT_WAS_RO.store(true, Ordering::SeqCst);
            Ok(())
        }
        false => {
            info!("/boot is already read-only");
            Ok(())
        }
    }
}

pub fn remount_boot_rw() -> Result<(), MountError> {
    match is_boot_rw()? {
        false => {
            info!("Remounting /boot as read-write");
            mount(
                None::<&str>,
                Path::new("/boot"),
                None::<&str>,
                MsFlags::MS_REMOUNT | MsFlags::MS_BIND,
                None::<&str>,
            )
            .map_err(|e| {
                warn!("Failed to remount /boot as RW: {}", e);
                MountError::RemountFailed(e.to_string())
            })?;
            BOOT_WAS_RO.store(true, Ordering::SeqCst);
            Ok(())
        }
        true => {
            info!("/boot is already read-write");
            Ok(())
        }
    }
}
