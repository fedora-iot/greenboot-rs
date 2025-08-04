// SPDX-License-Identifier: BSD-3-Clause

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;
use std::str;

use crate::mount::{is_boot_rw, remount_boot_ro, remount_boot_rw};

/// fetches boot_counter value, none if not set
pub fn get_boot_counter(grub_path: &str) -> Result<Option<i32>> {
    let grub_vars = Command::new("grub2-editenv")
        .arg(grub_path)
        .arg("list")
        .output()?;
    let grub_vars = str::from_utf8(&grub_vars.stdout[..])?;
    for var in grub_vars.lines() {
        let (k, v) = if let Some(kv) = var.split_once('=') {
            kv
        } else {
            continue;
        };
        if k != "boot_counter" {
            continue;
        }

        return match v.parse::<i32>() {
            Ok(n) => Ok(Some(n)),
            Err(_) => Err(anyhow::anyhow!("boot_counter has invalid value: {}", v)),
        };
    }
    Ok(None)
}

/// sets grub variable boot_counter if not set
pub fn set_boot_counter(reboot_count: u16, grub_path: &str, mount_info_path: &str) -> Result<()> {
    match get_boot_counter(grub_path) {
        Ok(Some(i)) => {
            bail!("already set boot_counter={i}");
        }
        Ok(None) => {
            log::info!("boot_counter does not exists");
        }
        Err(_) => {
            // Counter exists but has invalid value - overwrite it
            log::warn!("boot_counter exists with invalid value - overwriting");
        }
    }

    log::info!("setting boot counter");
    set_grub_var("boot_counter", reboot_count, grub_path, mount_info_path)?;
    Ok(())
}
/// sets grub variable boot_success
pub fn set_boot_status(success: bool, grub_path: &str, mount_info_path: &str) -> Result<()> {
    if success {
        set_grub_var("boot_success", 1, grub_path, mount_info_path)?;
        unset_boot_counter(grub_path, mount_info_path)?;
        return Ok(());
    }
    set_grub_var("boot_success", 0, grub_path, mount_info_path)
}

/// unset boot_counter
pub fn unset_boot_counter(grub_path: &str, mount_info_path: &str) -> Result<()> {
    unset_grub_var("boot_counter", grub_path, mount_info_path)
}

/// sets greenboot_rollback_trigger=1
pub fn set_rollback_trigger(grub_path: &str, mount_info_path: &str) -> Result<()> {
    set_grub_var("greenboot_rollback_trigger", 1, grub_path, mount_info_path)
}

/// unsets greenboot_rollback_trigger
pub fn unset_rollback_trigger(grub_path: &str, mount_info_path: &str) -> Result<()> {
    unset_grub_var("greenboot_rollback_trigger", grub_path, mount_info_path)
}

/// gets greenboot_rollback_trigger value, returns true if set to 1
pub fn get_rollback_trigger(grub_path: &str) -> Result<bool> {
    let grub_vars = Command::new("grub2-editenv")
        .arg(grub_path)
        .arg("list")
        .output()
        .context("Unable to list grubenv variables")?;

    let output = String::from_utf8_lossy(&grub_vars.stdout);
    for line in output.lines() {
        if line.starts_with("greenboot_rollback_trigger=") {
            let value = line.split('=').nth(1).unwrap_or("0");
            return Ok(value == "1");
        }
    }
    Ok(false) // Not set means false
}

fn unset_grub_var(key: &str, grub_path: &str, mount_info_path: &str) -> Result<()> {
    let was_rw = is_boot_rw(Path::new(mount_info_path))
        .map_err(|e| anyhow::anyhow!("Failed to check boot mount state: {}", e))?;

    if !was_rw {
        remount_boot_rw(Path::new(mount_info_path)).context("Failed to remount /boot as rw")?;
    }

    // Execute GRUB command and capture result
    let grub_result = Command::new("grub2-editenv")
        .arg(grub_path)
        .arg("unset")
        .arg(key)
        .status()
        .context("Unable to clear boot_counter");

    // Always attempt to restore mount state, regardless of GRUB command result
    if !was_rw {
        remount_boot_ro(Path::new(mount_info_path)).context("Failed to remount /boot as ro")?;
    }

    // Now check the GRUB command result
    grub_result?;
    log::info!("Clear grubenv: {key}");
    Ok(())
}

