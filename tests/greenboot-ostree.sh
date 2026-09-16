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
IMAGE_KEY="ostree-${TEST_UUID}"
GUEST_ADDRESS=192.168.100.50
SSH_USER="admin"
OS_NAME="rhel-edge"
IMAGE_TYPE=edge-commit
PROD_REPO_URL=http://192.168.100.1/repo
CONSOLE_LOG=/tmp/vm-console.log

# Set up temporary files.
TEMPDIR=$(mktemp -d)
BLUEPRINT_FILE=${TEMPDIR}/blueprint.toml
HTTPD_PATH="/var/www/html"
KS_FILE=${HTTPD_PATH}/ks.cfg
COMPOSE_START=${TEMPDIR}/compose-start-${IMAGE_KEY}.json
COMPOSE_INFO=${TEMPDIR}/compose-info-${IMAGE_KEY}.json
BOOT_ARGS="uefi"

# SSH setup.
SSH_OPTIONS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5)
SSH_KEY=key/ostree_key

# set locale to en_US.UTF-8
sudo dnf install -y glibc-langpack-en
sudo localectl set-locale LANG=en_US.UTF-8

# Install required packages
greenprint "Install required packages"
sudo dnf install -y --nogpgcheck httpd osbuild osbuild-composer composer-cli ansible-core podman qemu-img firewalld qemu-kvm libvirt-client libvirt-daemon-kvm libvirt-daemon virt-install lorax gobject-introspection

# Avoid collection installation filed sometime
for _ in $(seq 0 30); do
    ansible-galaxy collection install community.general community.libvirt
    install_result=$?
    if [[ $install_result == 0 ]]; then
        break
    fi
    sleep 10
done

# RPM acquisition mode:
#   If DOWNLOAD_NODE and COMPOSE_ID are both set -> download from compose
#   Otherwise -> install greenboot from Copr (default)
USE_COMPOSE_RPMS=false
if [[ -n "${DOWNLOAD_NODE:-}" && -n "${COMPOSE_ID:-}" ]]; then
    USE_COMPOSE_RPMS=true
fi
GREENBOOT_PACKAGES_URL=""
# PR_NUMBER is only needed for the Copr path; default it so `set -u` doesn't
# crash when running in compose-RPM mode without it set.
PR_NUMBER="${PR_NUMBER:-}"

# Unlike Fedora IoT / CentOS Stream Edge, RHEL nightly composes don't ship any
# enabled repos in the resulting ostree commit. Without a repo baked into the
# guest, rpm-ostree has nothing to install layered packages from once booted
# (e.g. "install tree as layered package" fails with "No enabled repositories").
# The bootc tests solve this via `COPY files/rhel-9-9.repo /etc/yum.repos.d/`
# in the Containerfile; for ostree the equivalent is a blueprint
# `[[customizations.repositories]]` entry, which osbuild-composer writes into
# /etc/yum.repos.d/ inside the built commit. Populated per-distro below; left
# empty for Fedora/CentOS, which already have working guest repos by default.
GUEST_REPO_ID=""
GUEST_REPO_BASEOS_URL=""
GUEST_REPO_APPSTREAM_URL=""

# Customize repository
sudo mkdir -p /etc/osbuild-composer/repositories

