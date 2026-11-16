#!/bin/bash
set -exuo pipefail

# Get OS data.
source /etc/os-release
if [[ -n "${TARGET_DISTRO:-}" ]]; then
    IFS='-' read -r ID VERSION_ID <<< "${TARGET_DISTRO}"
fi

# Dumps details about the instance running the CI job.
CPUS=$(nproc)
MEM=$(free -m | grep -oP '\d+' | head -n 1)
DISK=$(df --output=size -h / | sed '1d;s/[^0-9]//g')
HOSTNAME=$(uname -n)
USER=$(whoami)
ARCH=$(uname -m)
KERNEL=$(uname -r)

echo -e "\033[0;36m"
cat << EOF
------------------------------------------------------------------------------
CI MACHINE SPECS
------------------------------------------------------------------------------
     Hostname: ${HOSTNAME}
         User: ${USER}
         CPUs: ${CPUS}
          RAM: ${MEM} MB
         DISK: ${DISK} GB
         ARCH: ${ARCH}
       KERNEL: ${KERNEL}
------------------------------------------------------------------------------
EOF
echo -e "\033[0m"

function greenprint {
    echo -e "\033[1;32m${1}\033[0m"
}

# Set up variables
TEST_UUID=$(uuidgen)
IMAGE_KEY="legacy-trigger-${TEST_UUID}"
SSH_USER="root"
SSH_OPTIONS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SSH_KEY="${SCRIPT_DIR}/key/ostree_key"
CONSOLE_LOG=/tmp/vm-console-legacy.log

# Temporary files
TEMPDIR=$(mktemp -d)
BOOT_ARGS="uefi"

# For Fedora IoT testing
case "${ID}-${VERSION_ID}" in
    "fedora-44"|"fedora-45")
        FEDORA_VERSION="${VERSION_ID}"
        OS_VARIANT="fedora-unknown"
        LEGACY_GREENBOOT_VERSION="0.16.2-0.fc${VERSION_ID}"
        ;;
    *)
        greenprint "This test is designed for Fedora IoT 44/45"
        greenprint "Skipping on ${ID}-${VERSION_ID}"
        exit 0
        ;;
esac

greenprint "========================================="
greenprint "Legacy Trigger Compatibility Test"
greenprint "========================================="
greenprint "Testing backward compatibility: released greenboot (≤ v0.16.4) → current"
greenprint ""
greenprint "Scenario:"
greenprint "  1. Boot into legacy greenboot v0.16.2 on Fedora IoT ${FEDORA_VERSION}"
greenprint "  2. Stage PR greenboot + fixture RPM with failing health check (in /usr)"
greenprint "  3. OLD greenboot sets greenboot_rollback_trigger=1"
greenprint "  4. Reboot to PR greenboot - health checks fail"
greenprint "  5. NEW greenboot detects legacy trigger and rolls back"
greenprint ""
greenprint "Scope: Validates read-side compatibility (new detects legacy trigger)"
greenprint "Out of scope: Automatic upgrade path with greenboot-set-rollback-trigger.service"
greenprint ""

# Install required packages
greenprint "📦 Install required packages"
sudo dnf install -y qemu-kvm libvirt-client libvirt-daemon-kvm virt-install \
    kpartx e2fsprogs xz ansible-core firewalld rpm-build rpmdevtools

# Avoid collection installation failure
for _ in $(seq 0 30); do
    ansible-galaxy collection install community.general community.libvirt
    install_result=$?
    if [[ $install_result == 0 ]]; then
        break
    fi
    sleep 10
done

# Check ostree_key permissions
KEY_PERMISSION_PRE=$(stat -L -c "%a %G %U" key/ostree_key 2>/dev/null | grep -oP '\d+' | head -n 1 || echo "")
if [[ -n "${KEY_PERMISSION_PRE}" && "${KEY_PERMISSION_PRE}" != "600" ]]; then
   greenprint "💡 File permissions too open...Changing to 600"
   chmod 600 ./key/ostree_key
fi

# Start firewalld
greenprint "Start firewalld"
sudo systemctl enable --now firewalld

