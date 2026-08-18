#!/bin/bash
set -euox pipefail

ARCH=$(uname -m)

# Dumps details about the instance running the CI job.
echo -e "\033[0;36m"
cat << EOF
------------------------------------------------------------------------------
CI MACHINE SPECS
------------------------------------------------------------------------------
     Hostname: $(uname -n)
         User: $(whoami)
         CPUs: $(nproc)
          RAM: $(free -m | grep -oP '\d+' | head -n 1) MB
         DISK: $(df --output=size -h / | sed '1d;s/[^0-9]//g') GB
         ARCH: $(uname -m)
       KERNEL: $(uname -r)
------------------------------------------------------------------------------
EOF
echo -e "\033[0m"

# Get OS info
source /etc/os-release
if [[ -n "${TARGET_DISTRO:-}" ]]; then
    IFS='-' read -r ID VERSION_ID <<< "${TARGET_DISTRO}"
fi

# Setup variables
ARCH=$(uname -m)
TEST_UUID=anaconda-$((1 + RANDOM % 1000000))
TEMPDIR=$(mktemp -d)
GUEST_ADDRESS=192.168.100.50
SSH_OPTIONS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5)
SSH_KEY=key/ostree_key
SSH_KEY_PUB=$(cat "${SSH_KEY}".pub)
EDGE_USER=core
EDGE_USER_PASSWORD=foobar
CONSOLE_LOG=/tmp/vm-console.log

COPR_CHROOT=""

# RPM acquisition mode:
#   If DOWNLOAD_NODE and COMPOSE_ID are both set -> download from compose
#   Otherwise -> install greenboot from Copr (default)
USE_COMPOSE_RPMS=false
if [[ -n "${DOWNLOAD_NODE:-}" && -n "${COMPOSE_ID:-}" ]]; then
    USE_COMPOSE_RPMS=true
fi
GREENBOOT_PACKAGES_URL=""

