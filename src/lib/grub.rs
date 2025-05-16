use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;
use std::str;

use crate::mount::{remount_boot_ro, remount_boot_rw};

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

fn unset_grub_var(key: &str, grub_path: &str, mount_info_path: &str) -> Result<()> {
    remount_boot_rw(Path::new(mount_info_path)).context("Failed to remount /boot as rw")?;
    Command::new("grub2-editenv")
        .arg(grub_path)
        .arg("unset")
        .arg(key)
        .status()
        .context("Unable to clear boot_counter")?;
    log::info!("Clear grubenv: {key}");
    remount_boot_ro(Path::new(mount_info_path)).context("Failed to remount /boot as read-only")
}

fn set_grub_var(key: &str, val: u16, grub_path: &str, mount_info_path: &str) -> Result<()> {
    remount_boot_rw(Path::new(mount_info_path)).context("Failed to remount /boot as rw")?;
    Command::new("grub2-editenv")
        .arg(grub_path)
        .arg("set")
        .arg(format!("{key}={val}"))
        .status()
        .context("Unable to set grubenv")?;
    log::info!("Set grubenv: {key}={val}");
    remount_boot_ro(Path::new(mount_info_path)).context("Failed to remount /boot as read-only")
}

#[cfg(test)]
mod tests {
    use super::{get_boot_counter, set_boot_counter, unset_boot_counter};
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
}
