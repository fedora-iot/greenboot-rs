%global debug_package %{nil}
%bcond_without check
%global with_bundled 1
%global with_packit 0
%global __cargo_skip_build 0
%global __cargo_is_lib() false
%global forgeurl https://github.com/fedora-iot/greenboot

Version:            0.16.0

%forgemeta

Name:               greenboot
Release:            0%{?dist}
Summary:            Generic Health Check Framework for systemd
License:            LGPLv2+

URL:            %{forgeurl}
Source:         %{forgesource}
%if ! 0%{?with_packit}
%if "%{?commit}" != ""
Source1:        %{name}-%{commit}-vendor.tar.gz
%else
Source1:        %{name}-%{version}-vendor.tar.gz
%endif
%endif

ExcludeArch:    s390x i686 %{power64}

%if 0%{?centos} && !0%{?eln}
BuildRequires:  rust-toolset
%else
BuildRequires:  rust-packaging
%endif
BuildRequires:      systemd-rpm-macros
%{?systemd_requires}
Requires:           systemd >= 240
Requires:           rpm-ostree
# PAM is required to programmatically read motd messages from /etc/motd.d/*
# This causes issues with RHEL-8 as the fix isn't there an el8 is on pam-1.3.x
Requires:           pam >= 1.4.0
# While not strictly necessary to generate the motd, the main use-case of this package is to display it on SSH login
Recommends:         openssh
# List of bundled crates in vendor tarball, generated with:
# cargo metadata --locked --format-version 1 | CRATE_NAME="greenboot" ./bundled-provides.jq
Provides: bundled(crate(aho-corasick)) = 1.1.3
Provides: bundled(crate(anstream)) = 0.6.18
Provides: bundled(crate(anstyle)) = 1.0.10
Provides: bundled(crate(anstyle-parse)) = 0.2.6
Provides: bundled(crate(anstyle-query)) = 1.1.2
Provides: bundled(crate(anstyle-wincon)) = 3.0.7
Provides: bundled(crate(anyhow)) = 1.0.98
Provides: bundled(crate(arraydeque)) = 0.5.1
Provides: bundled(crate(async-trait)) = 0.1.88
Provides: bundled(crate(autocfg)) = 1.4.0
Provides: bundled(crate(base64)) = 0.21.7
Provides: bundled(crate(bitflags)) = 1.3.2
Provides: bundled(crate(bitflags)) = 2.9.0
Provides: bundled(crate(block-buffer)) = 0.10.4
Provides: bundled(crate(cfg-if)) = 1.0.0
Provides: bundled(crate(clap)) = 4.5.36
Provides: bundled(crate(clap_builder)) = 4.5.36
Provides: bundled(crate(clap_derive)) = 4.5.32
Provides: bundled(crate(clap_lex)) = 0.7.4
Provides: bundled(crate(colorchoice)) = 1.0.3
Provides: bundled(crate(config)) = 0.15.11
Provides: bundled(crate(const-random)) = 0.1.18
Provides: bundled(crate(const-random-macro)) = 0.1.16
Provides: bundled(crate(convert_case)) = 0.6.0
Provides: bundled(crate(cpufeatures)) = 0.2.17
Provides: bundled(crate(crunchy)) = 0.2.3
Provides: bundled(crate(crypto-common)) = 0.1.6
Provides: bundled(crate(digest)) = 0.10.7
Provides: bundled(crate(dlv-list)) = 0.5.2
Provides: bundled(crate(encoding_rs)) = 0.8.35
Provides: bundled(crate(env_logger)) = 0.10.2
Provides: bundled(crate(equivalent)) = 1.0.2
Provides: bundled(crate(errno)) = 0.3.11
Provides: bundled(crate(fastrand)) = 2.3.0
Provides: bundled(crate(foldhash)) = 0.1.5
Provides: bundled(crate(generic-array)) = 0.14.7
Provides: bundled(crate(getrandom)) = 0.2.15
Provides: bundled(crate(getrandom)) = 0.3.2
Provides: bundled(crate(glob)) = 0.3.2
Provides: bundled(crate(hashbrown)) = 0.14.5
Provides: bundled(crate(hashbrown)) = 0.15.2
Provides: bundled(crate(hashlink)) = 0.10.0
Provides: bundled(crate(heck)) = 0.5.0
Provides: bundled(crate(hermit-abi)) = 0.5.0
Provides: bundled(crate(humantime)) = 2.2.0
Provides: bundled(crate(indexmap)) = 2.9.0
Provides: bundled(crate(is-terminal)) = 0.4.16
Provides: bundled(crate(is_terminal_polyfill)) = 1.70.1
Provides: bundled(crate(itoa)) = 1.0.15
Provides: bundled(crate(json5)) = 0.4.1
Provides: bundled(crate(libc)) = 0.2.171
Provides: bundled(crate(linux-raw-sys)) = 0.9.4
Provides: bundled(crate(log)) = 0.4.27
Provides: bundled(crate(memchr)) = 2.7.4
Provides: bundled(crate(memoffset)) = 0.7.1
Provides: bundled(crate(nix)) = 0.26.4
Provides: bundled(crate(once_cell)) = 1.21.3
Provides: bundled(crate(ordered-multimap)) = 0.7.3
Provides: bundled(crate(pathdiff)) = 0.2.3
Provides: bundled(crate(pest)) = 2.8.0
Provides: bundled(crate(pest_derive)) = 2.8.0
Provides: bundled(crate(pest_generator)) = 2.8.0
Provides: bundled(crate(pest_meta)) = 2.8.0
Provides: bundled(crate(pin-utils)) = 0.1.0
Provides: bundled(crate(pretty_env_logger)) = 0.5.0
Provides: bundled(crate(proc-macro2)) = 1.0.94
Provides: bundled(crate(quote)) = 1.0.40
Provides: bundled(crate(r-efi)) = 5.2.0
Provides: bundled(crate(regex)) = 1.11.1
Provides: bundled(crate(regex-automata)) = 0.4.9
Provides: bundled(crate(regex-syntax)) = 0.8.5
Provides: bundled(crate(ron)) = 0.8.1
Provides: bundled(crate(rust-ini)) = 0.21.1
Provides: bundled(crate(rustix)) = 1.0.5
Provides: bundled(crate(ryu)) = 1.0.20
Provides: bundled(crate(serde)) = 1.0.219
Provides: bundled(crate(serde_derive)) = 1.0.219
Provides: bundled(crate(serde_json)) = 1.0.140
Provides: bundled(crate(serde_spanned)) = 0.6.8
Provides: bundled(crate(sha2)) = 0.10.8
Provides: bundled(crate(strsim)) = 0.11.1
Provides: bundled(crate(syn)) = 2.0.100
Provides: bundled(crate(tempfile)) = 3.19.1
Provides: bundled(crate(termcolor)) = 1.4.1
Provides: bundled(crate(thiserror)) = 2.0.12
Provides: bundled(crate(thiserror-impl)) = 2.0.12
Provides: bundled(crate(tiny-keccak)) = 2.0.2
Provides: bundled(crate(toml)) = 0.8.20
Provides: bundled(crate(toml_datetime)) = 0.6.8
Provides: bundled(crate(toml_edit)) = 0.22.24
Provides: bundled(crate(trim-in-place)) = 0.1.7
Provides: bundled(crate(typenum)) = 1.18.0
Provides: bundled(crate(ucd-trie)) = 0.1.7
Provides: bundled(crate(unicode-ident)) = 1.0.18
Provides: bundled(crate(unicode-segmentation)) = 1.12.0
Provides: bundled(crate(utf8parse)) = 0.2.2
Provides: bundled(crate(version_check)) = 0.9.5
Provides: bundled(crate(wasi)) = 0.11.0+wasi_snapshot_preview1
Provides: bundled(crate(wasi)) = 0.14.2+wasi_0.2.4
Provides: bundled(crate(winapi-util)) = 0.1.9
Provides: bundled(crate(windows-sys)) = 0.59.0
Provides: bundled(crate(windows-targets)) = 0.52.6
Provides: bundled(crate(windows_aarch64_gnullvm)) = 0.52.6
Provides: bundled(crate(windows_aarch64_msvc)) = 0.52.6
Provides: bundled(crate(windows_i686_gnu)) = 0.52.6
Provides: bundled(crate(windows_i686_gnullvm)) = 0.52.6
Provides: bundled(crate(windows_i686_msvc)) = 0.52.6
Provides: bundled(crate(windows_x86_64_gnu)) = 0.52.6
Provides: bundled(crate(windows_x86_64_gnullvm)) = 0.52.6
Provides: bundled(crate(windows_x86_64_msvc)) = 0.52.6
Provides: bundled(crate(winnow)) = 0.7.6
Provides: bundled(crate(wit-bindgen-rt)) = 0.39.0
Provides: bundled(crate(yaml-rust2)) = 0.10.1

