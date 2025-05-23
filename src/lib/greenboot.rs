use anyhow::{Result, bail};
use glob::glob;
use std::collections::HashSet;
use std::error::Error;
use std::path::Path;
use std::process::Command;

/// dir that greenboot looks for the health check and other scripts
static GREENBOOT_INSTALL_PATHS: [&str; 2] = ["/usr/lib/greenboot", "/etc/greenboot"];

/// runs all the scripts in required.d and wanted.d
pub fn run_diagnostics(skipped: Vec<String>) -> Result<Vec<String>> {
    let mut required_script_failure = false;
    let mut path_exists = false;
    let mut all_skipped = HashSet::new();

    // Convert input skipped Vec to HashSet for efficient lookups
    let disabled_scripts: HashSet<String> = skipped.clone().into_iter().collect();

    // Run required checks
    for path in GREENBOOT_INSTALL_PATHS {
        let greenboot_required_path = format!("{}/check/required.d/", path);
        if !Path::new(&greenboot_required_path).is_dir() {
            log::warn!("skipping test as {} is not a dir", greenboot_required_path);
            continue;
        }
        path_exists = true;
        let result = run_scripts("required", &greenboot_required_path, Some(&skipped));
        all_skipped.extend(result.skipped);

        if !result.errors.is_empty() {
            log::error!("required script error:");
            result.errors.iter().for_each(|e| log::error!("{e}"));
            required_script_failure = true;
        }
    }

    if !path_exists {
        bail!("cannot find any required.d folder");
    }

    // Run wanted checks
    for path in GREENBOOT_INSTALL_PATHS {
        let greenboot_wanted_path = format!("{}/check/wanted.d/", path);
        let result = run_scripts("wanted", &greenboot_wanted_path, Some(&skipped));
        all_skipped.extend(result.skipped);

        if !result.errors.is_empty() {
            log::warn!("wanted script runner error:");
            result.errors.iter().for_each(|e| log::error!("{e}"));
        }
    }

    // Check for disabled scripts that weren't found
    let missing_disabled: Vec<String> = disabled_scripts
        .difference(&all_skipped)
        .map(|s| s.to_string()) // Convert &String to String
        .collect();

    if !missing_disabled.is_empty() {
        log::warn!(
            "The following disabled scripts were not found in any directory: {:?}",
            missing_disabled
        );
    }

    if required_script_failure {
        bail!("health-check failed!");
    }
    Ok(missing_disabled)
}

// runs all the scripts in red.d when health-check fails
pub fn run_red() -> Vec<Box<dyn Error>> {
    let mut errors = Vec::new();

    for path in GREENBOOT_INSTALL_PATHS {
        let red_path = format!("{}/red.d/", path);
        let result = run_scripts("red", &red_path, None); // Pass None for disabled scripts
        errors.extend(result.errors);
    }

    errors
}

/// runs all the scripts green.d when health-check passes
pub fn run_green() -> Vec<Box<dyn Error>> {
    let mut errors = Vec::new();

    for path in GREENBOOT_INSTALL_PATHS {
        let green_path = format!("{}/green.d/", path);
        let result = run_scripts("green", &green_path, None); // Pass None for disabled scripts
        errors.extend(result.errors);
    }

    errors
}

struct ScriptRunResult {
    errors: Vec<Box<dyn Error>>,
    skipped: Vec<String>,
}