fn set_grub_var(key: &str, val: u16, grub_path: &str, mount_info_path: &str) -> Result<()> {
    let was_rw = is_boot_rw(Path::new(mount_info_path))
        .map_err(|e| anyhow::anyhow!("Failed to check boot mount state: {}", e))?;

    if !was_rw {
        remount_boot_rw(Path::new(mount_info_path)).context("Failed to remount /boot as rw")?;
    }

    // Execute GRUB command and capture result
    let grub_result = Command::new("grub2-editenv")
        .arg(grub_path)
        .arg("set")
        .arg(format!("{key}={val}"))
        .status()
        .context("Unable to set grubenv");

    // Always attempt to restore mount state, regardless of GRUB command result
    if !was_rw {
        remount_boot_ro(Path::new(mount_info_path))
            .context("Failed to remount /boot as read-only")?;
    }

    grub_result?;
    log::info!("Set grubenv: {key}={val}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        get_boot_counter, get_rollback_trigger, set_boot_counter, set_rollback_trigger,
        unset_boot_counter, unset_rollback_trigger,
    };
    use anyhow::Context;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;
    use tempfile::tempdir;

    static MOUNT_INFO_PATH: &str = "testing_assets/mounts";

    fn setup_test_paths() -> (TempDir, String) {
        let temp_dir = tempdir().unwrap();
        let temp_grubenv = temp_dir.path().join("grubenv");
        fs::copy("testing_assets/grubenv", &temp_grubenv).unwrap();
        (temp_dir, temp_grubenv.to_str().unwrap().to_string())
    }

    #[test]
    fn test_boot_counter_set() {
        let (_temp_dir, grubenv) = setup_test_paths();
        set_boot_counter(10, &grubenv, MOUNT_INFO_PATH).unwrap();
        assert_eq!(get_boot_counter(&grubenv).unwrap(), Some(10));
    }

    #[test]
    fn test_boot_counter_re_set() {
        let (_temp_dir, grubenv) = setup_test_paths();
        let _ = Command::new("grub2-editenv")
            .arg(&grubenv)
            .arg("set")
            .arg("boot_counter=99")
            .status()
            .context("Cannot create grub variable boot_counter");
        set_boot_counter(20, &grubenv, MOUNT_INFO_PATH).ok();
        assert_eq!(get_boot_counter(&grubenv).unwrap(), Some(99));
    }

    #[test]
    fn test_boot_counter_having_invalid_value() {
        let (_temp_dir, grubenv) = setup_test_paths();
        let _ = Command::new("grub2-editenv")
            .arg(&grubenv)
            .arg("set")
            .arg("boot_counter=foo")
            .status()
            .context("Cannot create grub variable boot_counter");
        set_boot_counter(13, &grubenv, MOUNT_INFO_PATH).unwrap();
        assert_eq!(get_boot_counter(&grubenv).unwrap(), Some(13));
    }

    #[test]
    fn test_unset_boot_counter() {
        let (_temp_dir, grubenv) = setup_test_paths();
        let _ = Command::new("grub2-editenv")
            .arg(&grubenv)
            .arg("set")
            .arg("boot_counter=199")
            .status()
            .context("Cannot create grub variable boot_counter");
        unset_boot_counter(&grubenv, MOUNT_INFO_PATH).unwrap();
        assert_eq!(get_boot_counter(&grubenv).unwrap(), None);
    }

    #[test]
    fn test_get_boot_counter() {
        let (_temp_dir, grubenv) = setup_test_paths();
        let _ = Command::new("grub2-editenv")
            .arg(&grubenv)
            .arg("set")
            .arg("boot_counter=99")
            .status()
            .context("Cannot create grub variable boot_counter");
        assert_eq!(get_boot_counter(&grubenv).unwrap(), Some(99));
    }

    #[test]
    fn test_rollback_trigger_functions() {
        let (_temp_dir, grubenv) = setup_test_paths();

        // Test when rollback trigger is not set
        assert!(!get_rollback_trigger(&grubenv).unwrap());

        // Test setting rollback trigger
        set_rollback_trigger(&grubenv, MOUNT_INFO_PATH).unwrap();
        assert!(get_rollback_trigger(&grubenv).unwrap());

        // Test unsetting rollback trigger
        unset_rollback_trigger(&grubenv, MOUNT_INFO_PATH).unwrap();
        assert!(!get_rollback_trigger(&grubenv).unwrap());
    }

    #[test]
    fn test_rollback_trigger_with_other_vars() {
        let (_temp_dir, grubenv) = setup_test_paths();

        // Set boot counter
        set_boot_counter(3, &grubenv, MOUNT_INFO_PATH).unwrap();

        // Set rollback trigger
        set_rollback_trigger(&grubenv, MOUNT_INFO_PATH).unwrap();

        // Both should coexist
        assert_eq!(get_boot_counter(&grubenv).unwrap(), Some(3));
        assert!(get_rollback_trigger(&grubenv).unwrap());

        // Unset rollback trigger, boot_counter should remain
        unset_rollback_trigger(&grubenv, MOUNT_INFO_PATH).unwrap();
        assert_eq!(get_boot_counter(&grubenv).unwrap(), Some(3));
        assert!(!get_rollback_trigger(&grubenv).unwrap());
    }
}
