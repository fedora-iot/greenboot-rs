use anyhow::{Result, bail};
use glob::glob;
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

/// dir that greenboot looks for the health check and other scripts
static GREENBOOT_INSTALL_PATHS: [&str; 2] = ["/usr/lib/greenboot", "/etc/greenboot"];

/// run required.d and wanted.d scripts.
/// If a required script fails, log the error, and skip remaining checks.
pub fn run_diagnostics(skipped: Vec<String>) -> Result<Vec<String>> {
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
            bail!("required health-check failed, skipping remaining scripts");
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

    let entries = match glob(&format!("{}*", path)) {
        Ok(e) => {
            let valid: Vec<_> = e
                .filter_map(Result::ok)
                .filter(|entry| {
                    if let Ok(metadata) = fs::metadata(entry) {
                        let mode = metadata.permissions().mode();
                        metadata.is_file()
                            && (entry.extension().and_then(|ext| ext.to_str()) == Some("sh")
                                || (mode & 0o001 != 0 || mode & 0o010 != 0 || mode & 0o100 != 0))
                    } else {
                        false
                    }
                })
                .collect();
            Some(valid).into_iter()
        }
        Err(e) => {
            result.errors.push(Box::new(e));
            return result;
        }
    };

    for entry in entries.flatten() {
        // Process script/binary name
        let file_name = match entry.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };

        // Check if script/binary should be skipped
        if let Some(disabled) = disabled_scripts {
            if disabled.contains(&file_name.to_string()) {
                log::info!("Skipping disabled script: {}", file_name);
                result.skipped.push(file_name.to_string());
                continue;
            }
        }

        log::info!("running {} check {}", name, entry.to_string_lossy());

        // Sort between scripts and binaries since they require different commands to execute properly.
        let output = if entry.extension().and_then(|ext| ext.to_str()) == Some("sh") {
            Command::new("bash").arg("-C").arg(&entry).output()
        } else {
            Command::new(&entry).output()
        };

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
                result
                    .errors
                    .push(Box::new(std::io::Error::other(error_msg)));
                if name == "required" {
                    break;
                }
            }
            Err(e) => {
                result.errors.push(Box::new(e));
                if name == "required" {
                    break;
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::{Context, Result};
    use std::fs::File;
    use std::io::Write;
    use std::sync::Once;
    use std::{fs, os::unix::fs::PermissionsExt};

    static INIT: Once = Once::new();

    fn init_logger() {
        INIT.call_once(|| {
            env_logger::builder().is_test(true).try_init().ok();
        });
    }

    static GREENBOOT_INSTALL_PATHS: [&str; 2] = ["/usr/lib/greenboot", "/etc/greenboot"];

    /// validate when the required folder is not found
    #[test]
    fn test_missing_required_folder() {
        let required_path = format!("{}/check/required.d", GREENBOOT_INSTALL_PATHS[1]);
        if Path::new(&required_path).exists() {
            fs::remove_dir_all(&required_path).unwrap();
        }
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
    fn test_required_script_failure_exit_early() {
        init_logger();
        setup_folder_structure(false)
            .context("Test setup failed")
            .unwrap();
        // Causes errors if these are not removed since they cause an excess amount
        // of failures.
        let required_path = format!("{}/check/required.d", GREENBOOT_INSTALL_PATHS[1]);
        let _ = std::fs::remove_file(format!("{}/01_failing_binary", required_path));
        let _ = std::fs::remove_file(format!("{}/02_failing_binary", required_path));

        let base_path = GREENBOOT_INSTALL_PATHS[1];

        let counter_file = format!("{}/fail_counter.txt", base_path);
        let mut file = File::create(&counter_file).expect("Failed to create counter file");
        writeln!(file, "0").unwrap();

        // Inject counter logic into the failing scripts
        for name in ["01_failing_script", "02_failing_script"] {
            let path = format!("{}/check/required.d/{}.sh", base_path, name);
            let mut script = File::create(&path).unwrap();
            writeln!(
                script,
                "#!/bin/bash\nCOUNTER_FILE=\"{}\"\ncount=$(cat $COUNTER_FILE)\necho $((count + 1)) >| $COUNTER_FILE\nexit 1",
                counter_file
            ).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let result = run_diagnostics(vec![]);
        log::debug!("Diagnostics result: {:?}", result);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "required health-check failed, skipping remaining scripts"
        );

        log::info!("Health check failed as expected.");

        let fail_script_count = fs::read_to_string(counter_file)
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap();
        assert_eq!(
            fail_script_count, 1,
            "Only one failing script should have executed"
        );

        tear_down().expect("teardown failed");
    }

    #[test]
    fn test_skip_nonexistent_script() {
        let nonexistent_script_name = "nonexistent_script.sh".to_string();
        setup_folder_structure(true)
            .context("Test setup failed")
            .unwrap();

        // Try to run a script that doesn't exist
        let state = run_diagnostics(vec![nonexistent_script_name.clone()]);
        assert!(
            state.unwrap().contains(&nonexistent_script_name),
            "non existent script names did not match"
        );

        tear_down().context("Test teardown failed").unwrap();
    }

    #[test]
    fn test_skip_disabled_script() {
        setup_folder_structure(false)
            .context("Test setup failed")
            .unwrap();

        // Removing extra failing binaries because this can cause a
        // failure if not added to the skips or removed as done below.
        let required_path = format!("{}/check/required.d", GREENBOOT_INSTALL_PATHS[1]);
        let _ = std::fs::remove_file(format!("{}/01_failing_binary", required_path));
        let _ = std::fs::remove_file(format!("{}/02_failing_binary", required_path));

        // Skip the disabled script in required.d ,since there are two
        // failing- scripts passing them both so that this test passes.
        let state = run_diagnostics(vec![
            "01_failing_script.sh".to_string(),
            "02_failing_script.sh".to_string(),
        ]);
        assert!(
            state.is_ok(),
            "Should pass when skipping disabled required script"
        );

        tear_down().context("Test teardown failed").unwrap();
    }

    // Since binaries are a separate and later added feature compared to
    // scripts, there should be a separate test to ensure they both work.
    #[test]
    fn test_skip_disabled_binary() {
        setup_folder_structure(false)
            .context("Test setup failed")
            .unwrap();

        // Removing extra failing scripts because this can cause a
        // failure if not added to the skips or removed as done below
        let required_path = format!("{}/check/required.d", GREENBOOT_INSTALL_PATHS[1]);
        let _ = std::fs::remove_file(format!("{}/01_failing_script.sh", required_path));
        let _ = std::fs::remove_file(format!("{}/02_failing_script.sh", required_path));

        // Skip the disabled script in required.d ,since there are two
        // failing- scripts passing them both so that this test passes.
        let state = run_diagnostics(vec![
            "01_failing_binary".to_string(),
            "02_failing_binary".to_string(),
        ]);
        assert!(
            state.is_ok(),
            "Should pass when skipping disabled required binary"
        );

        tear_down().context("Test teardown failed").unwrap();
    }

    fn setup_folder_structure(passing: bool) -> Result<()> {
        let required_path = format!("{}/check/required.d", GREENBOOT_INSTALL_PATHS[1]);
        let wanted_path = format!("{}/check/wanted.d", GREENBOOT_INSTALL_PATHS[1]);
        let passing_test_scripts = "testing_assets/passing_script.sh";
        let failing_test_scripts = "testing_assets/failing_script.sh";
        let passing_test_binary = "testing_assets/passing_binary";
        let failing_test_binary = "testing_assets/failing_binary";

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

        // Create passing binary in both required and wanted
        fs::copy(
            passing_test_binary,
            format!("{}/passing_binary", &required_path),
        )
        .context("unable to copy passing binary to required.d")?;

        fs::copy(
            passing_test_binary,
            format!("{}/passing_binary", &wanted_path),
        )
        .context("unable to copy passing binary to wanted.d")?;

        // Create failing script in wanted.d
        fs::copy(
            failing_test_scripts,
            format!("{}/failing_script.sh", &wanted_path),
        )
        .context("unable to copy failing script to wanted.d")?;

        // Create failing binary in wanted.d
        fs::copy(
            failing_test_binary,
            format!("{}/failing_binary", &wanted_path),
        )
        .context("unable to copy failing binary to wanted.d")?;

        if !passing {
            // Create multiple failing script in required.d for failure cases
            fs::copy(
                failing_test_scripts,
                format!("{}/01_failing_script.sh", &required_path),
            )
            .context("unable to copy failing script to required.d")?;
            fs::copy(
                failing_test_scripts,
                format!("{}/02_failing_script.sh", &required_path),
            )
            .context("unable to copy another failing script to required.d")?;

            // Create multiple failing binaries in required.d for failure cases
            fs::copy(
                failing_test_scripts,
                format!("{}/01_failing_binary", &required_path),
            )
            .context("unable to copy failing binary to required.d")?;
            fs::copy(
                failing_test_scripts,
                format!("{}/02_failing_binary", &required_path),
            )
            .context("unable to copy another failing binary to required.d")?;
        }
        Ok(())
    }

    fn tear_down() -> Result<()> {
        fs::remove_dir_all(GREENBOOT_INSTALL_PATHS[1]).expect("Unable to delete folder");
        Ok(())
    }
}
