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

// This function parses a string expected in bash-array format like
// `( "item1" "item2" ... )` into a Vec<String>.
fn parse_bash_array_string(raw_str: &str) -> Vec<String> {
    log::debug!("Attempting to parse raw bash-array string: '{}'", raw_str);

    if raw_str.starts_with('(') && raw_str.ends_with(')') {
        // Remove the outer parentheses
        let content = raw_str.trim_start_matches('(').trim_end_matches(')');

        // Split by whitespace, trim quotes from each part, and filter out empty strings
        let parsed_list: Vec<String> = content
            .split_whitespace()
            .map(|s| s.trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();

        log::debug!("Parsed list from bash-array string: {:?}", parsed_list);
        parsed_list
    } else if !raw_str.trim().is_empty() {
        // If the string is not empty but doesn't match the expected format,
        // log a warning and return an empty list.
        log::warn!(
            "String ('{}') is not in the expected bash-array format '( \"item1\" ... )'. Treating as empty list.",
            raw_str
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
        Commands::Rollback => trigger_rollback(),
    }
}
