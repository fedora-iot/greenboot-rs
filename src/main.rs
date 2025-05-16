use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use config::{Config, File, FileFormat};
use greenboot::{
    handle_motd, handle_reboot, handle_rollback, run_diagnostics, run_green, run_red,
    set_boot_counter, set_boot_status, unset_boot_counter,
};
use serde::Deserialize;
use std::process::Command;

/// greenboot config path
static GREENBOOT_CONFIG_FILE: &str = "/etc/greenboot/greenboot.conf";
static GRUB_PATH: &str = "/boot/grub2/grubenv";
static MOUNT_INFO_PATH: &str = "/proc/mounts";

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
#[derive(Debug, Deserialize)]
/// config params for greenboot
struct GreenbootConfig {
    max_reboot: u16,
}

impl GreenbootConfig {
    /// sets the default parameter for greenboot config
    fn set_default() -> Self {
        Self { max_reboot: 3 }
    }
    /// gets the config from the config file
    fn get_config() -> Self {
        let mut config = Self::set_default();
        let parsed = Config::builder()
            .add_source(File::new(GREENBOOT_CONFIG_FILE, FileFormat::Ini))
            .build();
        match parsed {
            Ok(c) => {
                config.max_reboot = match c.get_int("GREENBOOT_MAX_BOOT_ATTEMPTS") {
                    Ok(c) => c.try_into().unwrap_or_else(|e| {
                        log::warn!(
                            "{e}, config error, using default value: {}",
                            config.max_reboot
                        );
                        config.max_reboot
                    }),
                    Err(e) => {
                        log::warn!(
                            "{e}, config error, using default value: {}",
                            config.max_reboot
                        );
                        config.max_reboot
                    }
                }
            }
            Err(e) => log::warn!(
                "{e}, config error, using default value: {}",
                config.max_reboot
            ),
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
/// greenboot rollback -> if boot_counter satisfies it trigger rollback
enum Commands {
    HealthCheck,
    Rollback,
}

/// Check if greenboot-rollback.service successfully ran in the previous boot
fn check_previous_rollback() -> Result<bool> {
    log::debug!("Checking journalctl for previous rollback attempts...");

    let output = Command::new("journalctl")
        .arg("-b")
        .arg("-1")
        .arg("-u")
        .arg("greenboot-rollback.service")
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

    log::debug!("Rollback detection result: {}", success);
    Ok(success)
}

/// Generate appropriate MOTD message with optional fallback prefix
/// Generate MOTD message using pre-checked rollback status
fn generate_motd_message(base_msg: &str, previous_rollback: bool) -> Result<String> {
    let prefix = if previous_rollback {
        "FALLBACK BOOT DETECTED! Default bootc deployment has been rolled back.\n"
    } else {
        ""
    };
    Ok(format!("{prefix}{base_msg}"))
}

/// triggers the diagnostics followed by the action on the outcome
/// this also handles setting the grub variables and system restart
fn health_check() -> Result<()> {
    let config = GreenbootConfig::get_config();
    log::debug!("{config:?}");

    // Check rollback status with graceful error handling
    let previous_rollback = match check_previous_rollback() {
        Ok(status) => {
            if status {
                log::info!(
                    "FALLBACK BOOT DETECTED! Default bootc deployment has been rolled back."
                );
            }
            status
        }
        Err(e) => {
            log::warn!(
                "Failed to check previous rollback status: {}. Defaulting to false.",
                e
            );
            false
        }
    };

    // Rest of the function remains the same...
    handle_motd(&generate_motd_message(
        "Greenboot healthcheck is in progress",
        previous_rollback,
    )?)?;

    match run_diagnostics() {
        Ok(()) => {
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
            .unwrap_or_else(|e| log::error!("cannot set motd: {}", e));
            set_boot_status(true, GRUB_PATH, MOUNT_INFO_PATH)?;
            Ok(())
        }
        Err(e) => {
            log::error!("Greenboot error: {e}");

            handle_motd(&generate_motd_message(
                "Greenboot healthcheck failed - status is RED",
                previous_rollback,
            )?)
            .unwrap_or_else(|e| log::error!("cannot set motd: {}", e));
            let errors = run_red();
            if !errors.is_empty() {
                log::error!("There is a problem with red script runner");
                errors.iter().for_each(|e| log::error!("{e}"));
            }

            set_boot_status(false, GRUB_PATH, MOUNT_INFO_PATH)
                .unwrap_or_else(|e| log::error!("cannot set boot_status: {}", e));
            set_boot_counter(config.max_reboot, GRUB_PATH, MOUNT_INFO_PATH)
                .unwrap_or_else(|e| log::error!("cannot set boot_counter: {}", e));
            handle_reboot(false).unwrap_or_else(|e| log::error!("cannot reboot: {}", e));
            bail!("greenboot healthcheck failed")
        }
    }
}

/// initiates rollback if boot_counter and satisfies
fn trigger_rollback() -> Result<()> {
    match handle_rollback() {
        Ok(()) => {
            log::info!("Rollback successful");
            unset_boot_counter(GRUB_PATH, MOUNT_INFO_PATH)?;
            handle_reboot(true)
        }
        Err(e) => {
            bail!("{e}, Rollback is not initiated");
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    pretty_env_logger::formatted_builder()
        .filter_level(cli.log_level.to_log())
        .init();

    match cli.command {
        Commands::HealthCheck => health_check(),
        Commands::Rollback => trigger_rollback(),
    }
}
