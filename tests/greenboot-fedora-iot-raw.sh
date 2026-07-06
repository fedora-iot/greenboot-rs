#!/bin/bash
set -exuo pipefail

# Get OS data.
source /etc/os-release

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
echo "CPU info"
lscpu
echo -e "\033[0m"

# Colorful output.
function greenprint {
    echo -e "\033[1;32m${1}\033[0m"
}

# Set up variables.
TEST_UUID=$(uuidgen)
IMAGE_KEY="fedora-iot-raw-${TEST_UUID}"
GUEST_ADDRESS=192.168.100.50
SSH_USER="root"
CONSOLE_LOG=/tmp/vm-console.log

# Set up temporary files.
TEMPDIR=$(mktemp -d)
BOOT_ARGS="uefi"

# SSH setup.
SSH_OPTIONS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5)
SSH_KEY=key/ostree_key
SSH_KEY_PUB=$(cat "${SSH_KEY}".pub)

case "${ID}-${VERSION_ID}" in
    "fedora-44")
        FEDORA_VERSION="44"
        OS_VARIANT="fedora-unknown"
        ;;
    "fedora-45")
        FEDORA_VERSION="45"
        OS_VARIANT="fedora-rawhide"
        ;;
    *)
        echo "unsupported distro: ${ID}-${VERSION_ID}"
        exit 1;;
esac

# Install required packages
greenprint "Install required packages"
sudo dnf install -y --nogpgcheck ansible-core qemu-img firewalld qemu-kvm \
    libvirt-client libvirt-daemon-kvm libvirt-daemon virt-install xz \
    kpartx e2fsprogs

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
KEY_PERMISSION_PRE=$(stat -L -c "%a %G %U" key/ostree_key | grep -oP '\d+' | head -n 1)
echo -e "${KEY_PERMISSION_PRE}"
if [[ "${KEY_PERMISSION_PRE}" != "600" ]]; then
   greenprint "💡 File permissions too open...Changing to 600"
   chmod 600 ./key/ostree_key
fi

# Start firewalld
greenprint "Start firewalld"
sudo systemctl enable --now firewalld

# Start libvirtd and test it.
greenprint "🚀 Starting libvirt daemon"
sudo systemctl start libvirtd
sudo virsh list --all > /dev/null

# Set a customized dnsmasq configuration for libvirt so we always get the
# same address on boot up.
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

# Allow anyone in the wheel group to talk to libvirt.
greenprint "🚪 Allowing users in wheel group to talk to libvirt"
sudo tee /etc/polkit-1/rules.d/50-libvirt.rules > /dev/null << EOF
polkit.addRule(function(action, subject) {
    if (action.id == "org.libvirt.unix.manage" &&
        subject.isInGroup("adm")) {
            return polkit.Result.YES;
    }
});
EOF

