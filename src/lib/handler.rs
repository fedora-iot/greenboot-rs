// SPDX-License-Identifier: BSD-3-Clause

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use std::str;

use crate::grub::get_boot_counter;

/// Detects if the system is managed by bootc or is a rpm-ostree system.
/// First checks for `/run/ostree-booted`, then inspects `status.booted.image`
/// from `bootc status --booted --json` to distinguish between the two.
pub fn detect_os_deployment() -> Option<&'static str> {
    if !Path::new("/run/ostree-booted").exists() {
        log::info!("'/run/ostree-booted' not found, not an ostree-based system");
        return None;
    }

    let output = match Command::new("bootc")
        .args(["status", "--booted", "--json"])
        .output()
    {
        Ok(output) => output,
        Err(_) => return None,
    };

    if !output.status.success() {
        log::error!("'bootc status --booted --json' exited with non-zero status");
        return None;
    }

    let json: Value = match serde_json::from_slice::<Value>(&output.stdout) {
        Ok(json) => json,
        Err(_) => {
            log::error!("Failed to parse JSON from 'bootc status --booted --json'");
            return None;
        }
    };

    if let Some(image_type) = json
        .get("status")
        .and_then(|s| s.get("booted"))
        .and_then(|b| b.get("image"))
        .filter(|v| !v.is_null())
    {
        log::info!("System detected as bootc (status.booted.image: {image_type})");
        Some("bootc")
    } else {
        log::info!("System detected as rpm-ostree (status.booted.image is null or absent)");
        Some("rpm-ostree")
    }
}

/// Returns the deployment ID of the currently booted deployment, or None
/// if not on an ostree-based system or if the query fails.
pub fn get_booted_deployment_id() -> Option<String> {
    match detect_os_deployment() {
        Some("bootc") => get_bootc_deployment_id("booted"),
        Some("rpm-ostree") => get_rpm_ostree_deployment_id("booted"),
        _ => None,
    }
}

/// Returns the deployment ID of the staged (pending) deployment, or None
/// if not on an ostree-based system or if no deployment is staged.
pub fn get_staged_deployment_id() -> Option<String> {
    match detect_os_deployment() {
        Some("bootc") => get_bootc_deployment_id("staged"),
        Some("rpm-ostree") => get_rpm_ostree_deployment_id("staged"),
        _ => None,
    }
}

fn get_bootc_deployment_id(key: &str) -> Option<String> {
    let mut args = vec!["status", "--json"];
    if key == "booted" {
        args.insert(1, "--booted");
    }

    let output = Command::new("bootc").args(&args).output().ok()?;

    if !output.status.success() {
        log::warn!("Error parsing bootc status");
        return None;
    }

    let json: Value = serde_json::from_slice(&output.stdout).ok()?;

    json.get("status")
        .and_then(|s| s.get(key))
        .and_then(|d| d.get("image"))
        .and_then(|i| i.get("imageDigest"))
        .and_then(|d| d.as_str())
        .map(|s| s.to_string())
}

fn get_rpm_ostree_deployment_id(key: &str) -> Option<String> {
    let output = Command::new("rpm-ostree")
        .args(["status", "--json"])
        .output()
        .ok()?;

    if !output.status.success() {
        log::warn!("Error parsing rpm-ostree status");
        return None;
    }

    let json: Value = serde_json::from_slice(&output.stdout).ok()?;
    let deployments = json.get("deployments")?.as_array()?;

    for deployment in deployments {
        if deployment
            .get(key)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return deployment
                .get("checksum")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string());
        }
    }
    None
}

/// reboots the system if boot_counter is greater than 0 or can be forced too
pub fn handle_reboot(force: bool) -> Result<()> {
    if !force {
        let boot_counter = get_boot_counter()?;
        if boot_counter <= Some(0) {
            bail!("countdown ended, check greenboot-rollback status")
        };
    }
    log::info!("restarting the system");
    Command::new("systemctl").arg("reboot").status()?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackMode {
    /// GRUB kernel fallback detected — bypass boot_counter and force rollback immediately.
    Forced,
    /// Normal healthcheck-driven rollback — respect boot_counter guard.
    Normal,
}

/// `RollbackMode::Forced` bypasses the boot_counter check entirely
/// (used when GRUB kernel fallback is detected and rollback must be made permanent).
/// `RollbackMode::Normal` only proceeds if boot_counter has reached zero.
pub fn handle_rollback(mode: RollbackMode) -> Result<()> {
    if mode == RollbackMode::Normal {
        let boot_counter = get_boot_counter()?;

        match boot_counter {
            None => {
                bail!(
                    "System is unhealthy but boot_counter is not set, manual intervention required"
                )
            }
            Some(counter) if counter > 0 => {
                bail!("Rollback not initiated as boot_counter is {}", counter)
            }
            _ => {}
        }
    }

    log::info!("Greenboot will now attempt to rollback to a previous deployment.");
    if let Some(deployment_cmd) = detect_os_deployment() {
        log::info!("Deployment manager '{deployment_cmd}' detected, attempting rollback.");
        let status = Command::new(deployment_cmd)
            .arg("rollback")
            .status()
            .context(format!("Failed to execute '{deployment_cmd} rollback'"))?;

        if !status.success() {
            bail!(
                "Rollback with '{}' failed with status: {}",
                deployment_cmd,
                status
            );
        }
    } else {
        bail!("Rollback only supported in bootc or rpm-ostree environment.");
    }
    Ok(())
}

/// writes greenboot status to motd.d/boot-status
pub fn handle_motd(state: &str) -> Result<()> {
    std::fs::write("/etc/motd.d/boot-status", format!("{state}.").as_bytes())
        .map_err(|err| anyhow!("Error writing motd: {}", err))
}