# Set os-variant and boot location used by virt-install.
case "${ID}-${VERSION_ID}" in
    "fedora-"*)
        OSTREE_REF="fedora/${VERSION_ID}/${ARCH}/iot"
        OS_NAME="fedora-iot"
        IMAGE_TYPE=iot-commit
        OS_VARIANT="fedora-unknown"
        BOOT_LOCATION="https://dl.fedoraproject.org/pub/fedora/linux/releases/${VERSION_ID}/Everything/${ARCH}/os/"
        COPR_REPO_URL="https://download.copr.fedorainfracloud.org/results/packit/fedora-iot-greenboot-rs-${PR_NUMBER}/fedora-${VERSION_ID}-${ARCH}/"
        ;;
    "centos-9")
        OSTREE_REF="centos/9/${ARCH}/edge"
        OS_VARIANT="centos-stream9"
        BOOT_ARGS="uefi,firmware.feature0.name=secure-boot,firmware.feature0.enabled=no"
        CURRENT_COMPOSE_CS9=$(curl -s "https://composes.stream.centos.org/production/" | grep -ioE ">CentOS-Stream-9-.*/<" | tr -d '>/<' | tail -1)
        BOOT_LOCATION="https://composes.stream.centos.org/production/${CURRENT_COMPOSE_CS9}/compose/BaseOS/${ARCH}/os/"
        COPR_REPO_URL="https://download.copr.fedorainfracloud.org/results/packit/fedora-iot-greenboot-rs-${PR_NUMBER}/centos-stream-9-${ARCH}/"
        sudo cp files/centos-stream-9.json /etc/osbuild-composer/repositories/centos-9.json;;
    "rhel-9.8")
        OSTREE_REF="rhel/9/${ARCH}/edge"
        OS_VARIANT="rhel9-unknown"
        { set +x; } 2>/dev/null
        BOOT_LOCATION="http://${DOWNLOAD_NODE}/rhel-9/nightly/RHEL-9/latest-RHEL-9.8.0/compose/BaseOS/${ARCH}/os/"
        COPR_REPO_URL="https://download.copr.fedorainfracloud.org/results/packit/fedora-iot-greenboot-rs-${PR_NUMBER}/centos-stream-9-${ARCH}/"
        if [[ "${USE_COMPOSE_RPMS}" == true ]]; then
            GREENBOOT_PACKAGES_URL="https://${DOWNLOAD_NODE}/rhel-9/composes/RHEL-9/${COMPOSE_ID}/compose/AppStream/${ARCH}/os/Packages/"
        fi
        GUEST_REPO_ID="rhel-9-8"
        GUEST_REPO_BASEOS_URL="${BOOT_LOCATION}"
        GUEST_REPO_APPSTREAM_URL="${BOOT_LOCATION/BaseOS/AppStream}"
        sed "s/REPLACE_ME_HERE/${DOWNLOAD_NODE}/g" files/rhel-9-8-0.json | sudo tee /etc/osbuild-composer/repositories/rhel-98.json > /dev/null
        set -x
        ;;
    "rhel-9.9")
        OSTREE_REF="rhel/9/${ARCH}/edge"
        OS_VARIANT="rhel9-unknown"
        { set +x; } 2>/dev/null
        BOOT_LOCATION="http://${DOWNLOAD_NODE}/rhel-9/nightly/RHEL-9/latest-RHEL-9.9.0/compose/BaseOS/${ARCH}/os/"
        COPR_REPO_URL="https://download.copr.fedorainfracloud.org/results/packit/fedora-iot-greenboot-rs-${PR_NUMBER}/centos-stream-9-${ARCH}/"
        if [[ "${USE_COMPOSE_RPMS}" == true ]]; then
            GREENBOOT_PACKAGES_URL="https://${DOWNLOAD_NODE}/rhel-9/composes/RHEL-9/${COMPOSE_ID}/compose/AppStream/${ARCH}/os/Packages/"
        fi
        GUEST_REPO_ID="rhel-9-9"
        GUEST_REPO_BASEOS_URL="${BOOT_LOCATION}"
        GUEST_REPO_APPSTREAM_URL="${BOOT_LOCATION/BaseOS/AppStream}"
        sed "s/REPLACE_ME_HERE/${DOWNLOAD_NODE}/g" files/rhel-9-9-0.json | sudo tee /etc/osbuild-composer/repositories/rhel-99.json > /dev/null
        set -x
        ;;
    *)
        echo "unsupported distro: ${ID}-${VERSION_ID}"
        exit 1;;
esac

# Check ostree_key permissions
KEY_PERMISSION_PRE=$(stat -L -c "%a %G %U" key/ostree_key | grep -oP '\d+' | head -n 1)
echo -e "${KEY_PERMISSION_PRE}"
if [[ "${KEY_PERMISSION_PRE}" != "600" ]]; then
   greenprint "💡 File permissions too open...Changing to 600"
   chmod 600 ./key/ostree_key
fi

# Start httpd server as prod ostree repo
greenprint "Start httpd service"
sudo systemctl enable --now httpd.service

# Start osbuild-composer.socket
greenprint "Start osbuild-composer.socket"
sudo systemctl enable --now osbuild-composer.socket