# Wait for the ssh server up to be.
wait_for_ssh_up () {
    SSH_STATUS=$(sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${1}" '/bin/bash -c "echo -n READY"')
    if [[ $SSH_STATUS == READY ]]; then
        echo 1
    else
        echo 0
    fi
}

# Clean up our mess.
clean_up () {
    greenprint "🧼 Cleaning up"
    sudo virsh destroy "${IMAGE_KEY}"
    sudo virsh undefine "${IMAGE_KEY}" --nvram
    sudo rm -f "/var/lib/libvirt/images/${IMAGE_KEY}.raw"
    # Remomve tmp dir.
    sudo rm -rf "$TEMPDIR"
}

# Test result checking
check_result () {
    greenprint "Checking for test result"
    if [[ $RESULTS == 1 ]]; then
        greenprint "💚 Success"
    else
        greenprint "❌ Failed"
        clean_up
        exit 1
    fi
}

##################################################
##
## Download and prepare Fedora IoT raw image
##
##################################################

greenprint "📥 Downloading Fedora IoT ${FEDORA_VERSION} raw image"
COMPOSE_URL="https://kojipkgs.fedoraproject.org/compose/iot/latest-Fedora-IoT-${FEDORA_VERSION}/compose/IoT/${ARCH}/images/"
RAW_IMAGE_FILENAME=$(curl -s "${COMPOSE_URL}" | grep -oP "Fedora-IoT-raw-${FEDORA_VERSION}-[^\"]+\.${ARCH}\.raw\.xz" | head -1)

if [[ -z "${RAW_IMAGE_FILENAME}" ]]; then
    echo "Failed to find raw image at ${COMPOSE_URL}"
    exit 1
fi

curl -L -o "${TEMPDIR}/${RAW_IMAGE_FILENAME}" "${COMPOSE_URL}${RAW_IMAGE_FILENAME}"

greenprint "📦 Decompressing raw image"
xz -d "${TEMPDIR}/${RAW_IMAGE_FILENAME}"
RAW_IMAGE="${TEMPDIR}/${RAW_IMAGE_FILENAME%.xz}"

greenprint "📦 Preparing raw image"
LIBVIRT_IMAGE_PATH=/var/lib/libvirt/images/${IMAGE_KEY}.raw
sudo cp "${RAW_IMAGE}" "${LIBVIRT_IMAGE_PATH}"
rm -f "${RAW_IMAGE}"
sudo qemu-img resize "${LIBVIRT_IMAGE_PATH}" +10G

greenprint "🔧 Resizing root partition"
echo ", +" | sudo sfdisk -N 3 "${LIBVIRT_IMAGE_PATH}"

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

##################################################
##
## Boot VM from raw image
##
##################################################

greenprint "🚀 Installing VM from raw image"
sudo virt-install  --name="${IMAGE_KEY}"\
                   --disk path="${LIBVIRT_IMAGE_PATH}",format=raw \
                   --ram 4096 \
                   --vcpus 2 \
                   --network network=integration,mac=34:49:22:B0:83:30 \
                   --os-variant ${OS_VARIANT} \
                   --boot ${BOOT_ARGS} \
                   --graphics none \
                   --serial file,path=${CONSOLE_LOG} \
                   --noautoconsole \
                   --wait=-1 \
                   --import \
                   --noreboot

greenprint "Start VM"
sudo virsh start "${IMAGE_KEY}"

# Check for ssh ready to go.
greenprint "🛃 Checking for SSH is ready to go"
for _ in $(seq 0 30); do
    RESULTS="$(wait_for_ssh_up $GUEST_ADDRESS)"
    if [[ $RESULTS == 1 ]]; then
        echo "SSH is ready now! 🥳"
        break
    fi
    sleep 10
done

if [[ $RESULTS != 1 ]]; then
    greenprint "SSH failed — collecting VM diagnostics"
    sudo virsh domstate "${IMAGE_KEY}" || true
    sudo virsh net-dhcp-leases integration || true
    greenprint "VM console output (last 100 lines):"
    sudo tail -100 ${CONSOLE_LOG} 2>/dev/null || true
fi
check_result

##################################################
##
## Install greenboot from Copr PR build
##
##################################################

greenprint "📦 Enabling Packit Copr repo for PR #${PR_NUMBER}"
for _ in $(seq 0 30); do
    ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
        "sudo dnf copr enable -y packit/fedora-iot-greenboot-rs-${PR_NUMBER}"
    copr_result=$?
    if [[ $copr_result == 0 ]]; then
        break
    fi
    greenprint "Copr repo not ready yet, retrying in 30s..."
    sleep 30
done

if [[ $copr_result != 0 ]]; then
    greenprint "❌ Failed to enable Copr repo after retries"
    clean_up
    exit 1
fi

greenprint "📦 Downloading greenboot RPMs from Copr"
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "dnf download --destdir /tmp/greenboot-rpms greenboot greenboot-default-health-checks"

greenprint "📦 Replacing greenboot packages with PR build"
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "sudo rpm-ostree override replace /tmp/greenboot-rpms/*.rpm"

greenprint "🔄 Rebooting to activate new deployment"
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "nohup sudo systemctl reboot &>/dev/null & exit"

sleep 30

# Check for ssh ready to go after reboot.
greenprint "🛃 Checking for SSH after greenboot upgrade reboot"
for _ in $(seq 0 30); do
    RESULTS="$(wait_for_ssh_up $GUEST_ADDRESS)"
    if [[ $RESULTS == 1 ]]; then
        echo "SSH is ready now! 🥳"
        break
    fi
    sleep 10
done

if [[ $RESULTS != 1 ]]; then
    greenprint "SSH failed after greenboot upgrade — collecting VM diagnostics"
    sudo virsh domstate "${IMAGE_KEY}" || true
    sudo virsh net-dhcp-leases integration || true
    greenprint "VM console output (last 100 lines):"
    sudo tail -100 ${CONSOLE_LOG} 2>/dev/null || true
fi
check_result

##################################################
##
## Configure greenboot and deploy test assets
##
##################################################

greenprint "🔧 Configuring greenboot on VM"
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "sudo sed -i 's/GREENBOOT_MAX_BOOT_ATTEMPTS=3/GREENBOOT_MAX_BOOT_ATTEMPTS=5/g' /etc/greenboot/greenboot.conf"
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" \
    "sudo sed -i 's#DISABLED_HEALTHCHECKS=()#DISABLED_HEALTHCHECKS=(\"01_repository_dns_check.sh\" \"not_exit.sh\")#g' /etc/greenboot/greenboot.conf"

greenprint "🛃 Copying binary and script files to VM"

# Create red.d and green.d directories if they don't exist
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "sudo mkdir -p /etc/greenboot/red.d /etc/greenboot/green.d"

# Copy all files to temp directory first
scp "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" ../testing_assets/failing_binary."${ARCH}" "${SSH_USER}@${GUEST_ADDRESS}":/tmp/failing_binary
scp "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" ../testing_assets/passing_binary."${ARCH}" "${SSH_USER}@${GUEST_ADDRESS}":/tmp/passing_binary
scp "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" ../testing_assets/failing_script.sh "${SSH_USER}@${GUEST_ADDRESS}":/tmp/
scp "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" ../testing_assets/passing_script.sh "${SSH_USER}@${GUEST_ADDRESS}":/tmp/

# Setup all directories
greenprint "🛃 Setting up red.d directory files"
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "sudo cp /tmp/failing_binary /etc/greenboot/red.d/"
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "sudo cp /tmp/passing_binary /etc/greenboot/red.d/"
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "sudo cp /tmp/failing_script.sh /etc/greenboot/red.d/"
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "sudo cp /tmp/passing_script.sh /etc/greenboot/red.d/"

greenprint "🛃 Setting up green.d directory files"
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "sudo cp /tmp/failing_binary /etc/greenboot/green.d/"
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "sudo cp /tmp/passing_binary /etc/greenboot/green.d/"
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "sudo cp /tmp/failing_script.sh /etc/greenboot/green.d/"
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "sudo cp /tmp/passing_script.sh /etc/greenboot/green.d/"

# Setup check directories
greenprint "🛃 Copying binary check files to VM"
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "sudo mv /tmp/failing_binary /etc/greenboot/check/wanted.d/"
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "sudo mv /tmp/passing_binary /etc/greenboot/check/required.d/"

# Clean up remaining temp files
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "sudo rm -f /tmp/failing_script.sh /tmp/passing_script.sh"

# Add instance IP address into /etc/ansible/hosts
tee "${TEMPDIR}"/inventory > /dev/null << EOF
[ostree_guest]
${GUEST_ADDRESS}
[ostree_guest:vars]
ansible_python_interpreter=/usr/bin/python3
ansible_user=${SSH_USER}
ansible_private_key_file=${SSH_KEY}
ansible_ssh_common_args="-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"
EOF

# Test IoT/Edge OS
ansible-playbook -v -i "${TEMPDIR}/inventory" greenboot-ostree.yaml || RESULTS=0

# Check test result
check_result

# Final success clean up
clean_up

exit 0