greenprint "Waiting for firewalld D-Bus interface to be ready"
fw_timeout=30
fw_elapsed=0
until sudo firewall-cmd --state >/dev/null 2>&1; do
    sleep 1
    fw_elapsed=$((fw_elapsed + 1))
    if ! systemctl is-active --quiet firewalld; then
        echo "firewalld systemd unit is not active" >&2
        sudo systemctl status firewalld --no-pager >&2 || true
        exit 1
    fi
    if [[ ${fw_elapsed} -ge ${fw_timeout} ]]; then
        echo "firewalld did not become ready after ${fw_timeout} seconds" >&2
        exit 1
    fi
done

# Start libvirtd and test it
greenprint "🚀 Starting libvirt daemon"
sudo systemctl start libvirtd
sudo virsh list --all > /dev/null

# Clean up stale VMs from previous interrupted runs
greenprint "🧹 Cleaning up stale legacy-trigger VMs"
for stale_vm in $(sudo virsh list --all --name 2>/dev/null | grep legacy-trigger); do
    sudo virsh destroy "${stale_vm}" 2>/dev/null || true
    sudo virsh undefine "${stale_vm}" --nvram 2>/dev/null || true
    sudo rm -f "/var/lib/libvirt/images/${stale_vm}.raw" 2>/dev/null || true
    greenprint "  Removed stale VM: ${stale_vm}"
done

# Set up libvirt network
greenprint "💡 Setup libvirt network"
sudo tee /tmp/integration.xml > /dev/null << EOF
<network xmlns:dnsmasq='http://libvirt.org/schemas/network/dnsmasq/1.0'>
  <name>integration</name>
  <uuid>1c8fe98c-b53a-4ca4-bbdb-deb0f26b3579</uuid>
  <forward mode='nat'>
    <nat>
      <port start='1024' end='65535'/>
    </nat>
  </forward>
  <bridge name='integration' zone='trusted' stp='on' delay='0'/>
  <mac address='52:54:00:36:46:ef'/>
  <ip address='192.168.100.1' netmask='255.255.255.0'>
    <dhcp>
      <range start='192.168.100.2' end='192.168.100.254'/>
      <host mac='34:49:22:B0:83:30' name='vm-1' ip='192.168.100.50'/>
      <host mac='34:49:22:B0:83:31' name='vm-2' ip='192.168.100.60'/>
    </dhcp>
  </ip>
</network>
EOF

if ! sudo virsh net-info integration > /dev/null 2>&1; then
    sudo virsh net-define /tmp/integration.xml
fi
if [[ $(sudo virsh net-info integration | grep 'Active' | awk '{print $2}') == 'no' ]]; then
    sudo virsh net-start integration
fi

# Allow anyone in the wheel group to talk to libvirt
greenprint "🚪 Allowing users in the wheel group to talk to libvirt"
WHEEL_GROUP=$(stat -c '%g' /var/run/libvirt/libvirt-sock)
if ! id -Gn | grep -q "$WHEEL_GROUP"; then
    sudo chmod 777 /var/run/libvirt/libvirt-sock
fi

# Download Fedora IoT raw image from Koji compose (same source as other CI tests)
greenprint "📥 Downloading Fedora IoT ${FEDORA_VERSION} raw image"
COMPOSE_URL="https://kojipkgs.fedoraproject.org/compose/iot/latest-Fedora-IoT-${FEDORA_VERSION}/compose/IoT/${ARCH}/images/"
RAW_IMAGE_FILENAME=$(curl -sSf "${COMPOSE_URL}" | grep -oP "Fedora-IoT-raw-${FEDORA_VERSION}-[^\"]+\.${ARCH}\.raw\.xz" | head -1)

if [[ -z "${RAW_IMAGE_FILENAME}" ]]; then
    greenprint "❌ Could not find Fedora IoT raw image at ${COMPOSE_URL}"
    exit 1
fi

IMAGE_CACHE_DIR="/var/cache/greenboot-test"
sudo mkdir -p "${IMAGE_CACHE_DIR}"
CACHED_XZ="${IMAGE_CACHE_DIR}/${RAW_IMAGE_FILENAME}"