if [[ "${USE_COMPOSE_RPMS}" == true && -n "${GREENBOOT_PACKAGES_URL}" ]]; then
    # Layer in a pre-built greenboot RPM from compose instead of Copr, e.g.
    # for RHEL targets where Copr only provides a CentOS Stream approximation.
    greenprint "Downloading greenboot RPMs from compose: ${GREENBOOT_PACKAGES_URL}"
    sudo dnf install -y --nogpgcheck createrepo_c
    sudo mkdir -p /var/www/html/packages
    # /var/www/html is root-owned (created by the httpd package); hand it to
    # the current user so the unprivileged curl calls in download_compose_rpms
    # can write into it. restorecon below fixes the SELinux context afterward.
    sudo chown "$(id -u):$(id -g)" /var/www/html/packages
    # source: tests/common/download-compose-rpms.sh
    source "$(dirname "${BASH_SOURCE[0]}")/common/download-compose-rpms.sh"
    download_compose_rpms "${GREENBOOT_PACKAGES_URL}" "/var/www/html/packages"
    sudo createrepo_c /var/www/html/packages
    sudo restorecon -Rv /var/www/html/packages
    # Register the local repo with osbuild-composer so blueprints depsolve
    # picks up the compose RPMs instead of falling back to the nightly repo.
    greenprint "Adding local compose RPM repo as osbuild-composer source"
    sudo tee /tmp/greenboot-compose.toml > /dev/null << EOF
id = "greenboot-compose"
name = "Local compose greenboot RPMs"
type = "yum-baseurl"
url = "http://127.0.0.1/packages/"
check_gpg = false
check_ssl = false
EOF
    compose_source_added=false
    for _ in $(seq 0 30); do
        if sudo composer-cli sources add /tmp/greenboot-compose.toml; then
            compose_source_added=true
            break
        fi
        greenprint "Compose RPM source not ready yet, retrying in 30s..."
        sleep 30
    done

    if [ "$compose_source_added" = false ]; then
        echo "Failed to add compose RPM source after 30 attempts."
        exit 1
    fi
else
    # Add Copr repo as osbuild-composer source for greenboot PR builds
    greenprint "Adding Copr source for greenboot PR #${PR_NUMBER}"
    sudo tee /tmp/greenboot-copr.toml > /dev/null << EOF
id = "greenboot-copr"
name = "Packit Copr greenboot PR build"
type = "yum-baseurl"
url = "${COPR_REPO_URL}"
check_gpg = false
check_ssl = false
EOF
    copr_added=false
    for _ in $(seq 0 30); do
        if sudo composer-cli sources add /tmp/greenboot-copr.toml; then
            copr_added=true
            break
        fi
        greenprint "Copr source not ready yet, retrying in 30s..."
        sleep 30
    done

    if [ "$copr_added" = false ]; then
        echo "Failed to add Copr source after 30 attempts."
        exit 1
    fi
fi

# Listing greenboot as a blueprint package (version = "*") is not enough to
# guarantee it comes from the intended source (Copr or compose): dnf always
# installs the highest NEVRA across all enabled repos, and Copr snapshot
# builds conventionally use a Release starting at "0.<timestamp>...", the
# same convention official pre-GA/rebuilt packages use. Whenever
# BaseOS/AppStream ships a greenboot release that outranks the build we
# want, dnf silently installs the stock package instead. Pin the exact
# version-release dnf resolves in the source repo so there is only one
# candidate to resolve to, regardless of what other repos offer. (Verified:
# neither `composer-cli sources add` nor a blueprint's
# `[[customizations.repositories]]` priority/install_from affect depsolve at
# build time -- those only shape the .repo files written into the resulting
# image for its own future dnf use.)
if [[ "${USE_COMPOSE_RPMS}" == true && -n "${GREENBOOT_PACKAGES_URL}" ]]; then
    GREENBOOT_NEVR_LOOKUP_URL="http://127.0.0.1/packages/"
else
    GREENBOOT_NEVR_LOOKUP_URL="${COPR_REPO_URL}"
fi

greenprint "Looking up exact greenboot NEVR to pin from ${GREENBOOT_NEVR_LOOKUP_URL}"
GREENBOOT_NEVR=""
for _ in $(seq 0 30); do
    GREENBOOT_NEVR=$(sudo dnf repoquery \
        --repofrompath="greenboot-nevr-lookup,${GREENBOOT_NEVR_LOOKUP_URL}" \
        --disablerepo='*' --enablerepo=greenboot-nevr-lookup \
        --quiet --qf '%{version}-%{release}' --latest-limit=1 greenboot || true)
    if [ -n "$GREENBOOT_NEVR" ]; then
        break
    fi
    greenprint "Copr metadata not ready yet, retrying NEVR lookup in 30s..."
    sleep 30
done

