// SPDX-License-Identifier: BSD-3-Clause

pub mod greenboot;
pub mod grub;
pub mod handler;
pub mod mount;

// Re-export public API
pub use greenboot::*;
pub use grub::*;
pub use handler::*;
pub use mount::*;

#[cfg(test)]
mod test {
    // Without Conflicts=/Before=final.target, nothing stops this unit at
    // shutdown, so ExecStop= (where all its work happens) never runs (#218).
    const ROLLBACK_TRIGGER_SERVICE: &str =
        include_str!("../../usr/lib/systemd/system/greenboot-set-rollback-trigger.service");

    #[test]
    fn test_rollback_trigger_service_stops_at_shutdown() {
        assert!(
            ROLLBACK_TRIGGER_SERVICE.contains("Conflicts=final.target"),
            "missing Conflicts=final.target; ExecStop= won't run at shutdown (#218)"
        );
        assert!(
            ROLLBACK_TRIGGER_SERVICE.contains("Before=final.target"),
            "missing Before=final.target; ExecStop= won't run before final.target (#218)"
        );
    }

    // bootupd concatenates static grub2 snippets; no trailing newline here
    // would merge the next snippet's first line onto this one.
    const GREENBOOT_GRUB_CFG: &str = include_str!("../../grub2/08_greenboot.cfg");

    #[test]
    fn test_greenboot_grub_cfg_ends_with_newline() {
        assert!(
            GREENBOOT_GRUB_CFG.ends_with('\n'),
            "missing trailing newline; bootupd may merge the next snippet onto this line"
        );
    }
}