case "${ID}-${VERSION_ID}" in
    "fedora-44")
        OS_VARIANT="fedora-unknown"
        BASE_IMAGE_URL="quay.io/fedora/fedora-iot:44"
        BIB_URL="quay.io/centos-bootc/bootc-image-builder:latest"
        BOOT_ARGS="uefi"
        ;;
    "fedora-45")
        OS_VARIANT="fedora-unknown"
        BASE_IMAGE_URL="quay.io/fedora/fedora-iot:45"
        BIB_URL="quay.io/centos-bootc/bootc-image-builder:latest"
        BOOT_ARGS="uefi"
        ;;
    "fedora-rawhide")
        OS_VARIANT="fedora-rawhide"
        BASE_IMAGE_URL="quay.io/fedora/fedora-iot:rawhide"
        BIB_URL="quay.io/centos-bootc/bootc-image-builder:latest"
        BOOT_ARGS="uefi"
        COPR_CHROOT="fedora-rawhide-${ARCH}"
        ;;
    "centos-10")
        OS_VARIANT="centos-stream9"
        BASE_IMAGE_URL="quay.io/centos-bootc/centos-bootc:stream10"
        BIB_URL="quay.io/centos-bootc/bootc-image-builder:latest"
        BOOT_ARGS="uefi,firmware.feature0.name=secure-boot,firmware.feature0.enabled=no"
        COPR_CHROOT="centos-stream-10-${ARCH}"
        ;;
    "rhel-9.8")
        OS_VARIANT="rhel9-unknown"
        BASE_IMAGE_URL="registry.stage.redhat.io/rhel9/rhel-bootc:9.8"
        BIB_URL="registry.stage.redhat.io/rhel9/bootc-image-builder:9.8"
        BOOT_ARGS="uefi"
        COPR_CHROOT="centos-stream-9-${ARCH}"
        { set +x; } 2>/dev/null
        sed -i "s/REPLACE_ME_HERE/${DOWNLOAD_NODE}/g" files/rhel-9-8.repo
        sed -i "s/REPLACE_ARCH_HERE/${ARCH}/g" files/rhel-9-8.repo
        if [[ "${USE_COMPOSE_RPMS}" == true ]]; then
            GREENBOOT_PACKAGES_URL="https://${DOWNLOAD_NODE}/rhel-9/composes/RHEL-9/${COMPOSE_ID}/compose/AppStream/${ARCH}/os/Packages/"
        fi
        set -x
        ;;
    "rhel-9.9")
        OS_VARIANT="rhel9-unknown"
        BASE_IMAGE_URL="registry.stage.redhat.io/rhel9/rhel-bootc:9.9"
        BIB_URL="registry.stage.redhat.io/rhel9/bootc-image-builder:9.9"
        BOOT_ARGS="uefi"
        COPR_CHROOT="centos-stream-9-${ARCH}"
        { set +x; } 2>/dev/null
        sed -i "s/REPLACE_ME_HERE/${DOWNLOAD_NODE}/g" files/rhel-9-9.repo
        sed -i "s/REPLACE_ARCH_HERE/${ARCH}/g" files/rhel-9-9.repo
        if [[ "${USE_COMPOSE_RPMS}" == true ]]; then
            GREENBOOT_PACKAGES_URL="https://${DOWNLOAD_NODE}/rhel-9/composes/RHEL-9/${COMPOSE_ID}/compose/AppStream/${ARCH}/os/Packages/"
        fi
        set -x
        ;;
    "rhel-10.2")
        OS_VARIANT="rhel10-unknown"
        BASE_IMAGE_URL="registry.stage.redhat.io/rhel10/rhel-bootc:10.2"
        BIB_URL="registry.stage.redhat.io/rhel10/bootc-image-builder:10.2"
        BOOT_ARGS="uefi"
        COPR_CHROOT="centos-stream-10-${ARCH}"
        { set +x; } 2>/dev/null
        sed -i "s/REPLACE_ME_HERE/${DOWNLOAD_NODE}/g" files/rhel-10-2.repo
        sed -i "s/REPLACE_ARCH_HERE/${ARCH}/g" files/rhel-10-2.repo
        if [[ "${USE_COMPOSE_RPMS}" == true ]]; then
            GREENBOOT_PACKAGES_URL="https://${DOWNLOAD_NODE}/rhel-10/composes/RHEL-10/${COMPOSE_ID}/compose/AppStream/${ARCH}/os/Packages/"
        fi
        set -x
        ;;
    "rhel-10.3")
        OS_VARIANT="rhel10-unknown"
        BASE_IMAGE_URL="registry.stage.redhat.io/rhel10/rhel-bootc:10.3"
        BIB_URL="registry.stage.redhat.io/rhel10/bootc-image-builder:10.3"
        BOOT_ARGS="uefi"
        COPR_CHROOT="centos-stream-10-${ARCH}"
        { set +x; } 2>/dev/null
        sed -i "s/REPLACE_ME_HERE/${DOWNLOAD_NODE}/g" files/rhel-10-3.repo
        sed -i "s/REPLACE_ARCH_HERE/${ARCH}/g" files/rhel-10-3.repo
        if [[ "${USE_COMPOSE_RPMS}" == true ]]; then
            GREENBOOT_PACKAGES_URL="https://${DOWNLOAD_NODE}/rhel-10/composes/RHEL-10/${COMPOSE_ID}/compose/AppStream/${ARCH}/os/Packages/"
        fi
        set -x
        ;;
    *)
        echo "unsupported distro: ${ID}-${VERSION_ID}"
        exit 1;;
esac

# Colorful output.
function greenprint {
    echo -e "\033[1;32m${1}\033[0m"
}

check_result () {
    greenprint "🎏 Checking for test result"
    if [[ $RESULTS == 1 ]]; then
        greenprint "💚 Success"
    else
        greenprint "❌ Failed"
        exit 1
    fi
}