if [ -z "$GREENBOOT_NEVR" ]; then
    echo "Failed to resolve greenboot version-release from repo ${GREENBOOT_NEVR_LOOKUP_URL}"
    exit 1
fi
greenprint "Pinning greenboot to build ${GREENBOOT_NEVR}"

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

# Basic weldr API status checking
sudo composer-cli status show

# Source checking
sudo composer-cli sources list
for SOURCE in $(sudo composer-cli sources list); do
    sudo composer-cli sources info "$SOURCE"
done

# Get the compose log.
get_compose_log () {
    COMPOSE_ID=$1
    LOG_FILE=osbuild-${ID}-${VERSION_ID}-${COMPOSE_ID}.log

    # Download the logs.
    sudo composer-cli compose log "$COMPOSE_ID" | tee "$LOG_FILE" > /dev/null
}

# Get the compose metadata.
get_compose_metadata () {
    COMPOSE_ID=$1
    METADATA_FILE=osbuild-${ID}-${VERSION_ID}-${COMPOSE_ID}.json

    # Download the metadata.
    sudo composer-cli compose metadata "$COMPOSE_ID" > /dev/null

    # Find the tarball and extract it.
    TARBALL=$(basename "$(find . -maxdepth 1 -type f -name "*-metadata.tar")")
    sudo tar -xf "$TARBALL" -C "${TEMPDIR}"
    sudo rm -f "$TARBALL"

    # Move the JSON file into place.
    sudo cat "${TEMPDIR}"/"${COMPOSE_ID}".json | jq -M '.' | tee "$METADATA_FILE" > /dev/null
}

# Build ostree image.
build_image() {
    blueprint_file=$1
    blueprint_name=$2

    # Prepare the blueprint for the compose.
    greenprint "📋 Preparing blueprint"
    sudo composer-cli blueprints push "$blueprint_file"
    sudo composer-cli blueprints depsolve "$blueprint_name" | tee /tmp/depsolve-output.txt

    # Verify greenboot is being pulled from Copr (should have PR-specific NVR)
    greenprint "🔍 Verifying greenboot source"
    if grep -i "greenboot" /tmp/depsolve-output.txt; then
        greenprint "✅ greenboot package found in depsolve output"
    else
        greenprint "⚠️  greenboot not explicitly in depsolve (may be in base image)"
    fi

    # Get worker unit file so we can watch the journal.
    WORKER_UNIT=$(sudo systemctl list-units | grep -o -E "osbuild.*worker.*\.service")
    sudo journalctl -af -n 1 -u "${WORKER_UNIT}" &
    WORKER_JOURNAL_PID=$!

    # Start the compose.
    greenprint "🚀 Starting compose"
    if [[ $blueprint_name == upgrade ]]; then
        # composer-cli in Fedora 32 has a different start-ostree arguments
        # see https://github.com/weldr/lorax/pull/1051
        sudo composer-cli --json compose start-ostree --ref "$OSTREE_REF" "$blueprint_name" $IMAGE_TYPE | tee "$COMPOSE_START"
    else
        sudo composer-cli --json compose start "$blueprint_name" $IMAGE_TYPE | tee "$COMPOSE_START"
    fi

    COMPOSE_ID=$(jq -r '.[0].body.build_id' "$COMPOSE_START")

    # Wait for the compose to finish.
    greenprint "⏱ Waiting for compose to finish: ${COMPOSE_ID}"
    while true; do
        sudo composer-cli --json compose info "${COMPOSE_ID}" | tee "$COMPOSE_INFO" > /dev/null

        # Handle both v1 (CentOS/RHEL) and v2 (Fedora) API formats
        COMPOSE_STATUS=$(jq -r '.[].body | (.queue_status // .image_status?.status) | select(. != null)' "$COMPOSE_INFO")

        # Is the compose finished?
        # v1: RUNNING/WAITING/FINISHED/FAILED  v2: building/pending/success/failure
        if [[ $COMPOSE_STATUS != RUNNING ]] && [[ $COMPOSE_STATUS != WAITING ]] \
            && [[ $COMPOSE_STATUS != building ]] && [[ $COMPOSE_STATUS != pending ]]; then
            break
        fi

        # Wait 30 seconds and try again.
        sleep 5
    done

    # Capture the compose logs from osbuild.
    greenprint "💬 Getting compose log and metadata"
    get_compose_log "$COMPOSE_ID"
    get_compose_metadata "$COMPOSE_ID"

    # Did the compose finish with success?
    if [[ $COMPOSE_STATUS != FINISHED ]] && [[ $COMPOSE_STATUS != success ]]; then
        echo "Something went wrong with the compose. 😢"
        exit 1
    fi

    # Stop watching the worker journal.
    sudo pkill -P ${WORKER_JOURNAL_PID}
}

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
    # Remove qcow2 file.
    sudo virsh vol-delete --pool images "${IMAGE_KEY}.qcow2"
    # Remove "remote" repo.
    sudo rm -rf "${HTTPD_PATH}"/{httpboot,repo,compose.json,ks.cfg}
    # Remomve tmp dir.
    sudo rm -rf "$TEMPDIR"
    # Stop httpd
    sudo systemctl disable httpd --now
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
## ostree image/commit installation
##
##################################################

