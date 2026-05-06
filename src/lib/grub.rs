// SPDX-License-Identifier: BSD-3-Clause

use anyhow::{Context, Result, bail};
use std::process::Command;
use std::str;

/// Shared GRUB environment path used by default helpers
static GRUB_PATH: &str = "/boot/grub2/grubenv";

/// fetches boot_counter value, none if not set
pub fn get_boot_counter() -> Result<Option<i32>> {
    get_boot_counter_at(GRUB_PATH)
}

fn get_boot_counter_at(grub_path: &str) -> Result<Option<i32>> {
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
pub fn set_boot_counter(reboot_count: u16) -> Result<()> {
    set_boot_counter_at(reboot_count, GRUB_PATH)
}

fn set_boot_counter_at(reboot_count: u16, grub_path: &str) -> Result<()> {
    match get_boot_counter_at(grub_path) {
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
    set_grub_var("boot_counter", reboot_count, grub_path)?;
    Ok(())
}
/// sets grub variable boot_success
pub fn set_boot_status(success: bool) -> Result<()> {
    set_boot_status_at(success, GRUB_PATH)
}

fn set_boot_status_at(success: bool, grub_path: &str) -> Result<()> {
    if success {
        set_grub_var("boot_success", 1, grub_path)?;
        unset_boot_counter_at(grub_path)?;
        return Ok(());
    }
    set_grub_var("boot_success", 0, grub_path)
}

/// unset boot_counter
pub fn unset_boot_counter() -> Result<()> {
    unset_boot_counter_at(GRUB_PATH)
}

fn unset_boot_counter_at(grub_path: &str) -> Result<()> {
    unset_grub_var("boot_counter", grub_path)
}

/// sets greenboot_rollback_trigger=1 and fallback=1
pub fn set_rollback_trigger() -> Result<()> {
    set_rollback_trigger_at(GRUB_PATH)
}

fn set_rollback_trigger_at(grub_path: &str) -> Result<()> {
    set_grub_var("greenboot_rollback_trigger", 1, grub_path)?;
    set_grub_var("fallback", 1, grub_path)
}

/// unsets greenboot_rollback_trigger
pub fn unset_rollback_trigger() -> Result<()> {
    unset_rollback_trigger_at(GRUB_PATH)
}

fn unset_rollback_trigger_at(grub_path: &str) -> Result<()> {
    unset_grub_var("greenboot_rollback_trigger", grub_path)
}

/// sets fallback=1 for GRUB-level kernel fallback protection
pub fn set_fallback() -> Result<()> {
    set_fallback_at(GRUB_PATH)
}

fn set_fallback_at(grub_path: &str) -> Result<()> {
    set_grub_var("fallback", 1, grub_path)
}

/// unsets the fallback grub variable
pub fn unset_fallback() -> Result<()> {
    unset_fallback_at(GRUB_PATH)
}

fn unset_fallback_at(grub_path: &str) -> Result<()> {
    unset_grub_var("fallback", grub_path)
}

/// gets fallback value, returns true if set to 1
pub fn get_fallback() -> Result<bool> {
    get_fallback_at(GRUB_PATH)
}

fn get_fallback_at(grub_path: &str) -> Result<bool> {
    get_grub_bool_var("fallback", grub_path)
}

/// gets greenboot_rollback_trigger value, returns true if set to 1
pub fn get_rollback_trigger() -> Result<bool> {
    get_rollback_trigger_at(GRUB_PATH)
}

fn get_rollback_trigger_at(grub_path: &str) -> Result<bool> {
    get_grub_bool_var("greenboot_rollback_trigger", grub_path)
}

fn get_grub_bool_var(key: &str, grub_path: &str) -> Result<bool> {
    let grub_vars = Command::new("grub2-editenv")
        .arg(grub_path)
        .arg("list")
        .output()
        .context(format!("Unable to list grubenv variables for key: {key}"))?;

    if !grub_vars.status.success() {
        bail!(
            "grub2-editenv failed to list variables: {}",
            String::from_utf8_lossy(&grub_vars.stderr)
        );
    }

    let prefix = format!("{key}=");
    let output = String::from_utf8_lossy(&grub_vars.stdout);
    for line in output.lines() {
        if let Some(value) = line.strip_prefix(&prefix) {
            return Ok(value == "1");
        }
    }
    Ok(false)
}

fn unset_grub_var(key: &str, grub_path: &str) -> Result<()> {
    let grub_result = Command::new("grub2-editenv")
        .arg(grub_path)
        .arg("unset")
        .arg(key)
        .status()
        .context(format!("Unable to unset grubenv key: {key}"))?;

    if !grub_result.success() {
        bail!("Failed to unset grubenv key: {key}");
    }

    log::info!("Clear grubenv: {key}");
    Ok(())
}

fn set_grub_var(key: &str, val: u16, grub_path: &str) -> Result<()> {
    // Execute GRUB command and capture result
    let grub_result = Command::new("grub2-editenv")
        .arg(grub_path)
        .arg("set")
        .arg(format!("{key}={val}"))
        .status()
        .context("Unable to set grubenv")?;

    if !grub_result.success() {
        bail!("Failed to set grubenv key: {key}");
    }

    log::info!("Set grubenv: {key}={val}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        get_boot_counter_at, get_fallback_at, get_rollback_trigger_at, set_boot_counter_at,
        set_fallback_at, set_rollback_trigger_at, unset_boot_counter_at, unset_fallback_at,
        unset_rollback_trigger_at,
    };
    use anyhow::Context;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;
    use tempfile::tempdir;

    fn setup_test_paths() -> (TempDir, String) {
        let temp_dir = tempdir().unwrap();
        let temp_grubenv = temp_dir.path().join("grubenv");
        fs::copy("testing_assets/grubenv", &temp_grubenv).unwrap();
        (temp_dir, temp_grubenv.to_str().unwrap().to_string())
    }

    #[test]
    fn test_boot_counter_set() {
        let (_temp_dir, grubenv) = setup_test_paths();
        set_boot_counter_at(10, &grubenv).unwrap();
        assert_eq!(get_boot_counter_at(&grubenv).unwrap(), Some(10));
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
        set_boot_counter_at(20, &grubenv).ok();
        assert_eq!(get_boot_counter_at(&grubenv).unwrap(), Some(99));
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
        set_boot_counter_at(13, &grubenv).unwrap();
        assert_eq!(get_boot_counter_at(&grubenv).unwrap(), Some(13));
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
        unset_boot_counter_at(&grubenv).unwrap();
        assert_eq!(get_boot_counter_at(&grubenv).unwrap(), None);
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
        assert_eq!(get_boot_counter_at(&grubenv).unwrap(), Some(99));
    }

    #[test]
    fn test_rollback_trigger_functions() {
        let (_temp_dir, grubenv) = setup_test_paths();

        assert!(!get_rollback_trigger_at(&grubenv).unwrap());
        assert!(!get_fallback_at(&grubenv).unwrap());

        set_rollback_trigger_at(&grubenv).unwrap();
        assert!(get_rollback_trigger_at(&grubenv).unwrap());
        assert!(get_fallback_at(&grubenv).unwrap());

        // unset_rollback_trigger only clears the trigger, not fallback
        unset_rollback_trigger_at(&grubenv).unwrap();
        assert!(!get_rollback_trigger_at(&grubenv).unwrap());
        assert!(get_fallback_at(&grubenv).unwrap());

        // fallback has its own lifecycle, unset independently
        unset_fallback_at(&grubenv).unwrap();
        assert!(!get_fallback_at(&grubenv).unwrap());
    }

    #[test]
    fn test_rollback_trigger_with_other_vars() {
        let (_temp_dir, grubenv) = setup_test_paths();

        set_boot_counter_at(3, &grubenv).unwrap();
        set_rollback_trigger_at(&grubenv).unwrap();

        assert_eq!(get_boot_counter_at(&grubenv).unwrap(), Some(3));
        assert!(get_rollback_trigger_at(&grubenv).unwrap());
        assert!(get_fallback_at(&grubenv).unwrap());

        // unset_rollback_trigger leaves boot_counter and fallback intact
        unset_rollback_trigger_at(&grubenv).unwrap();
        assert_eq!(get_boot_counter_at(&grubenv).unwrap(), Some(3));
        assert!(!get_rollback_trigger_at(&grubenv).unwrap());
        assert!(get_fallback_at(&grubenv).unwrap());
    }

    #[test]
    fn test_fallback_set_and_get() {
        let (_temp_dir, grubenv) = setup_test_paths();

        assert!(!get_fallback_at(&grubenv).unwrap());

        set_fallback_at(&grubenv).unwrap();
        assert!(get_fallback_at(&grubenv).unwrap());
    }

    #[test]
    fn test_fallback_unset() {
        let (_temp_dir, grubenv) = setup_test_paths();

        set_fallback_at(&grubenv).unwrap();
        assert!(get_fallback_at(&grubenv).unwrap());

        unset_fallback_at(&grubenv).unwrap();
        assert!(!get_fallback_at(&grubenv).unwrap());
    }

    #[test]
    fn test_fallback_unset_when_not_set() {
        let (_temp_dir, grubenv) = setup_test_paths();

        unset_fallback_at(&grubenv).unwrap();
        assert!(!get_fallback_at(&grubenv).unwrap());
    }

    #[test]
    fn test_fallback_set_via_rollback_trigger() {
        let (_temp_dir, grubenv) = setup_test_paths();

        set_rollback_trigger_at(&grubenv).unwrap();
        assert!(get_fallback_at(&grubenv).unwrap());
        assert!(get_rollback_trigger_at(&grubenv).unwrap());
    }

    #[test]
    fn test_fallback_coexists_with_boot_counter() {
        let (_temp_dir, grubenv) = setup_test_paths();

        set_boot_counter_at(3, &grubenv).unwrap();
        set_rollback_trigger_at(&grubenv).unwrap();

        assert_eq!(get_boot_counter_at(&grubenv).unwrap(), Some(3));
        assert!(get_fallback_at(&grubenv).unwrap());
        assert!(get_rollback_trigger_at(&grubenv).unwrap());
    }

    #[test]
    fn test_fallback_independent_unset() {
        let (_temp_dir, grubenv) = setup_test_paths();

        set_rollback_trigger_at(&grubenv).unwrap();
        assert!(get_fallback_at(&grubenv).unwrap());

        unset_fallback_at(&grubenv).unwrap();
        assert!(!get_fallback_at(&grubenv).unwrap());
        assert!(get_rollback_trigger_at(&grubenv).unwrap());
    }
}