fn run_scripts(name: &str, path: &str, disabled_scripts: Option<&[String]>) -> ScriptRunResult {
    let mut result = ScriptRunResult {
        errors: Vec::new(),
        skipped: Vec::new(),
    };

    // Handle glob pattern error early
    let entries = match glob(&format!("{}*.sh", path)) {
        Ok(e) => e,
        Err(e) => {
            result.errors.push(Box::new(e));
            return result;
        }
    };

    for entry in entries.flatten() {
        // Process script name
        let script_name = match entry.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };

        // Check if script should be skipped
        if let Some(disabled) = disabled_scripts {
            if disabled.contains(&script_name.to_string()) {
                log::info!("Skipping disabled script: {}", script_name);
                result.skipped.push(script_name.to_string());
                continue;
            }
        }

        log::info!("running {} check {}", name, entry.to_string_lossy());

        // Execute script and handle output
        let output = Command::new("bash").arg("-C").arg(&entry).output();

        match output {
            Ok(o) if o.status.success() => {
                log::info!("{} script {} success!", name, entry.to_string_lossy());
            }
            Ok(o) => {
                let error_msg = format!(
                    "{} script {} failed!\n{}\n{}",
                    name,
                    entry.to_string_lossy(),
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                result.errors.push(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    error_msg,
                )));
            }
            Err(e) => {
                result.errors.push(Box::new(e));
            }
        }
    }

    result
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::{Context, Result};
    use std::fs;

    static GREENBOOT_INSTALL_PATHS: [&str; 2] = ["/usr/lib/greenboot", "/etc/greenboot"];

    /// validate when the required folder is not found
    #[test]
    fn missing_required_folder() {
        assert_eq!(
            run_diagnostics(vec![]).unwrap_err().to_string(),
            String::from("cannot find any required.d folder")
        );
    }

    #[test]
    fn test_passed_diagnostics() {
        setup_folder_structure(true)
            .context("Test setup failed")
            .unwrap();
        let state = run_diagnostics(vec![]);
        assert!(state.is_ok());
        tear_down().context("Test teardown failed").unwrap();
    }

    #[test]
    fn test_failed_diagnostics() {
        setup_folder_structure(false)
            .context("Test setup failed")
            .unwrap();
        let failed_msg = run_diagnostics(vec![]).unwrap_err().to_string();
        assert_eq!(failed_msg, String::from("health-check failed!"));
        tear_down().context("Test teardown failed").unwrap();
    }

    #[test]
    fn test_skip_nonexistent_script() {
        let nonexistent_script_name = "nonexistent_script.sh".to_string();
        setup_folder_structure(true)
            .context("Test setup failed")
            .unwrap();

        // Try to skip a script that doesn't exist
        let state = run_diagnostics(vec![nonexistent_script_name.clone()]);
        assert!(
            state.unwrap().contains(&nonexistent_script_name),
            "non existent script names did not match"
        );

        tear_down().context("Test teardown failed").unwrap();
    }

    #[test]
    fn test_skip_failing_script() {
        setup_folder_structure(false)
            .context("Test setup failed")
            .unwrap();

        // Skip the failing script in required.d
        let state = run_diagnostics(vec!["failing_script.sh".to_string()]);
        assert!(
            state.is_ok(),
            "Should pass when skipping failing required script"
        );

        tear_down().context("Test teardown failed").unwrap();
    }

    fn setup_folder_structure(passing: bool) -> Result<()> {
        let required_path = format!("{}/check/required.d", GREENBOOT_INSTALL_PATHS[1]);
        let wanted_path = format!("{}/check/wanted.d", GREENBOOT_INSTALL_PATHS[1]);
        let passing_test_scripts = "testing_assets/passing_script.sh";
        let failing_test_scripts = "testing_assets/failing_script.sh";

        fs::create_dir_all(&required_path).expect("cannot create folder");
        fs::create_dir_all(&wanted_path).expect("cannot create folder");

        // Create passing script in both required and wanted
        fs::copy(
            passing_test_scripts,
            format!("{}/passing_script.sh", &required_path),
        )
        .context("unable to copy passing script to required.d")?;

        fs::copy(
            passing_test_scripts,
            format!("{}/passing_script.sh", &wanted_path),
        )
        .context("unable to copy passing script to wanted.d")?;

        // Create failing script in wanted.d
        fs::copy(
            failing_test_scripts,
            format!("{}/failing_script.sh", &wanted_path),
        )
        .context("unable to copy failing script to wanted.d")?;

        if !passing {
            // Create failing script in required.d for failure cases
            fs::copy(
                failing_test_scripts,
                format!("{}/failing_script.sh", &required_path),
            )
            .context("unable to copy failing script to required.d")?;
        }
        Ok(())
    }

    fn tear_down() -> Result<()> {
        fs::remove_dir_all(GREENBOOT_INSTALL_PATHS[1]).expect("Unable to delete folder");
        Ok(())
    }
}
