// SPDX-License-Identifier: BSD-3-Clause

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use std::str;

use crate::grub::get_boot_counter;

/// Detects if the system is managed by bootc or is a rpm-ostree system
fn detect_os_deployment() -> Option<&'static str> {
    // 1. Check if this is a bootc-managed host.
    if let Ok(output) = Command::new("bootc")
        .args(["status", "--booted", "--json"])
        .output()
        && output.status.success()
        && let Ok(json) = serde_json::from_slice::<Value>(&output.stdout)
        && json.get("kind").and_then(|v| v.as_str()) == Some("BootcHost")
    {
        log::info!("System detected as bootc-managed host.");
        return Some("bootc");
    }

    // 2. If not bootc, check if it's an ostree-based OS by looking for /run/ostree-booted.
    if Path::new("/run/ostree-booted").exists() {
        log::info!("System detected as ostree-based (via /run/ostree-booted).");
        return Some("rpm-ostree");
    }

    // 3. If neither check passes, the deployment type is unsupported.
    log::warn!("System is neither bootc nor a known ostree variant.");
    None
}

/// reboots the system if boot_counter is greater than 0 or can be forced too
pub fn handle_reboot(force: bool) -> Result<()> {
    if !force {
        let boot_counter = get_boot_counter("/boot/grub2/grubenv")?;
        if boot_counter <= Some(0) {
            bail!("countdown ended, check greenboot-rollback status")
        };
    }
    log::info!("restarting the system");
    Command::new("systemctl").arg("reboot").status()?;
    Ok(())
}

/// Rollback to the previous deployment if the boot counter allows.
pub fn handle_rollback() -> Result<()> {
    let boot_counter = get_boot_counter("/boot/grub2/grubenv")?;

    match boot_counter {
        // Exit early if boot_counter is not set
        None => {
            bail!("System is unhealthy but boot_counter is not set, manual intervention required")
        }
        // Proceed with rollback if boot_counter is <= 0
        Some(counter) if counter <= 0 => {
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
        // Reject if boot_counter is > 0
        Some(counter) => bail!("Rollback not initiated as boot_counter is {}", counter),
    }
}

/// writes greenboot status to motd.d/boot-status
pub fn handle_motd(state: &str) -> Result<()> {
    std::fs::write("/etc/motd.d/boot-status", format!("{state}.").as_bytes())
        .map_err(|err| anyhow!("Error writing motd: {}", err))
}
