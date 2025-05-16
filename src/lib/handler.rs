use anyhow::{Context, Result, anyhow, bail};
use std::process::Command;
use std::str;

use crate::grub::get_boot_counter;

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

/// rollback to previous deployment if boot counter is less than 0
pub fn handle_rollback() -> Result<()> {
    let boot_counter = get_boot_counter("/boot/grub2/grubenv")?;

    match boot_counter {
        // Exit early if boot_counter is not set
        None => {
            bail!("System is unhealthy but boot_counter is not set, manual intervention required")
        }
        // Proceed with rollback if boot_counter is <= 0
        Some(counter) if counter <= 0 => {
            log::info!("Greenboot will now attempt to rollback to previous deployment");
            let status = Command::new("bootc")
                .arg("rollback")
                .status()
                .context("Failed to execute bootc rollback")?;
            if !status.success() {
                bail!("Rollback error: {}", status);
            }
            Ok(())
        }
        // Reject if boot_counter is > 0 with the actual value
        Some(counter) => bail!("Rollback not initiated as boot_counter is {}", counter),
    }
}

/// writes greenboot status to motd.d/boot-status
pub fn handle_motd(state: &str) -> Result<()> {
    std::fs::write("/etc/motd.d/boot-status", format!("{state}.").as_bytes())
        .map_err(|err| anyhow!("Error writing motd: {}", err))
}