# Wait for the ssh server up to be.
wait_for_ssh_up () {
    SSH_STATUS=$(sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" ${EDGE_USER}@"${1}" '/bin/bash -c "echo -n READY"')
    if [[ $SSH_STATUS == READY ]]; then
        echo 1
    else
        echo 0
    fi
}

###########################################################
##
## Prepare before run test
##
###########################################################
greenprint "Installing required packages"
sudo dnf install -y podman qemu-img firewalld qemu-kvm libvirt-client libvirt-daemon-kvm libvirt-daemon virt-install ansible-core lorax gobject-introspection
ansible-galaxy collection install community.general

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

# Check ostree_key permissions
KEY_PERMISSION_PRE=$(stat -L -c "%a %G %U" key/ostree_key | grep -oP '\d+' | head -n 1)
echo -e "${KEY_PERMISSION_PRE}"
if [[ "${KEY_PERMISSION_PRE}" != "600" ]]; then
   greenprint "💡 File permissions too open...Changing to 600"
   chmod 600 ./key/ostree_key
fi

# Setup libvirt
greenprint "Starting libvirt service and configure libvirt network"
sudo tee /etc/polkit-1/rules.d/50-libvirt.rules > /dev/null << EOF
polkit.addRule(function(action, subject) {
    if (action.id == "org.libvirt.unix.manage" &&
        subject.isInGroup("adm")) {
            return polkit.Result.YES;
    }
});
EOF
sudo systemctl start libvirtd
sudo virsh list --all > /dev/null
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
      <host mac='34:49:22:B0:83:31' name='vm-2' ip='192.168.100.51'/>
      <host mac='34:49:22:B0:83:32' name='vm-3' ip='192.168.100.52'/>
    </dhcp>
  </ip>
  <dnsmasq:options>
    <dnsmasq:option value='dhcp-vendorclass=set:efi-http,HTTPClient:Arch:00016'/>
    <dnsmasq:option value='dhcp-option-force=tag:efi-http,60,HTTPClient'/>
    <dnsmasq:option value='dhcp-boot=tag:efi-http,&quot;http://192.168.100.1/httpboot/EFI/BOOT/BOOTX64.EFI&quot;'/>
  </dnsmasq:options>
</network>
EOF
if ! sudo virsh net-info integration > /dev/null 2>&1; then
    sudo virsh net-define /tmp/integration.xml
fi
if [[ $(sudo virsh net-info integration | grep 'Active' | awk '{print $2}') == 'no' ]]; then
    sudo virsh net-start integration
fi

###########################################################
##
## Copy test assets
##
###########################################################
greenprint "Copying test assets"
(
    cd ..
    cp testing_assets/passing_script.sh tests/
    cp testing_assets/passing_binary tests/
    cp testing_assets/failing_script.sh tests/
    cp testing_assets/failing_binary tests/
)

###########################################################
##
## Optionally download greenboot rpm packages from compose
##
###########################################################
if [[ "${USE_COMPOSE_RPMS}" == true && -n "${GREENBOOT_PACKAGES_URL}" ]]; then
    greenprint "Downloading greenboot RPMs from compose"
    rm -f greenboot-*.rpm
    # source: tests/common/download-compose-rpms.sh
    source "$(dirname "${BASH_SOURCE[0]}")/common/download-compose-rpms.sh"
    download_compose_rpms "${GREENBOOT_PACKAGES_URL}" "."
fi

###########################################################
##
## Build bootc container with greenboot installed
##
###########################################################
greenprint "Building bootc container with greenboot installed"
podman login quay.io -u ${QUAY_USERNAME} -p ${QUAY_PASSWORD}
podman login registry.stage.redhat.io -u ${STAGE_REDHAT_IO_USERNAME} -p ${STAGE_REDHAT_IO_TOKEN}
tee Containerfile > /dev/null << EOF
FROM ${BASE_IMAGE_URL}
EOF

# RHEL repo is always needed: Copr path uses it for dnf deps,
# anaconda-iso BIB uses it for depsolve
case "${ID}-${VERSION_ID}" in
    "rhel-9.8")
        tee -a Containerfile > /dev/null << EOF
COPY files/rhel-9-8.repo /etc/yum.repos.d/rhel-9-8.repo
EOF
        ;;
    "rhel-9.9")
        tee -a Containerfile > /dev/null << EOF
COPY files/rhel-9-9.repo /etc/yum.repos.d/rhel-9-9.repo
EOF
        ;;
    "rhel-10.2")
        tee -a Containerfile > /dev/null << EOF
COPY files/rhel-10-2.repo /etc/yum.repos.d/rhel-10-2.repo
EOF
        ;;
    "rhel-10.3")
        tee -a Containerfile > /dev/null << EOF
COPY files/rhel-10-3.repo /etc/yum.repos.d/rhel-10-3.repo
EOF
        ;;
esac

if [[ "${USE_COMPOSE_RPMS}" == true && -n "${GREENBOOT_PACKAGES_URL}" ]]; then
    tee -a Containerfile > /dev/null << EOF
COPY greenboot-*.rpm /tmp/
RUN dnf install -y /tmp/greenboot-*.rpm && \
    rm -f /tmp/greenboot-*.rpm && \
    systemctl enable greenboot-healthcheck.service
EOF
else
    tee -a Containerfile > /dev/null << EOF
RUN (dnf install -y 'dnf5-command(copr)' || dnf install -y 'dnf-command(copr)') && \
    dnf copr enable -y packit/fedora-iot-greenboot-rs-${PR_NUMBER} ${COPR_CHROOT} && \
    dnf clean metadata && \
    (dnf reinstall -y greenboot greenboot-default-health-checks || dnf install -y greenboot greenboot-default-health-checks) && \
    systemctl enable greenboot-healthcheck.service
