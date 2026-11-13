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
