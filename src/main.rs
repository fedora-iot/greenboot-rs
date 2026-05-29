// SPDX-License-Identifier: BSD-3-Clause

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use config::{Config, File, FileFormat};
use greenboot::{detect_os_deployment, get_booted_deployment_id, get_staged_deployment_id};
use greenboot::{
    get_boot_counter, get_fallback, get_next_deployment_id, handle_motd, handle_reboot,
    handle_rollback, run_diagnostics, run_green, run_red, set_boot_counter, set_boot_status,
    set_fallback, set_next_deployment_id, unset_boot_counter, unset_fallback,
    unset_next_deployment_id,
};
use greenboot::{is_boot_rw, remount_boot_ro, remount_boot_rw};
use std::{process::Command, sync::OnceLock};

/// greenboot config path
static GREENBOOT_CONFIG_FILE: &str = "/etc/greenboot/greenboot.conf";

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
/// cli parameters for greenboot
struct Cli {
    #[clap(value_enum, short, long, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
    #[clap(subcommand)]
    command: Commands,
}
#[derive(Debug)]
/// config params for greenboot
struct GreenbootConfig {
    max_reboot: u16,
    disabled_healthchecks: Vec<String>,
}

impl GreenbootConfig {
    pub fn get_config() -> Self {
        let mut config = Self {
            max_reboot: 3,                 // Default value
            disabled_healthchecks: vec![], //empty list
        };

        // Try to load from config file
        if let Ok(parsed_config) = Config::builder()
            .add_source(File::new(GREENBOOT_CONFIG_FILE, FileFormat::Ini))
            .build()
        {
            config.max_reboot = match parsed_config.get_int("GREENBOOT_MAX_BOOT_ATTEMPTS") {
                Ok(max) => max as u16,
                Err(_) => {
                    log::debug!(
                        "GREENBOOT_MAX_BOOT_ATTEMPTS not found in config using default value : 3"
                    );
                    3_u16
                }
            };

            config.disabled_healthchecks = match parsed_config.get_string("DISABLED_HEALTHCHECKS") {
                Ok(raw_disabled_str) => parse_bash_array_string(&raw_disabled_str),
                Err(_) => {
                    log::debug!(
                        "DISABLED_HEALTHCHECKS key not found in config, using default empty list."
                    );
                    vec![]
                }
            };
        }

        config
    }
}
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
/// log level for journald logging
enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Off,
}

impl LogLevel {
    fn to_log(self) -> log::LevelFilter {
        match self {
            LogLevel::Trace => log::LevelFilter::Trace,
            LogLevel::Debug => log::LevelFilter::Debug,
            LogLevel::Info => log::LevelFilter::Info,
            LogLevel::Warn => log::LevelFilter::Warn,
            LogLevel::Error => log::LevelFilter::Error,
            LogLevel::Off => log::LevelFilter::Off,
        }
    }
}

#[derive(Subcommand)]
/// params that greenboot accepts
///
/// greenboot health-check -> runs the custom health checks
///
/// greenboot set-rollback-trigger -> sets rollback trigger flag for next boot
enum Commands {
    HealthCheck,
    SetRollbackTrigger,
}

/// Determine if we're executing inside a containerized environment.
fn running_in_container() -> bool {
    static IS_CONTAINER: OnceLock<bool> = OnceLock::new();
    *IS_CONTAINER.get_or_init(|| {
        match Command::new("systemd-detect-virt")
            .arg("--container")
            .status()
        {
            Ok(status) => {
                if status.success() {
                    log::debug!("systemd-detect-virt detected container environment ({status})");
                    true
                } else {
                    log::debug!("systemd-detect-virt reported non-container context ({status})");
                    false
                }
            }
            Err(err) => {
                log::debug!("Unable to determine container state via systemd-detect-virt: {err}");
                false
            }
        }
    })
}

/// Execute a mutating GRUB operation while ensuring /boot is temporarily remounted RW if needed
fn with_boot_rw<F>(f: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    if running_in_container() {
        log::info!("Container environment detected; skipping /boot remounts");
        return f();
    }

    let was_rw =
        is_boot_rw().map_err(|e| anyhow::anyhow!("Failed to check boot mount state: {}", e))?;

    log::info!(
        "Initial /boot mount state: {}",
        if was_rw { "rw" } else { "ro" }
    );

    if !was_rw {
        log::info!("Remounting /boot as rw for operation");
        remount_boot_rw().context("Failed to remount /boot as rw")?;
    } else {
        log::info!("/boot is already rw; no remount needed");
    }

    let op_result = f();

    if !was_rw {
        log::info!("Restoring /boot mount to ro");
        remount_boot_ro().context("Failed to remount /boot as ro")?;
    }

    op_result
}