EOF
fi

tee -a Containerfile > /dev/null << EOF
RUN sed -i "s/GREENBOOT_MAX_BOOT_ATTEMPTS=3/GREENBOOT_MAX_BOOT_ATTEMPTS=5/g" /etc/greenboot/greenboot.conf
RUN sed -i 's#DISABLED_HEALTHCHECKS=()#DISABLED_HEALTHCHECKS=("01_repository_dns_check.sh" "not_exit.sh")#g' /etc/greenboot/greenboot.conf

COPY passing_script.sh /etc/greenboot/green.d
COPY passing_binary /etc/greenboot/green.d/
COPY failing_binary /etc/greenboot/green.d
COPY failing_script.sh /etc/greenboot/green.d

COPY passing_script.sh /etc/greenboot/red.d
COPY passing_binary /etc/greenboot/red.d/
COPY failing_binary /etc/greenboot/red.d
COPY failing_script.sh /etc/greenboot/red.d

COPY passing_binary /etc/greenboot/check/required.d/
COPY failing_binary /etc/greenboot/check/wanted.d/

RUN useradd -m -G wheel ${EDGE_USER} && echo '${EDGE_USER}:${EDGE_USER_PASSWORD}' | chpasswd
RUN mkdir -p /home/${EDGE_USER}/.ssh && \
    echo '${SSH_KEY_PUB}' > /home/${EDGE_USER}/.ssh/authorized_keys && \
    chmod 700 /home/${EDGE_USER}/.ssh && \
    chmod 600 /home/${EDGE_USER}/.ssh/authorized_keys && \
    chown -R ${EDGE_USER}:${EDGE_USER} /home/${EDGE_USER}/.ssh
EOF

greenprint "Building container (retrying until Copr build is available)"
build_success=false
for attempt in $(seq 1 10); do
    if podman build --retry=5 --retry-delay=10s -t quay.io/${QUAY_USERNAME}/greenboot-bootc:${TEST_UUID} -f Containerfile .; then
        build_success=true
        break
    fi
    greenprint "Container build attempt ${attempt}/10 failed, retrying in 60s..."
    sleep 60
done

if [ "$build_success" = false ]; then
    echo "Container build failed after 10 attempts."
    exit 1
fi

greenprint "Pushing bootc container to quay.io"
podman push quay.io/${QUAY_USERNAME}/greenboot-bootc:${TEST_UUID}

###########################################################
##
## BIB to convert bootc container to qcow2/iso images
##
###########################################################
greenprint "Using BIB to convert container to anaconda iso"
tee config.json > /dev/null << EOF
{
  "customizations": {
    "installer": {
      "kickstart": {
        "contents": "text --non-interactive\nzerombr\nclearpart --all --initlabel --disklabel=gpt\nautopart --nohome --type=plain --fstype=xfs\nnetwork --bootproto=dhcp --device=link --activate --onboot=on\nbootloader --append=\"console=ttyS0,115200\"\npoweroff\n"
      },
      "modules": {
        "disable": [
          "org.fedoraproject.Anaconda.Modules.Users"
        ]
      }
    }
  }
}
EOF
sudo rm -fr output && mkdir -p output
podman run \
    --rm \
    -it \
    --privileged \
    --pull=newer \
    --security-opt label=type:unconfined_t \
    -v $(pwd)/config.json:/config.json \
    -v $(pwd)/output:/output \
    -v /var/lib/containers/storage:/var/lib/containers/storage \
    ${BIB_URL} \
    --type anaconda-iso \
    --config /config.json \
    --rootfs xfs \
    --use-librepo=true \
    quay.io/${QUAY_USERNAME}/greenboot-bootc:${TEST_UUID}

greenprint "Copying BIB ISO to libvirt images"
sudo cp output/bootiso/install.iso /var/lib/libvirt/images/install.iso