%description
%{summary}.

%package default-health-checks
Summary:            Series of optional and curated health checks
Requires:           %{name} = %{version}-%{release}
Requires:           util-linux
Requires:           jq

%description default-health-checks
%{summary}.

%prep
%forgeautosetup
%if ! 0%{?with_packit}
tar xvf %{SOURCE1}
%endif
%if ! 0%{?with_bundled}
%cargo_prep
%else
mkdir -p .cargo
cat >.cargo/config << EOF
[build]
rustc = "%{__rustc}"
rustdoc = "%{__rustdoc}"
rustflags = "%{__global_rustflags_toml}"

[profile.rpm]
inherits = "release"

[install]
root = "%{buildroot}%{_prefix}"
 
[term]
verbose = true
 
[source.crates-io]
replace-with = "vendored-sources"
 
[source.vendored-sources]
directory = "vendor"
EOF
%endif

%if ! 0%{?with_bundled}
%generate_buildrequires
%cargo_generate_buildrequires
%endif

%build
%cargo_build

%install
%cargo_install
mkdir -p %{buildroot}%{_libexecdir}
mkdir -p %{buildroot}%{_libexecdir}/%{name}
mv %{buildroot}%{_bindir}/greenboot %{buildroot}%{_libexecdir}/%{name}/%{name}
install -Dpm0644 -t %{buildroot}%{_unitdir} usr/lib/systemd/system/*.service
install -Dpm0644 -t %{buildroot}%{_unitdir} usr/lib/systemd/system/*.target
mkdir -p %{buildroot}%{_exec_prefix}/lib/motd.d/
mkdir -p %{buildroot}%{_libexecdir}/%{name}
install -Dpm0644 -t %{buildroot}%{_sysconfdir}/%{name} etc/greenboot/greenboot.conf
install -D -t %{buildroot}%{_prefix}/lib/bootupd/grub2-static/configs.d grub2/08_greenboot.cfg
mkdir -p %{buildroot}%{_sysconfdir}/%{name}/check/required.d
mkdir    %{buildroot}%{_sysconfdir}/%{name}/check/wanted.d
mkdir    %{buildroot}%{_sysconfdir}/%{name}/green.d
mkdir    %{buildroot}%{_sysconfdir}/%{name}/red.d
mkdir -p %{buildroot}%{_prefix}/lib/%{name}/check/required.d
mkdir    %{buildroot}%{_prefix}/lib/%{name}/check/wanted.d
mkdir    %{buildroot}%{_prefix}/lib/%{name}/green.d
mkdir    %{buildroot}%{_prefix}/lib/%{name}/red.d
mkdir -p %{buildroot}%{_unitdir}
mkdir -p %{buildroot}%{_tmpfilesdir}
install -DpZm 0755 usr/lib/greenboot/check/required.d/* %{buildroot}%{_prefix}/lib/%{name}/check/required.d
install -DpZm 0755 usr/lib/greenboot/check/wanted.d/* %{buildroot}%{_prefix}/lib/%{name}/check/wanted.d

%post
%systemd_post greenboot-healthcheck.service
%systemd_post greenboot-rollback.service
%systemd_post greenboot-success.target

%preun
%systemd_preun greenboot-healthcheck.service
%systemd_preun greenboot-rollback.service
%systemd_preun greenboot-success.target

%postun
%systemd_postun greenboot-healthcheck.service
%systemd_postun greenboot-rollback.service
%systemd_postun greenboot-success.target

%files
%doc README.md
%license LICENSE
%dir %{_libexecdir}/%{name}
%{_libexecdir}/%{name}/%{name}
%{_unitdir}/greenboot-healthcheck.service
%{_unitdir}/greenboot-rollback.service
%{_unitdir}/greenboot-success.target
%{_sysconfdir}/%{name}/greenboot.conf
%{_prefix}/lib/bootupd/grub2-static/configs.d/08_greenboot.cfg
%dir %{_prefix}/lib/%{name}
%dir %{_prefix}/lib/%{name}/check
%dir %{_prefix}/lib/%{name}/check/required.d
%dir %{_prefix}/lib/%{name}/check/wanted.d
%dir %{_prefix}/lib/%{name}/green.d
%dir %{_prefix}/lib/%{name}/red.d
%dir %{_sysconfdir}/%{name}
%dir %{_sysconfdir}/%{name}/check
%dir %{_sysconfdir}/%{name}/check/required.d
%dir %{_sysconfdir}/%{name}/check/wanted.d
%dir %{_sysconfdir}/%{name}/green.d
%dir %{_sysconfdir}/%{name}/red.d

%files default-health-checks
%{_prefix}/lib/%{name}/check/required.d/01_repository_dns_check.sh
%{_prefix}/lib/%{name}/check/wanted.d/01_update_platforms_check.sh
%{_prefix}/lib/%{name}/check/required.d/02_watchdog.sh

%changelog
* Fri Apr 25 2025 Sayan Paul <paul.sayan@gmail.com> - 0.16-1
- Initial Package