/// Check if greenboot-rollback.service successfully ran in the previous boot
fn check_previous_rollback() -> Result<bool> {
    log::debug!("Checking journalctl for previous rollback attempts...");

    let output = Command::new("journalctl")
        .arg("-b")
        .arg("-1")
        .arg("-u")
        .arg("greenboot-healthcheck.service")
        .arg("--no-pager")
        .output()
        .context("Failed to execute journalctl command to check rollback status")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::warn!(
            "journalctl command failed with status: {}. Error: {}",
            output.status,
            stderr.trim()
        );
        return Ok(false);
    }

    let journal_output =
        String::from_utf8(output.stdout).context("Failed to parse journalctl output as UTF-8")?;

    if journal_output.trim().is_empty() {
        log::debug!("No rollback service logs found in previous boot");
        return Ok(false);
    }

    // Check for specific success indicators
    let success = journal_output.contains("Rollback successful");

    log::debug!("Rollback detection result: {success}");
    Ok(success)
}

/// Detect GRUB-level kernel fallback by comparing the stored next_deployment_id
/// against the currently booted deployment. A mismatch means GRUB fell back to
/// the previous deployment (e.g. due to kernel panic on the new one).
fn detect_grub_fallback() -> bool {
    let expected_id = match get_next_deployment_id() {
        Ok(Some(id)) => id,
        _ => return false,
    };

    let booted_id = match get_booted_deployment_id() {
        Some(id) => id,
        None => {
            log::warn!("Could not determine booted deployment ID, skipping fallback detection");
            return false;
        }
    };

    if booted_id != expected_id {
        log::warn!(
            "GRUB kernel fallback detected: expected deployment {expected_id}, booted {booted_id}"
        );
        return true;
    }

    false
}

/// Generate appropriate MOTD message with optional fallback prefix
/// Generate MOTD message using pre-checked rollback status
fn generate_motd_message(base_msg: &str, previous_rollback: bool) -> Result<String> {
    let prefix = if previous_rollback {
        match detect_os_deployment() {
            Some(manager) => {
                format!(
                    "FALLBACK BOOT DETECTED! Default {manager} deployment has been rolled back.\n"
                )
            }
            None => String::from(""),
        }
    } else {
        String::from("")
    };
    Ok(format!("{prefix}{base_msg}"))
}