# Write a blueprint for ostree image.
tee "$BLUEPRINT_FILE" > /dev/null << EOF
name = "ostree"
description = "A base ostree image"
version = "0.0.1"
modules = []
groups = []

[[packages]]
name = "python3"
version = "*"

[[packages]]
name = "sssd"
version = "*"

[[packages]]
name = "greenboot"
version = "${GREENBOOT_NEVR}"

[[packages]]
name = "greenboot-default-health-checks"
version = "${GREENBOOT_NEVR}"

[customizations.services]
enabled = ["greenboot-healthcheck.service", "greenboot-set-rollback-trigger.service", "greenboot-success.target"]

[[customizations.user]]
name = "${SSH_USER}"
description = "Administrator account"
password = "\$6\$GRmb7S0p8vsYmXzH\$o0E020S.9JQGaHkszoog4ha4AQVs3sk8q0DvLjSMxoxHBKnB2FBXGQ/OkwZQfW/76ktHd0NX5nls2LPxPuUdl."
key = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQCzxo5dEcS+LDK/OFAfHo6740EyoDM8aYaCkBala0FnWfMMTOq7PQe04ahB0eFLS3IlQtK5bpgzxBdFGVqF6uT5z4hhaPjQec0G3+BD5Pxo6V+SxShKZo+ZNGU3HVrF9p2V7QH0YFQj5B8F6AicA3fYh2BVUFECTPuMpy5A52ufWu0r4xOFmbU7SIhRQRAQz2u4yjXqBsrpYptAvyzzoN4gjUhNnwOHSPsvFpWoBFkWmqn0ytgHg3Vv9DlHW+45P02QH1UFedXR2MqLnwRI30qqtaOkVS+9rE/dhnR+XPpHHG+hv2TgMDAuQ3IK7Ab5m/yCbN73cxFifH4LST0vVG3Jx45xn+GTeHHhfkAfBSCtya6191jixbqyovpRunCBKexI5cfRPtWOitM3m7Mq26r7LpobMM+oOLUm4p0KKNIthWcmK9tYwXWSuGGfUQ+Y8gt7E0G06ZGbCPHOrxJ8lYQqXsif04piONPA/c9Hq43O99KPNGShONCS9oPFdOLRT3U= ostree-image-test"
home = "/home/${SSH_USER}/"
groups = ["wheel"]
EOF

# For distros with no working guest repos out of the box (see comment near
# GUEST_REPO_ID above), embed the base repos into the ostree commit so
# rpm-ostree can install layered packages once the guest is booted.
if [[ -n "${GUEST_REPO_BASEOS_URL}" ]]; then
    { set +x; } 2>/dev/null
    tee -a "$BLUEPRINT_FILE" > /dev/null << EOF

[[customizations.repositories]]
id = "${GUEST_REPO_ID}-baseos"
filename = "${GUEST_REPO_ID}.repo"
baseurls = ["${GUEST_REPO_BASEOS_URL}"]
gpgcheck = false
enabled = true

[[customizations.repositories]]
id = "${GUEST_REPO_ID}-appstream"
filename = "${GUEST_REPO_ID}.repo"
baseurls = ["${GUEST_REPO_APPSTREAM_URL}"]
gpgcheck = false
enabled = true
EOF
    set -x
fi

# Build installation image.
build_image "$BLUEPRINT_FILE" ostree

# Download the image and extract tar into web server root folder.
greenprint "📥 Downloading and extracting the image"
sudo composer-cli compose image "${COMPOSE_ID}" > /dev/null
IMAGE_FILENAME="${COMPOSE_ID}-commit.tar"
sudo tar -xf "${IMAGE_FILENAME}" -C ${HTTPD_PATH}
sudo rm -f "$IMAGE_FILENAME"

