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

/// sets greenboot_next_deployment_id to the given deployment ID string
pub fn set_next_deployment_id(id: &str) -> Result<()> {
    set_next_deployment_id_at(id, GRUB_PATH)
}

fn set_next_deployment_id_at(id: &str, grub_path: &str) -> Result<()> {
    set_grub_str_var("greenboot_next_deployment_id", id, grub_path)
}

/// unsets greenboot_next_deployment_id
pub fn unset_next_deployment_id() -> Result<()> {
    unset_next_deployment_id_at(GRUB_PATH)
}

fn unset_next_deployment_id_at(grub_path: &str) -> Result<()> {
    unset_grub_var("greenboot_next_deployment_id", grub_path)
}

/// returns the stored deployment ID, or None if not set
pub fn get_next_deployment_id() -> Result<Option<String>> {
    get_next_deployment_id_at(GRUB_PATH)
}

fn get_next_deployment_id_at(grub_path: &str) -> Result<Option<String>> {
    get_grub_str_var("greenboot_next_deployment_id", grub_path)
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

fn set_grub_str_var(key: &str, val: &str, grub_path: &str) -> Result<()> {
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

fn get_grub_str_var(key: &str, grub_path: &str) -> Result<Option<String>> {
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
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            return Ok(Some(trimmed.to_string()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::{
        get_boot_counter_at, get_fallback_at, get_next_deployment_id_at, set_boot_counter_at,
        set_fallback_at, set_next_deployment_id_at, unset_boot_counter_at, unset_fallback_at,
        unset_next_deployment_id_at,
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
    fn test_next_deployment_id_set_and_get() {
        let (_temp_dir, grubenv) = setup_test_paths();

        assert_eq!(get_next_deployment_id_at(&grubenv).unwrap(), None);

        let id = "sha256:abc123def456";
        set_next_deployment_id_at(id, &grubenv).unwrap();
        assert_eq!(
            get_next_deployment_id_at(&grubenv).unwrap(),
            Some(id.to_string())
        );
    }

    #[test]
    fn test_next_deployment_id_unset() {
        let (_temp_dir, grubenv) = setup_test_paths();

        let id = "sha256:abc123def456";
        set_next_deployment_id_at(id, &grubenv).unwrap();
        assert!(get_next_deployment_id_at(&grubenv).unwrap().is_some());

        unset_next_deployment_id_at(&grubenv).unwrap();
        assert_eq!(get_next_deployment_id_at(&grubenv).unwrap(), None);
    }

    #[test]
    fn test_next_deployment_id_coexists_with_boot_counter() {
        let (_temp_dir, grubenv) = setup_test_paths();

        let id = "sha256:abc123def456";
        set_boot_counter_at(3, &grubenv).unwrap();
        set_next_deployment_id_at(id, &grubenv).unwrap();

        assert_eq!(get_boot_counter_at(&grubenv).unwrap(), Some(3));
        assert_eq!(
            get_next_deployment_id_at(&grubenv).unwrap(),
            Some(id.to_string())
        );

        unset_next_deployment_id_at(&grubenv).unwrap();
        assert_eq!(get_boot_counter_at(&grubenv).unwrap(), Some(3));
        assert_eq!(get_next_deployment_id_at(&grubenv).unwrap(), None);
    }

    #[test]
    fn test_next_deployment_id_coexists_with_fallback() {
        let (_temp_dir, grubenv) = setup_test_paths();

        let id = "sha256:abc123def456";
        set_next_deployment_id_at(id, &grubenv).unwrap();
        set_fallback_at(&grubenv).unwrap();

        assert_eq!(
            get_next_deployment_id_at(&grubenv).unwrap(),
            Some(id.to_string())
        );
        assert!(get_fallback_at(&grubenv).unwrap());

        unset_next_deployment_id_at(&grubenv).unwrap();
        assert_eq!(get_next_deployment_id_at(&grubenv).unwrap(), None);
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
    fn test_fallback_coexists_with_boot_counter() {
        let (_temp_dir, grubenv) = setup_test_paths();

        set_boot_counter_at(3, &grubenv).unwrap();
        set_fallback_at(&grubenv).unwrap();

        assert_eq!(get_boot_counter_at(&grubenv).unwrap(), Some(3));
        assert!(get_fallback_at(&grubenv).unwrap());
    }

    #[test]
    fn test_fallback_independent_of_deployment_id() {
        let (_temp_dir, grubenv) = setup_test_paths();

        let id = "sha256:abc123def456";
        set_next_deployment_id_at(id, &grubenv).unwrap();
        set_fallback_at(&grubenv).unwrap();

        unset_next_deployment_id_at(&grubenv).unwrap();
        assert_eq!(get_next_deployment_id_at(&grubenv).unwrap(), None);
        assert!(get_fallback_at(&grubenv).unwrap());

        unset_fallback_at(&grubenv).unwrap();
        assert!(!get_fallback_at(&grubenv).unwrap());
    }
}