/// triggers the diagnostics followed by the action on the outcome
/// this also handles setting the grub variables and system restart
fn health_check() -> Result<()> {
    let config = GreenbootConfig::get_config();
    log::debug!("{config:?}");

    let container_mode = running_in_container();
    if container_mode {
        log::info!("Container environment detected; skipping reboot and rollback handling");
    }

    let mut previous_rollback = false;

    if !container_mode {
        // Detect GRUB-level kernel fallback via deployment ID comparison.
        // If mismatch, clear next_deployment_id immediately to prevent rollback loop.
        let grub_fallback = detect_grub_fallback();
        if grub_fallback {
            previous_rollback = true;
            log::info!("GRUB kernel fallback detected - new deployment failed to boot");
            log::info!(
                "Making GRUB fallback permanent by rolling back to the currently booted deployment"
            );
            match handle_rollback(true) {
                Ok(()) => {
                    log::info!("Rollback to previous deployment completed successfully");
                    with_boot_rw(unset_next_deployment_id)
                        .unwrap_or_else(|e| log::error!("Failed to clear next-deployment-id: {e}"));
                }
                Err(e) => log::error!("Failed to make GRUB fallback permanent: {e}"),
            }
        } else if check_previous_rollback().unwrap_or(false) {
            previous_rollback = true;
            match detect_os_deployment() {
                Some(manager) => log::info!(
                    "FALLBACK BOOT DETECTED! Default {manager} deployment has been rolled back."
                ),
                None => log::info!("FALLBACK BOOT DETECTED! Cannot determine the deployment type."),
            }
        }

        // Disarm GRUB fallback entry if the kernel booted successfully
        if get_fallback().unwrap_or(false) {
            log::info!("Kernel booted successfully, disarming GRUB fallback before healthchecks");
            with_boot_rw(unset_fallback).unwrap_or_else(|e: anyhow::Error| {
                log::error!("Failed to unset GRUB fallback: {e}")
            });
        }
    }

    handle_motd(&generate_motd_message(
        "Greenboot healthcheck is in progress",
        previous_rollback,
    )?)?;

    match run_diagnostics(config.disabled_healthchecks) {
        Ok(_) => {
            log::info!("greenboot health-check passed.");
            let errors = run_green();
            if !errors.is_empty() {
                log::error!("There is a problem with green script runner");
                errors.iter().for_each(|e| log::error!("{e}"));
            }

            handle_motd(&generate_motd_message(
                "Greenboot healthcheck passed - status is GREEN",
                previous_rollback,
            )?)
            .unwrap_or_else(|e| log::error!("cannot set motd: {e}"));

            if !container_mode {
                with_boot_rw(|| set_boot_status(true))?;
                // Unset next_deployment_id on successful health check
                if get_next_deployment_id().unwrap_or(None).is_some() {
                    with_boot_rw(unset_next_deployment_id)
                        .unwrap_or_else(|e| log::error!("Failed to unset next-deployment-id: {e}"));
                }
            }

            Ok(())
        }
        Err(e) => {
            log::error!("Greenboot error: {e}");

            handle_motd(&generate_motd_message(
                "Greenboot healthcheck failed - status is RED",
                previous_rollback,
            )?)
            .unwrap_or_else(|e| log::error!("cannot set motd: {e}"));
            let errors = run_red();
            if !errors.is_empty() {
                log::error!("There is a problem with red script runner");
                errors.iter().for_each(|e| log::error!("{e}"));
            }

            if !container_mode {
                with_boot_rw(|| set_boot_status(false))
                    .unwrap_or_else(|e| log::error!("cannot set boot_status: {e}"));

                // Check if boot_counter is 0 (exhausted retries) or if no counter is set
                match get_boot_counter()? {
                    Some(counter) if counter > 0 => {
                        // Still have retries left, just reboot
                        log::info!("Boot counter is {counter}, rebooting to try again");
                        handle_reboot(false).unwrap_or_else(|e| log::error!("cannot reboot: {e}"));
                    }
                    Some(_) => {
                        // Boot counter reached 0 (or negative) - check rollback trigger
                        if get_next_deployment_id().unwrap_or(None).is_some() {
                            log::info!(
                                "Boot counter exhausted and next-deployment-id is set - initiating rollback"
                            );
                            match handle_rollback(false) {
                                Ok(()) => {
                                    log::info!("Rollback successful");
                                    with_boot_rw(|| {
                                        unset_boot_counter()?;
                                        unset_next_deployment_id()?;
                                        Ok(())
                                    })
                                    .unwrap_or_else(|e| {
                                        log::error!("Failed to clear grub vars: {e}")
                                    });
                                    handle_reboot(true)
                                        .unwrap_or_else(|e| log::error!("cannot reboot: {e}"));
                                }
                                Err(rollback_err) => {
                                    log::error!("Rollback failed: {rollback_err}");
                                    bail!("Manual intervention required - rollback failed");
                                }
                            }
                        } else {
                            log::warn!(
                                "Boot counter exhausted but no next-deployment-id set - manual intervention required"
                            );
                            bail!("Manual intervention required - no rollback trigger");
                        }
                    }
                    None => {
                        // No boot counter set - this is the first failure, set it and reboot
                        log::info!(
                            "First health check failure, setting boot counter to {}",
                            config.max_reboot
                        );
                        with_boot_rw(|| set_boot_counter(config.max_reboot))
                            .unwrap_or_else(|e| log::error!("cannot set boot_counter: {e}"));
                        handle_reboot(false).unwrap_or_else(|e| log::error!("cannot reboot: {e}"));
                    }
                }
            }

            bail!("greenboot healthcheck failed")
        }
    }
}

// This function parses a string expected in bash-array format like
// `( "item1" "item2" ... )` into a Vec<String>.
fn parse_bash_array_string(raw_str: &str) -> Vec<String> {
    log::debug!("Attempting to parse raw bash-array string: '{raw_str}'");

    if raw_str.starts_with('(') && raw_str.ends_with(')') {
        // Remove the outer parentheses
        let content = raw_str.trim_start_matches('(').trim_end_matches(')');

        // Split by whitespace, trim quotes from each part, and filter out empty strings
        let parsed_list: Vec<String> = content
            .split_whitespace()
            .map(|s| s.trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();

        log::debug!("Parsed list from bash-array string: {parsed_list:?}");
        parsed_list
    } else if !raw_str.trim().is_empty() {
        // If the string is not empty but doesn't match the expected format,
        // log a warning and return an empty list.
        log::warn!(
            "String ('{raw_str}') is not in the expected bash-array format '( \"item1\" ... )'. Treating as empty list."
        );
        vec![]
    } else {
        // If the string is empty (e.g., "DISABLED_HEALTHCHECKS=" or "DISABLED_HEALTHCHECKS=()"),
        // it correctly results in an empty list.
        log::debug!("Bash-array string is empty or effectively empty, resulting in an empty list.");
        vec![]
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    pretty_env_logger::formatted_builder()
        .filter_level(cli.log_level.to_log())
        .init();

    match cli.command {
        Commands::HealthCheck => health_check(),
        Commands::SetRollbackTrigger => {
            if running_in_container() {
                log::info!("Container environment detected; skipping rollback trigger updates");
                return Ok(());
            }

            let deployment_id = get_staged_deployment_id().ok_or_else(|| {
                anyhow::anyhow!("Failed to get staged deployment ID - is an update staged?")
            })?;

            log::info!("Setting rollback trigger for deployment: {deployment_id}");
            with_boot_rw(|| {
                set_next_deployment_id(&deployment_id)?;
                set_fallback()
            })?;
            log::info!("Rollback trigger set successfully.");
            Ok(())
        }
    }
}
