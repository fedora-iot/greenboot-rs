%global debug_package %{nil}
%bcond_without check
%global __cargo_skip_build 0
%global __cargo_is_lib() false
%global forgeurl https://github.com/fedora-iot/greenboot

Version:            0.16.0

%forgemeta

Name:               greenboot
Release:            0%{?dist}
Summary:            Generic Health Check Framework for systemd
License:            LGPLv2+

URL:               %{forgeurl}
Source0:           %{forgesource}

ExcludeArch:    s390x i686 %{power64}

%if 0%{?centos} && !0%{?eln}
BuildRequires:  rust-toolset
%else
BuildRequires:  rust-packaging
%endif
BuildRequires:      systemd-rpm-macros

# greenboot dependencies
BuildRequires: rust-anyhow-devel
BuildRequires: rust-clap+default-devel
BuildRequires: rust-clap_derive-devel
BuildRequires: rust-config-devel
BuildRequires: rust-env_logger-devel
BuildRequires: rust-glob-devel
BuildRequires: rust-once_cell-devel
BuildRequires: rust-pretty_env_logger-devel
BuildRequires: rust-serde_json-devel
BuildRequires: rust-tempfile+default-devel
BuildRequires: rust-thiserror-devel
BuildRequires: rust-config+default-devel
BuildRequires: rust-nix-devel

%{?systemd_requires}
Requires:           systemd >= 240
Requires:           rpm-ostree
# PAM is required to programmatically read motd messages from /etc/motd.d/*
# This causes issues with RHEL-8 as the fix isn't there an el8 is on pam-1.3.x
Requires:           pam >= 1.4.0
# While not strictly necessary to generate the motd, the main use-case of this package is to display it on SSH login
Recommends:         openssh

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
%cargo_prep

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
install -DpZm 0644 usr/lib/systemd/system/greenboot-healthcheck.service.d/10-network-online.conf %{buildroot}%{_unitdir}/greenboot-healthcheck.service.d/10-network-online.conf

%post
%systemd_post greenboot-healthcheck.service
%systemd_post greenboot-set-rollback-trigger.service
%systemd_post greenboot-success.target

%preun
%systemd_preun greenboot-healthcheck.service
%systemd_preun greenboot-set-rollback-trigger.service
%systemd_preun greenboot-success.target

%postun
%systemd_postun greenboot-healthcheck.service
%systemd_postun greenboot-set-rollback-trigger.service
%systemd_postun greenboot-success.target

%files
%doc README.md
%license LICENSE
%dir %{_libexecdir}/%{name}
%{_libexecdir}/%{name}/%{name}
%{_unitdir}/greenboot-healthcheck.service
%{_unitdir}/greenboot-set-rollback-trigger.service
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
%{_prefix}/lib/%{name}/check/wanted.d/01_update_platforms_check.sh
%{_prefix}/lib/%{name}/check/required.d/02_watchdog.sh
%{_prefix}/lib/%{name}/check/required.d/01_repository_dns_check.sh
%{_unitdir}/greenboot-healthcheck.service.d/10-network-online.conf

%changelog
* Thu Jul 24 2025 Sayan Paul <paul.sayan@gmail.com> - 0.16-1
- Initial Package
- Switched to native Fedora dependencies, removing vendoring.