###########################################################
##
## Provision vm with qcow2/iso artifacts
##
###########################################################
greenprint "Installing vm with bootc anaconda iso image"
greenprint "💾 Create vm qcow2 files for ISO installation"
LIBVIRT_IMAGE_PATH_UEFI=/var/lib/libvirt/images/${TEST_UUID}-disk.qcow2
sudo qemu-img create -f qcow2 "/var/lib/libvirt/images/${TEST_UUID}-disk.qcow2" 10G
sudo restorecon -Rv /var/lib/libvirt/images/
ISO_LABEL=$(blkid -o value -s LABEL /var/lib/libvirt/images/install.iso)
[[ -n "$ISO_LABEL" ]] || { greenprint "❌ ISO label is empty"; exit 1; }
greenprint "ISO label: ${ISO_LABEL}"
sudo virt-install  --name="${TEST_UUID}-uefi"\
                   --disk path="${LIBVIRT_IMAGE_PATH_UEFI}",format=qcow2 \
                   --ram 8192 \
                   --vcpus 2 \
                   --network network=integration,mac=34:49:22:B0:83:30 \
                   --os-type linux \
                   --os-variant ${OS_VARIANT} \
                   --location "/var/lib/libvirt/images/install.iso",kernel=images/pxeboot/vmlinuz,initrd=images/pxeboot/initrd.img \
                   --extra-args "console=ttyS0,115200 inst.stage2=hd:LABEL=${ISO_LABEL} inst.ks=hd:LABEL=${ISO_LABEL}:/osbuild.ks" \
                   --boot ${BOOT_ARGS} \
                   --tpm none \
                   --graphics none \
                   --serial file,path=${CONSOLE_LOG} \
                   --noautoconsole \
                   --wait=-1 \
                   --noreboot
greenprint "Starting UEFI VM"
sudo virsh start "${TEST_UUID}-uefi"

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
    greenprint "SSH failed on initial boot — collecting VM diagnostics"
    sudo virsh domstate "${TEST_UUID}-uefi" || true
    sudo virsh domiflist "${TEST_UUID}-uefi" || true
    sudo virsh net-dhcp-leases integration || true
    sudo ip addr show integration || true
    greenprint "Firewall zone for integration bridge:"
    sudo firewall-cmd --get-zone-of-interface=integration || true
    greenprint "Connectivity test:"
    ping -c 2 -W 2 192.168.100.50 || true
    greenprint "VM console output (last 100 lines):"
    sudo tail -100 ${CONSOLE_LOG} 2>/dev/null || true
fi
check_result

###########################################################
##
## Build upgrade container with failing-unit installed
##
###########################################################
greenprint "Building upgrade container"
tee Containerfile > /dev/null << EOF
FROM quay.io/${QUAY_USERNAME}/greenboot-bootc:${TEST_UUID}
RUN dnf install -y https://kite-webhook-prod.s3.amazonaws.com/greenboot-failing-unit-1.0-1.el8.noarch.rpm
EOF
podman build  --retry=5 --retry-delay=10s -t quay.io/${QUAY_USERNAME}/greenboot-bootc:${TEST_UUID} -f Containerfile .
greenprint "Pushing upgrade container to quay.io"
podman push quay.io/${QUAY_USERNAME}/greenboot-bootc:${TEST_UUID}

###########################################################
##
## Bootc upgrade and check if greenboot can rollback
##
###########################################################
greenprint "Get /boot mount status"
BOOT_MOUNT_STATUS=$(ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" ${EDGE_USER}@${GUEST_ADDRESS} "findmnt -r -o OPTIONS -n /boot | awk -F ',' '{print \$1}'")
greenprint "Bootc upgrade and reboot"
sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" ${EDGE_USER}@${GUEST_ADDRESS} "echo ${EDGE_USER_PASSWORD} |sudo -S bootc upgrade"
sudo ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" ${EDGE_USER}@${GUEST_ADDRESS} "echo ${EDGE_USER_PASSWORD} |nohup sudo -S systemctl reboot &>/dev/null & exit"

# Wait vm to finish the fallback
sleep 300

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
    greenprint "SSH failed after upgrade — collecting VM diagnostics"
    sudo virsh domstate "${TEST_UUID}-uefi" || true
    sudo virsh net-dhcp-leases integration || true
    greenprint "VM console output (last 100 lines):"
    sudo tail -100 ${CONSOLE_LOG} 2>/dev/null || true
fi
check_result

# Add instance IP address into /etc/ansible/hosts
tee ${TEMPDIR}/inventory > /dev/null << EOF
[greenboot_guest]
${GUEST_ADDRESS}

[greenboot_guest:vars]
ansible_python_interpreter=/usr/bin/python3
ansible_user=${EDGE_USER}
ansible_private_key_file=${SSH_KEY}
ansible_ssh_common_args="-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"
ansible_become=yes
ansible_become_method=sudo
ansible_become_pass=${EDGE_USER_PASSWORD}
EOF

# Test greenboot functionality
ansible-playbook -v -i /${TEMPDIR}/inventory -e boot_mount_status="${BOOT_MOUNT_STATUS}" greenboot-bootc.yaml || RESULTS=0

# Test result checking
check_result
exit 0