# Clean compose and blueprints.
greenprint "Clean up osbuild-composer"
sudo composer-cli compose delete "${COMPOSE_ID}" > /dev/null
sudo composer-cli blueprints delete ostree > /dev/null

# Ensure SELinux is happy with our new images.
greenprint "👿 Running restorecon on image directory"
sudo restorecon -Rv /var/lib/libvirt/images/

# Create qcow2 file for virt install.
greenprint "Create qcow2 file for virt install"
LIBVIRT_IMAGE_PATH=/var/lib/libvirt/images/${IMAGE_KEY}.qcow2
sudo qemu-img create -f qcow2 "${LIBVIRT_IMAGE_PATH}" 20G

# Write kickstart file for ostree image installation.
greenprint "Generate kickstart file"
sudo tee "$KS_FILE" > /dev/null << STOPHERE
text
rootpw --lock --iscrypted locked
user --name=${SSH_USER} --groups=wheel --iscrypted --password=\$6\$GRmb7S0p8vsYmXzH\$o0E020S.9JQGaHkszoog4ha4AQVs3sk8q0DvLjSMxoxHBKnB2FBXGQ/OkwZQfW/76ktHd0NX5nls2LPxPuUdl.
network --bootproto=dhcp --device=link --activate --onboot=on
zerombr
clearpart --all --initlabel --disklabel=gpt
autopart --nohome --noswap --type=plain
ostreesetup --nogpg --osname=${OS_NAME} --remote=${OS_NAME} --url=${PROD_REPO_URL} --ref=${OSTREE_REF}
poweroff

%post --log=/var/log/anaconda/post-install.log --erroronfail
# no sudo password for SSH user
echo -e '${SSH_USER}\tALL=(ALL)\tNOPASSWD: ALL' >> /etc/sudoers
%end
STOPHERE

# Install ostree image via anaconda.
greenprint "Install ostree image via anaconda"
sudo virt-install  --name="${IMAGE_KEY}"\
                   --initrd-inject="${KS_FILE}" \
                   --extra-args="inst.ks=file:/ks.cfg console=ttyS0,115200" \
                   --disk path="${LIBVIRT_IMAGE_PATH}",format=qcow2 \
                   --ram 8192 \
                   --vcpus 2 \
                   --network network=integration,mac=34:49:22:B0:83:30 \
                   --os-variant ${OS_VARIANT} \
                   --boot ${BOOT_ARGS} \
                   --tpm none \
                   --location "${BOOT_LOCATION}" \
                   --graphics none \
                   --serial file,path=${CONSOLE_LOG} \
                   --noautoconsole \
                   --wait=-1 \
                   --noreboot

# Start VM.
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
    greenprint "SSH failed on initial boot — collecting VM diagnostics"
    sudo virsh domstate "${IMAGE_KEY}" || true
    sudo virsh net-dhcp-leases integration || true
    greenprint "VM console output (last 100 lines):"
    sudo tail -100 ${CONSOLE_LOG} 2>/dev/null || true
fi
check_result

greenprint "🛃 Copying binary and script files to edge vm"

# Create red.d and green.d directories if they don't exist
ssh "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "${SSH_USER}@${GUEST_ADDRESS}" "sudo mkdir -p /etc/greenboot/red.d /etc/greenboot/green.d"

# Copy all files to temp directory first (all at once)
scp "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "../testing_assets/failing_binary.${ARCH}" "${SSH_USER}@${GUEST_ADDRESS}":/tmp/failing_binary && \
scp "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" "../testing_assets/passing_binary.${ARCH}" "${SSH_USER}@${GUEST_ADDRESS}":/tmp/passing_binary && \
scp "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" ../testing_assets/failing_script.sh "${SSH_USER}@${GUEST_ADDRESS}":/tmp/ && \
scp "${SSH_OPTIONS[@]}" -i "${SSH_KEY}" ../testing_assets/passing_script.sh "${SSH_USER}@${GUEST_ADDRESS}":/tmp/

# Setup all directories using cp (copy) instead of mv (move) so files stay in /tmp/
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

# Setup original check directories (keeping existing behavior)
greenprint "🛃 Copying binary check files to edge vm"
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

# Check image installation result
check_result

# Final success clean up
clean_up

exit 0
