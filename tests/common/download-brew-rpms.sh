#!/bin/bash
# Source this file to get the download_brew_rpms helper function.

# download_brew_rpms <greenboot_url> <health_checks_url> <dest_dir>
#
# Download two exact RPM URLs without listing a compose or Brew directory.
# Preserve each URL's filename, including its version, release and architecture.
# Validate package names, matching epoch/version/release and host architecture
# before moving either RPM into the destination.
#
# -k accommodates internal Brew certificates whose CA is not trusted by CI hosts.
download_brew_rpms() (
    # Keep tracing changes and temporary-file cleanup local to this helper.
    set -euo pipefail
    { set +x; } 2>/dev/null

    if [[ $# -ne 3 ]]; then
        echo "ERROR: expected two RPM URLs and a destination directory" >&2
        return 1
    fi

    local dest_dir="$3"
    local arch
    arch=$(uname -m)
    local -a urls=("$1" "$2")
    local -a packages=(greenboot greenboot-default-health-checks)
    local -a filenames=()
    local url filename
    for url in "${urls[@]}"; do
        case "${url}" in
            http://?*|https://?*) ;;
            *)
                echo "ERROR: Brew RPM URLs must use HTTP or HTTPS" >&2
                return 1
                ;;
        esac
        # Query strings and fragments are not part of the published filename.
        filename="${url%%[?#]*}"
        filename="${filename##*/}"
        if [[ ! "${filename}" =~ ^greenboot-[a-zA-Z0-9._+-]+\.rpm$ ]]; then
            echo "ERROR: expected a direct URL with a greenboot RPM filename" >&2
            return 1
        fi
        filenames+=("${filename}")
    done
    if [[ "${filenames[0]}" == "${filenames[1]}" ]]; then
        echo "ERROR: the two RPM URLs must have distinct filenames" >&2
        return 1
    fi

    mkdir -p -- "${dest_dir}" || return 1
    local download_dir
    download_dir=$(mktemp -d "${dest_dir}/.brew-rpms.XXXXXX") || return 1
    trap 'rm -rf -- "${download_dir}"' EXIT

    local i rpm_file metadata rpm_name rpm_evr rpm_arch
    local greenboot_evr=""
    for i in "${!urls[@]}"; do
        rpm_file="${download_dir}/${filenames[$i]}"
        echo "Downloading ${filenames[$i]} from Brew"
        if ! curl -kfsSL --retry 5 --retry-delay 2 --retry-all-errors \
            --output "${rpm_file}" -- "${urls[$i]}"; then
            echo "ERROR: failed to download ${packages[$i]} RPM" >&2
            return 1
        fi
        if ! metadata=$(rpm -qp \
            --queryformat '%{NAME} %{EPOCHNUM}:%{VERSION}-%{RELEASE} %{ARCH}\n' \
            "${rpm_file}"); then
            echo "ERROR: ${filenames[$i]} is not a readable RPM" >&2
            return 1
        fi
        read -r rpm_name rpm_evr rpm_arch <<< "${metadata}"
        if [[ "${rpm_name}" != "${packages[$i]}" ]]; then
            # Some Brew builds may name the main binary package greenboot-rs.
            if [[ "$i" != 0 || "${rpm_name}" != greenboot-rs ]]; then
                echo "ERROR: expected ${packages[$i]}, received ${rpm_name}" >&2
                return 1
            fi
        fi
        if [[ "${rpm_arch}" != "${arch}" && "${rpm_arch}" != noarch ]]; then
            echo "ERROR: ${rpm_name} architecture ${rpm_arch} does not match ${arch}" >&2
            return 1
        fi
        if [[ "$i" == 0 ]]; then
            greenboot_evr="${rpm_evr}"
        elif [[ "${rpm_evr}" != "${greenboot_evr}" ]]; then
            echo "ERROR: greenboot and health-checks RPM versions do not match" >&2
            return 1
        fi
        echo "Validated ${rpm_name} ${rpm_evr} (${rpm_arch})"
    done

    for filename in "${filenames[@]}"; do
        mv -- "${download_dir}/${filename}" "${dest_dir}/${filename}" || return 1
    done
)