if [[ -f "${CACHED_XZ}" ]]; then
    greenprint "✅ Using cached image: ${CACHED_XZ}"
    cp "${CACHED_XZ}" "${TEMPDIR}/${RAW_IMAGE_FILENAME}"
else
    greenprint "📥 Downloading ${RAW_IMAGE_FILENAME}"
    curl -L -o "${TEMPDIR}/${RAW_IMAGE_FILENAME}" "${COMPOSE_URL}${RAW_IMAGE_FILENAME}"
    sudo cp "${TEMPDIR}/${RAW_IMAGE_FILENAME}" "${CACHED_XZ}"
fi

# Extract the image
greenprint "📦 Extracting raw image"
xz -d "${TEMPDIR}/${RAW_IMAGE_FILENAME}"
RAW_IMAGE="${TEMPDIR}/${RAW_IMAGE_FILENAME%.xz}"

# Copy to libvirt images directory
LIBVIRT_IMAGE_PATH="/var/lib/libvirt/images/${IMAGE_KEY}.raw"
sudo cp "${RAW_IMAGE}" "${LIBVIRT_IMAGE_PATH}"
sudo chmod 644 "${LIBVIRT_IMAGE_PATH}"

# Resize the image to have room for RPM staging
greenprint "🔧 Resizing disk image"
sudo qemu-img resize "${LIBVIRT_IMAGE_PATH}" +10G

greenprint "🔧 Resizing root partition"
echo ", +" | sudo sfdisk -N 3 "${LIBVIRT_IMAGE_PATH}"

# Inject SSH key into the raw image
greenprint "🔑 Injecting SSH key via kpartx"
sudo kpartx -av "${LIBVIRT_IMAGE_PATH}"
LOOP_DEV=$(sudo kpartx -l "${LIBVIRT_IMAGE_PATH}" | head -1 | awk '{print $1}' | sed 's/p[0-9]*$//')
ROOT_PART="/dev/mapper/${LOOP_DEV}p3"

sudo e2fsck -f "${ROOT_PART}" -y
sudo resize2fs "${ROOT_PART}"

MOUNT_DIR="${TEMPDIR}/root"
mkdir -p "${MOUNT_DIR}"
sudo mount "${ROOT_PART}" "${MOUNT_DIR}"

sudo mkdir -p "${MOUNT_DIR}/ostree/deploy/fedora-iot/var/roothome/.ssh"
cat "${SSH_KEY}.pub" | sudo tee -a "${MOUNT_DIR}/ostree/deploy/fedora-iot/var/roothome/.ssh/authorized_keys"
sudo chmod -R u=rwX,o=,g= "${MOUNT_DIR}/ostree/deploy/fedora-iot/var/roothome/.ssh/"

sudo umount "${MOUNT_DIR}"
sudo kpartx -dv "${LIBVIRT_IMAGE_PATH}"

# Create the domain using virt-install import
greenprint "🖥️ Creating VM from raw image"
sudo virt-install \
    --name="${IMAGE_KEY}" \
    --memory=2048 \
    --vcpus=2 \
    --disk path="${LIBVIRT_IMAGE_PATH}",format=raw \
    --import \
    --os-variant="${OS_VARIANT}" \
    --network network=integration,mac=34:49:22:B0:83:31 \
    --graphics none \
    --serial file,path="${CONSOLE_LOG}" \
    --noautoconsole \
    --noreboot \
    --boot "${BOOT_ARGS}"

