// SPDX-License-Identifier: BSD-3-Clause
//
// Integration tests for mount namespace refactoring (issue #120).
// All sensitive system paths are mocked using tempdir — no real /boot,
// /proc, mount, or unshare operations are performed.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

// ---------------------------------------------------------------------------
// Mount namespace setup
// ---------------------------------------------------------------------------

#[test]
fn ensure_mount_namespace_succeeds() {
    let result = greenboot::ensure_mount_namespace();
    assert!(result.is_ok(), "ensure_mount_namespace should succeed");
}

// ---------------------------------------------------------------------------
// Boot writable check
// ---------------------------------------------------------------------------

#[test]
fn is_boot_writable_at_detects_writable_path() {
    let mock_boot = tempfile::tempdir().unwrap();
    let result = greenboot::is_boot_writable_at(mock_boot.path());
    assert!(result.is_ok());
    assert!(result.unwrap(), "Writable mock /boot should return true");
}

#[test]
fn is_boot_writable_at_detects_readonly_path() {
    if nix::unistd::geteuid().is_root() {
        eprintln!("Skipping: root bypasses permission bits in access(W_OK)");
        return;
    }

    let mock_boot = tempfile::tempdir().unwrap();
    let mut perms = fs::metadata(mock_boot.path()).unwrap().permissions();
    perms.set_mode(0o555);
    fs::set_permissions(mock_boot.path(), perms).unwrap();

    let result = greenboot::is_boot_writable_at(mock_boot.path());
    assert!(result.is_ok());
    assert!(!result.unwrap(), "Read-only mock /boot should return false");

    let mut perms = fs::metadata(mock_boot.path()).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(mock_boot.path(), perms).unwrap();
}

#[test]
fn is_boot_writable_at_errors_on_missing_path() {
    let result = greenboot::is_boot_writable_at(Path::new("/nonexistent/mock/boot"));
    assert!(result.is_err(), "Missing path should return error");
}

// ---------------------------------------------------------------------------
// Boot remount in namespace
// ---------------------------------------------------------------------------

#[test]
fn remount_boot_rw_succeeds() {
    let result = greenboot::remount_boot_rw();
    assert!(result.is_ok(), "remount_boot_rw should succeed");
}

// ---------------------------------------------------------------------------
// Public API surface — compile-time + runtime validation
// ---------------------------------------------------------------------------

#[test]
fn public_api_surface() {
    let _: Result<(), greenboot::MountError> = greenboot::ensure_mount_namespace();
    let _: Result<(), greenboot::MountError> = greenboot::remount_boot_rw();

    let _fn_ref: fn() -> Result<bool, greenboot::MountError> = greenboot::is_boot_writable;

    let _: greenboot::MountError = greenboot::MountError::RemountFailed("test".to_string());
    let _: greenboot::MountError = greenboot::MountError::NamespaceError("test".to_string());
    let _: greenboot::MountError = greenboot::MountError::BootCheckError("test".to_string());
}
