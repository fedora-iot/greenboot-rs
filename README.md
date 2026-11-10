

[![Continuous integration](https://github.com/fedora-iot/greenboot-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/fedora-iot/greenboot-rs/actions/workflows/ci.yml)


# greenboot-rs
Rust implementation of [greenboot](https://github.com/fedora-iot/greenboot) designed for [bootc](https://bootc-dev.github.io/bootc/) based systems.

## Table of contents
- [greenboot-rs](#greenboot-rs)
  - [Table of contents](#table-of-contents)
  - [Installation](#installation)
  - [Usage](#usage)
    - [Health checks with bash scripts](#health-checks-with-bash-scripts)
      - [Health checks included with subpackage greenboot-default-health-checks](#health-checks-included-with-subpackage-greenboot-default-health-checks)
    - [Health Checks with systemd services](#health-checks-with-systemd-services)
    - [Configuration](#configuration)
  - [How does it work](#how-does-it-work)

## Installation
Greenboot is comprised of two packages:
- `greenboot` itself, with all core functionalities: check provided scripts, reboot if these checks don't pass, rollback to previous deployment if rebooting hasn't solved the problem, etc.
- `greenboot-default-health-checks`, a series of optional and curated health checks provided by Greenboot maintainers.

In order to get a full Greenboot installation on any bootc system like Fedora Silverblue, Fedora IoT or Fedora CoreOS, make container with:

```
RUN dnf install -y greenboot greenboot-default-health-checks

```

## Usage

### Health checks with bash scripts
Place shell scripts representing *health checks* that **MUST NOT FAIL** in the `/etc/greenboot/check/required.d` directory. If any script in this folder exits with an error code, the boot will be declared as failed. Error message will appear in both MOTD and in `journalctl -u greenboot-healthcheck.service`.
Place shell scripts representing *health checks* that **MAY FAIL** in the `/etc/greenboot/check/wanted.d` directory. Scripts in this folder can exit with an error code and the boot will not be declared as failed. Error message will appear in both MOTD and in `journalctl -u greenboot-healthcheck.service -b`.
Place shell scripts you want to run *after* a boot has been declared **successful** (green) in `/etc/greenboot/green.d`.
Place shell scripts you want to run *after* a boot has been declared **failed** (red) in `/etc/greenboot/red.d`.

Unless greenboot is enabled by default in your distribution, enable it by running `systemctl enable greenboot-healthcheck.service`.
It will automatically start during the next boot process and run its checks.

When you `ssh` into the machine after that, a boot status message will be shown:

```
Boot Status is GREEN - Health Check SUCCESS
```
```
Boot Status is RED - Health Check FAILURE!
```

Directory structure: 
```
/etc
└── greenboot
    ├── check
    │   ├── required.d
    │   └── wanted.d
    ├── green.d
    └── red.d
```

#### Health checks included with subpackage greenboot-default-health-checks
These health checks are available in `/usr/lib/greenboot/check`, a read-only directory in ostree systems. If you find a bug in any of them or you have an improvement, please create a PR with such fix/feature and we'll review it and potentially include it.

- **Check if repositories URLs are still DNS solvable**: This script is under `/usr/lib/greenboot/check/required.d/01_repository_dns_check.sh` and makes sure that DNS queries to repository URLs are still available.
- **Check if update platforms are still reachable**: This script is under `/usr/lib/greenboot/check/wanted.d/01_update_platform_check.sh` and tries to connect and get a 2XX or 3XX HTTP code from the update platforms defined in `/etc/ostree/remotes.d`.
- **Check if current boot has been triggered by hardware watchdog**: This script is under `/usr/lib/greenboot/check/required.d/02_watchdog.sh` and checks whether the current boot has been watchdog-triggered or not. If it is, but the reboot has occurred after a certain grace period (default of 24 hours, configurable via `GREENBOOT_WATCHDOG_GRACE_PERIOD=number_of_hours` in `/etc/greenboot/greenboot.conf`), Greenboot won't mark the current boot as red and won't rollback to the previous deployment. If has occurred within the grace period, at the moment the current boot will be marked as red, but Greenboot won't rollback to the previous deployment. It is enabled by default but it can be disabled by modifying `GREENBOOT_WATCHDOG_CHECK_ENABLED` in `/etc/greenboot/greenboot.conf` to `false`.

### Health Checks with systemd services
Overall boot success is measured against `boot-complete.target`.
Ordering of units can be achieved using standard systemd vocabulary.

### Configuration
At the moment, it is possible to customize the following parameters via environment variables. These environment variables can be described as well in the config file `/etc/greenboot/greenboot.conf`:
- **GREENBOOT_MAX_BOOT_ATTEMPTS**: Maximum number of boot attempts before declaring the deployment as problematic and rolling back to the previous one.
- **GREENBOOT_WATCHDOG_CHECK_ENABLED**: Enables/disables *Check if current boot has been triggered by hardware watchdog* health check. More info on [Health checks included with subpackage greenboot-default-health-checks](#health-checks-included-with-subpackage-greenboot\-default\-health\-checks) section.
- **GREENBOOT_WATCHDOG_GRACE_PERIOD**: Number of hours after an upgrade that we consider the new deployment as culprit of reboot.

## How does it work
- `greenboot-healthcheck.service` runs **before** systemd's [boot-complete.target](https://www.freedesktop.org/software/systemd/man/systemd.special.html#boot-complete.target). It launches `/usr/libexec/greenboot/greenboot health-check`, which runs the `required.d` and `wanted.d` scripts.
  - If any script in the `required.d` folder fails
    - This will cause scripts in `red.d` folder to run.
    - After the above:
      - Creates the MOTD specifying which scripts have failed.
      - It performs a series of checks to determine if there's a requirement for manual intervention. If there's not, it reboots the system.
  - If all scripts in `required.d` folder succeeded:
    - `boot-complete.target` is reached.
    - Unsets `boot_counter` GRUB env var and sets `boot_success` GRUB env var to 1.
    - Runs the scripts in `green.d` folder, scripts that are meant to be run after a successful update.
    - Creates the MOTD with a success message.

## Integration Tests

To run integration tests:

1. **Requirements**:
   - Fedora Rawhide OR CentOS Stream 10
   - Quay.io account with container registry access

2. **Execution**:
   ```bash
   QUAY_USERNAME=<your_quay_username> QUAY_PASSWORD=<your_quay_password> make integration-test
   ```


### Testing bootc images with Brew RPMs

On RHEL 9.9 or RHEL 10.3, run either bootc test script from `tests/` with
`USE_BREW_RPMS=true` and direct download URLs for both binary RPMs from the
same build. Use the actual published filenames and architectures; the helper
preserves those filenames and validates package names, matching versions and
compatibility with the host architecture. Build-directory URLs and source RPMs
are not supported.

`DOWNLOAD_NODE` remains required as a hostname (without a URL scheme) for the
RHEL dependency repositories. Brew mode takes precedence over `COMPOSE_ID`;
missing URLs or download errors fail the test instead of falling back to Copr.
Internal Brew HTTPS downloads use `curl -k`, and Brew RPM installation uses
`--nogpgcheck` to accommodate unsigned builds.

With registry credentials exported, for example:

```bash
cd tests
sudo env \
  USE_BREW_RPMS=true \
  BREW_GREENBOOT_RPM_URL="https://brew.example/path/greenboot-0.16.4-0.el10.x86_64.rpm" \
  BREW_GREENBOOT_DEFAULT_HEALTH_CHECKS_RPM_URL="https://brew.example/path/greenboot-default-health-checks-0.16.4-0.el10.x86_64.rpm" \
  DOWNLOAD_NODE="download-node.example.com" \
  QUAY_USERNAME="${QUAY_USERNAME}" \
  QUAY_PASSWORD="${QUAY_PASSWORD}" \
  STAGE_REDHAT_IO_USERNAME="${STAGE_REDHAT_IO_USERNAME}" \
  STAGE_REDHAT_IO_TOKEN="${STAGE_REDHAT_IO_TOKEN}" \
  ./greenboot-bootc-qcow2.sh
```

Replace the example URLs with the build for the host's RHEL version and
architecture. To test Anaconda ISO instead, use the same environment variables
with `./greenboot-bootc-anaconda-iso.sh`. The scripts detect the distro as usual.


### Testing OSTree images with Brew RPMs

On RHEL 9.8 or RHEL 9.9, run the OSTree test from `tests/` with direct URLs
for both binary RPMs from the same build. Replace the example URLs with the
actual filenames for the host's RHEL version and architecture. RPM filenames
are preserved. The script detects the distro automatically.

```bash
cd tests
sudo env \
  USE_BREW_RPMS=true \
  BREW_GREENBOOT_RPM_URL="https://brew.example/path/greenboot-0.16.4-0.el9.x86_64.rpm" \
  BREW_GREENBOOT_DEFAULT_HEALTH_CHECKS_RPM_URL="https://brew.example/path/greenboot-default-health-checks-0.16.4-0.el9.x86_64.rpm" \
  DOWNLOAD_NODE="download-node.example.com" \
  ./greenboot-ostree.sh
```

`DOWNLOAD_NODE` is a hostname without a URL scheme and is still required for
RHEL installation content and dependency repositories. `COMPOSE_ID`,
`PR_NUMBER`, and container-registry credentials are not required in Brew mode.
Brew takes precedence over compose RPM selection. The downloaded RPMs use the
existing local repository at `/var/www/html/packages`, with GPG checking
already disabled in its composer source. Blueprint and dependency-resolution
behavior, including the existing expected-build check, are unchanged.