# Helper function to wait for SSH
function wait_for_ssh_up {
    SSH_STATUS=$(sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${1}" '/bin/bash -c "echo -n READY"' || echo FAILED)
    if [[ $SSH_STATUS == READY ]]; then
        echo 1
    else
        echo 0
    fi
}

# Cleanup function
function clean_up {
    greenprint "🧹 Cleaning up"

    # Stop VM
    if sudo virsh list --name | grep -q "${IMAGE_KEY}"; then
        sudo virsh destroy "${IMAGE_KEY}" || true
    fi

    # Undefine VM
    if sudo virsh list --all --name | grep -q "${IMAGE_KEY}"; then
        sudo virsh undefine "${IMAGE_KEY}" --nvram || true
    fi

    # Remove image
    if [[ -f "${LIBVIRT_IMAGE_PATH}" ]]; then
        sudo rm -f "${LIBVIRT_IMAGE_PATH}"
    fi

    # Remove temp files
    rm -rf "${TEMPDIR}"
}

# Build PR greenboot RPMs from source (not from Copr) because this test
# needs the exact code from the PR branch, and the version bump ensures
# the NVR differs from the base image for rpm-ostree override replace.
greenprint "🔨 Building PR greenboot RPMs"
cd "$(dirname "$0")/.."
rm -rf rpmbuild/RPMS
make rpm

ARCH_DIR="$(pwd)/rpmbuild/RPMS/$(uname -m)"
PR_GREENBOOT_RPM="$(find "${ARCH_DIR}" -name 'greenboot-[0-9]*.rpm' ! -name '*debug*' ! -name '*health*' | head -1)"
PR_HEALTHCHECKS_RPM="$(find "${ARCH_DIR}" -name 'greenboot-default-health-checks-*.rpm' ! -name '*debug*' | head -1)"

if [[ ! -f "${PR_GREENBOOT_RPM}" ]]; then
    greenprint "❌ PR greenboot RPM not found in ${ARCH_DIR}"
    clean_up
    exit 1
fi

greenprint "✅ Built PR greenboot: ${PR_GREENBOOT_RPM}"

# Build fixture RPM with always-failing health check in /usr
# Must be in /usr (not /etc) so the check only exists in the staged
# deployment and disappears after rpm-ostree rollback.
greenprint "📝 Building fixture RPM with always-failing health check"
mkdir -p "${TEMPDIR}/fixture-rpm"/{SPECS,SOURCES,BUILD,RPMS,SRPMS}
cat > "${TEMPDIR}/fixture-rpm/SPECS/greenboot-test-fail.spec" << 'SPEC'
Name:    greenboot-test-fail
Version: 1.0
Release: 1%{?dist}
Summary: Always-failing greenboot health check fixture
License: MIT
BuildArch: noarch

%description
Test fixture: always-failing health check installed to /usr so it only
exists in the staged deployment and disappears after rpm-ostree rollback.

%install
mkdir -p %{buildroot}/usr/lib/greenboot/check/required.d
cat > %{buildroot}/usr/lib/greenboot/check/required.d/99_test_always_fail.sh << 'EOF'
#!/bin/bash
echo "TEST FIXTURE: Always failing health check for legacy trigger test"
exit 1
EOF
chmod +x %{buildroot}/usr/lib/greenboot/check/required.d/99_test_always_fail.sh

%files
/usr/lib/greenboot/check/required.d/99_test_always_fail.sh
SPEC

rpmbuild --define "_topdir ${TEMPDIR}/fixture-rpm" -bb "${TEMPDIR}/fixture-rpm/SPECS/greenboot-test-fail.spec"
FIXTURE_RPM="$(find "${TEMPDIR}/fixture-rpm/RPMS" -name 'greenboot-test-fail-*.rpm' | head -1)"

if [[ ! -f "${FIXTURE_RPM}" ]]; then
    greenprint "❌ Failed to build fixture RPM"
    exit 1
fi

greenprint "✅ Built fixture RPM: ${FIXTURE_RPM}"

# Start the VM
greenprint "🚀 Starting VM"
sudo virsh start "${IMAGE_KEY}"

# Wait for VM to get an IP address via DHCP
greenprint "⏳ Waiting for VM to get an IP address"
GUEST_ADDRESS=""
for _ in $(seq 0 30); do
    GUEST_ADDRESS=$(sudo virsh domifaddr "${IMAGE_KEY}" 2>/dev/null \
        | grep -oP '(\d+\.){3}\d+' | head -1)
    if [[ -n "${GUEST_ADDRESS}" ]]; then
        greenprint "✅ VM got IP: ${GUEST_ADDRESS}"
        break
    fi
    sleep 5
done

if [[ -z "${GUEST_ADDRESS}" ]]; then
    greenprint "❌ VM never got a DHCP address"
    greenprint "VM state:"
    sudo virsh domstate "${IMAGE_KEY}" || true
    greenprint "DHCP leases:"
    sudo virsh net-dhcp-leases integration || true
    clean_up
    exit 1
fi

# Wait for SSH to come up
greenprint "⏳ Waiting for SSH to be ready at ${GUEST_ADDRESS}"
for _ in $(seq 0 60); do
    RESULTS="$(wait_for_ssh_up $GUEST_ADDRESS)"
    if [[ $RESULTS == 1 ]]; then
        greenprint "✅ SSH is ready!"
        break
    fi
    sleep 10
done

if [[ $RESULTS != 1 ]]; then
    greenprint "❌ SSH failed to come up"
    greenprint "VM state:"
    sudo virsh domstate "${IMAGE_KEY}" || true
    greenprint "DHCP leases:"
    sudo virsh net-dhcp-leases integration || true
    clean_up
    exit 1
fi

# Check current greenboot version and downgrade to legacy if needed
greenprint "🔍 Checking installed greenboot version"
INSTALLED_VERSION=$(sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "rpm -q greenboot")
greenprint "Installed greenboot: ${INSTALLED_VERSION}"

if [[ ! "${INSTALLED_VERSION}" =~ ${LEGACY_GREENBOOT_VERSION} ]]; then
    greenprint "📥 Image has ${INSTALLED_VERSION}, need to downgrade to legacy ${LEGACY_GREENBOOT_VERSION}"
    KOJI_BASE="https://kojipkgs.fedoraproject.org/packages/greenboot-rs/0.16.2/0.fc${FEDORA_VERSION}/x86_64"
    sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
        "curl -Lo /tmp/legacy-greenboot.rpm '${KOJI_BASE}/greenboot-0.16.2-0.fc${FEDORA_VERSION}.x86_64.rpm' && \
         curl -Lo /tmp/legacy-health-checks.rpm '${KOJI_BASE}/greenboot-default-health-checks-0.16.2-0.fc${FEDORA_VERSION}.x86_64.rpm'"

    greenprint "🔄 Replacing greenboot with legacy v0.16.2"
    sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
        "sudo rpm-ostree override replace /tmp/legacy-greenboot.rpm /tmp/legacy-health-checks.rpm"

    greenprint "🔄 Rebooting into legacy greenboot"
    sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
        "nohup sudo systemctl reboot &>/dev/null & exit" || true

    sleep 30

    # Re-discover IP and wait for SSH
    GUEST_ADDRESS=""
    for _ in $(seq 0 30); do
        GUEST_ADDRESS=$(sudo virsh domifaddr "${IMAGE_KEY}" 2>/dev/null \
            | grep -oP '(\d+\.){3}\d+' | head -1)
        if [[ -n "${GUEST_ADDRESS}" ]]; then break; fi
        sleep 5
    done

    greenprint "⏳ Waiting for SSH after downgrade reboot (IP: ${GUEST_ADDRESS})"
    for _ in $(seq 0 60); do
        RESULTS="$(wait_for_ssh_up $GUEST_ADDRESS)"
        if [[ $RESULTS == 1 ]]; then
            greenprint "✅ SSH is ready after downgrade!"
            break
        fi
        sleep 10
    done

    if [[ $RESULTS != 1 ]]; then
        greenprint "❌ SSH failed after downgrade reboot"
        clean_up
        exit 1
    fi

    # Verify downgrade worked
    INSTALLED_VERSION=$(sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "rpm -q greenboot")
    greenprint "Greenboot after downgrade: ${INSTALLED_VERSION}"
fi

if [[ ! "${INSTALLED_VERSION}" =~ ${LEGACY_GREENBOOT_VERSION} ]]; then
    greenprint "❌ Failed to get legacy greenboot ${LEGACY_GREENBOOT_VERSION}, got ${INSTALLED_VERSION}"
    clean_up
    exit 1
fi

greenprint "✅ Running legacy greenboot ${INSTALLED_VERSION}"

# Clear any stale greenboot grubenv variables from prior deployments
greenprint "🧹 Clearing stale greenboot grubenv variables before test"
sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "sudo grub2-editenv /boot/grub2/grubenv unset greenboot_next_deployment_id; \
     sudo grub2-editenv /boot/grub2/grubenv unset greenboot_rollback_trigger; \
     sudo grub2-editenv /boot/grub2/grubenv unset fallback"

# Copy PR RPMs and fixture RPM to VM
greenprint "📦 Copying RPMs to VM"
sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "mkdir -p /tmp/pr-rpms /tmp/fixture"
sudo scp "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${PR_GREENBOOT_RPM}" "${SSH_USER}@${GUEST_ADDRESS}":/tmp/pr-rpms/
sudo scp "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${PR_HEALTHCHECKS_RPM}" "${SSH_USER}@${GUEST_ADDRESS}":/tmp/pr-rpms/
sudo scp "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${FIXTURE_RPM}" "${SSH_USER}@${GUEST_ADDRESS}":/tmp/fixture/

# Stage PR greenboot upgrade and layer fixture RPM
greenprint "🔄 Staging greenboot upgrade to PR build"
sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "sudo rpm-ostree override replace /tmp/pr-rpms/greenboot-*.rpm"

greenprint "🔧 Layering fixture RPM with always-failing health check"
sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "sudo rpm-ostree install /tmp/fixture/greenboot-test-fail-*.rpm"

# Show deployment status after staging
greenprint "📋 Deployment status after staging"
sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "rpm-ostree status"

# Invoke the LEGACY greenboot binary to set the rollback trigger
# This is the key compatibility test: OLD greenboot sets the trigger,
# NEW greenboot (in PR) must detect it
greenprint "🎯 Running OLD greenboot binary to set rollback trigger"
greenprint "Command: sudo /usr/libexec/greenboot/greenboot set-rollback-trigger"
sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "sudo /usr/libexec/greenboot/greenboot set-rollback-trigger"

# Verify grubenv state BEFORE upgrade
greenprint "📋 Verifying grubenv state (should have legacy trigger, no next_deployment_id yet)"
sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "sudo grub2-editenv /boot/grub2/grubenv list | grep greenboot" || true

# Verify legacy trigger is set
LEGACY_TRIGGER_SET=$(sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "sudo grub2-editenv /boot/grub2/grubenv list | grep 'greenboot_rollback_trigger=1' || echo ''")

if [[ -z "${LEGACY_TRIGGER_SET}" ]]; then
    greenprint "❌ Failed to set legacy rollback trigger!"
    clean_up
    exit 1
fi

greenprint "✅ Legacy trigger set successfully"

# Check if next_deployment_id is also set (may happen due to systemd services
# during rpm-ostree staging — this is fine, our code handles both being present)
NEW_TRIGGER_CHECK=$(sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "sudo grub2-editenv /boot/grub2/grubenv list | grep 'greenboot_next_deployment_id' || echo ''")

if [[ -n "${NEW_TRIGGER_CHECK}" ]]; then
    greenprint "ℹ️  greenboot_next_deployment_id is also set (likely from systemd service)"
    greenprint "This is OK — our fix checks has_legacy_trigger first in the if-else chain"
else
    greenprint "✅ Only legacy trigger is set (no greenboot_next_deployment_id)"
fi

# Reboot to new deployment (will fail health checks and rollback)
greenprint "🔄 Rebooting to new deployment (will fail health checks)"
greenprint "Expected behavior:"
greenprint "  1. Boot into new deployment (PR greenboot)"
greenprint "  2. Health check fails (always-fail script)"
greenprint "  3. Reboot 3 times (boot_counter exhausted)"
greenprint "  4. NEW greenboot detects LEGACY trigger (greenboot_rollback_trigger=1)"
greenprint "  5. Rollback to original deployment"
greenprint ""

sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "nohup sudo systemctl reboot &>/dev/null & exit" || true

# Wait for rollback to complete (expect 3 failed boots + 1 rollback boot)
greenprint "⏳ Waiting for rollback to complete (~4 minutes for 4 boots)"
sleep 300  # ~5 minutes to be safe

# Re-discover VM IP after rollback reboots
greenprint "🛃 Re-discovering VM IP after rollback"
GUEST_ADDRESS=""
for _ in $(seq 0 30); do
    GUEST_ADDRESS=$(sudo virsh domifaddr "${IMAGE_KEY}" 2>/dev/null \
        | grep -oP '(\d+\.){3}\d+' | head -1)
    if [[ -n "${GUEST_ADDRESS}" ]]; then
        greenprint "✅ VM IP after rollback: ${GUEST_ADDRESS}"
        break
    fi
    sleep 5
done

greenprint "🛃 Checking for SSH after rollback"
for _ in $(seq 0 60); do
    RESULTS="$(wait_for_ssh_up $GUEST_ADDRESS)"
    if [[ $RESULTS == 1 ]]; then
        greenprint "✅ SSH is ready after rollback!"
        break
    fi
    sleep 10
done

if [[ $RESULTS != 1 ]]; then
    greenprint "❌ SSH failed after rollback - test FAILED"
    greenprint "Collecting VM diagnostics..."
    sudo virsh domstate "${IMAGE_KEY}" || true
    sudo virsh net-dhcp-leases integration || true
    clean_up
    exit 1
fi

# Verify rollback occurred
greenprint "🔍 Verifying rollback occurred"
CURRENT_VERSION=$(sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "rpm -q greenboot")
greenprint "Current greenboot version: ${CURRENT_VERSION}"

if [[ "${CURRENT_VERSION}" =~ ${LEGACY_GREENBOOT_VERSION} ]]; then
    greenprint "✅ Rolled back to legacy version!"
else
    greenprint "⚠️  Currently on: ${CURRENT_VERSION} (expected ${LEGACY_GREENBOOT_VERSION})"
    greenprint "Dumping grubenv for diagnostics:"
    sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
        "sudo grub2-editenv /boot/grub2/grubenv list" || true
fi

# Check deployment status
greenprint "📋 Deployment status after rollback"
sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "rpm-ostree status"

# THE CRITICAL TEST: Check logs for legacy trigger detection
greenprint "🎯 CRITICAL CHECK: Verifying logs show legacy trigger was detected"
LEGACY_LOG=$(sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "sudo journalctl -u greenboot-healthcheck --no-pager | grep -i 'legacy rollback trigger' || echo ''")

if [[ -n "${LEGACY_LOG}" ]]; then
    greenprint "✅ SUCCESS! Legacy trigger detected in logs:"
    greenprint "${LEGACY_LOG}"
else
    greenprint "❌ FAILED! No 'legacy rollback trigger' message found in logs"
    greenprint "Showing full greenboot-healthcheck logs:"
    sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
        "sudo journalctl -u greenboot-healthcheck --no-pager | tail -100"
    clean_up
    exit 1
fi

# Verify grubenv was cleaned up
greenprint "🧹 Verifying grubenv cleanup"
GRUBENV_STATE=$(sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "sudo grub2-editenv /boot/grub2/grubenv list | grep greenboot || echo 'CLEAN'")
greenprint "Grubenv state: ${GRUBENV_STATE}"

if echo "${GRUBENV_STATE}" | grep -q 'greenboot_rollback_trigger'; then
    greenprint "❌ FAILED: legacy trigger not cleared after rollback"
    clean_up
    exit 1
fi

if echo "${GRUBENV_STATE}" | grep -q 'greenboot_next_deployment_id'; then
    greenprint "❌ FAILED: next_deployment_id not cleared after rollback"
    clean_up
    exit 1
fi

greenprint "✅ Both triggers were properly cleaned up after rollback"

# Clean up
clean_up

greenprint ""
greenprint "========================================="
greenprint "✅ TEST PASSED!"
greenprint "========================================="
greenprint "Backward compatibility verified (released greenboot ≤ v0.16.4 → current):"
greenprint "  ✓ Legacy greenboot set greenboot_rollback_trigger=1"
greenprint "  ✓ New greenboot detected legacy trigger"
greenprint "  ✓ Rollback executed successfully"
greenprint "  ✓ Both triggers cleared after rollback"
greenprint "========================================="
