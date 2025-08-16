%global __cargo_is_lib() false
%global pkgname greenboot

Name:		greenboot-rs
Version:	0.16.0
Release:	3%{?dist}
Summary:	Generic Health Check Framework for systemd
License:	(Apache-2.0 OR MIT) AND BSD-3-Clause
URL:		https://github.com/fedora-iot/greenboot-rs
Source0:	%{url}/releases/download/%{version}/%{name}-%{version}.tar.gz

ExcludeArch:	s390x i686 %{power64}

%if 0%{?centos} && !0%{?eln}
BuildRequires:	rust-toolset
%else
BuildRequires:	rust-packaging
%endif
BuildRequires:	systemd-rpm-macros


%description
Greenboot is a generic health check framework for systemd allowing
the use of healthchecks to check the state of the system post
upgrade to ensure the system is in a known-good state and to allow
automated rollback actions if it's not.

%package -n %{pkgname}
Summary:	%{summary}
# this replaces the bash version of greenboot
Provides:	%{pkgname} = %{version}-%{release}
Obsoletes:	%{pkgname} < %{version}-%{release}
Conflicts:	%{pkgname} < %{version}-%{release}
%{?systemd_requires}
Requires:	systemd >= 240
Requires:	rpm-ostree
Requires:	bootupd
Requires:	pam >= 1.4.0
Recommends:	openssh

%description -n %{pkgname}

%{description}.

%package -n %{pkgname}-default-health-checks
Summary:	Series of optional and curated health checks
License:	BSD-3-Clause
Requires:	%{pkgname} = %{version}-%{release}
Requires:	util-linux
Requires:	jq

%description -n %{pkgname}-default-health-checks
%{description}.

This package adds some default healthchecks for greenboot.

%prep
%autosetup -n %{name}-%{version}
%cargo_prep

%generate_buildrequires
%cargo_generate_buildrequires -a

%build
%cargo_build
%{cargo_license_summary}
%{cargo_license} > LICENSE.dependencies

%install
%cargo_install
mkdir -p %{buildroot}%{_libexecdir}
mkdir -p %{buildroot}%{_libexecdir}/%{pkgname}
mv %{buildroot}%{_bindir}/greenboot %{buildroot}%{_libexecdir}/%{pkgname}/%{pkgname}
install -Dpm0644 -t %{buildroot}%{_unitdir} usr/lib/systemd/system/*.service
install -Dpm0644 -t %{buildroot}%{_unitdir} usr/lib/systemd/system/*.target
mkdir -p %{buildroot}%{_exec_prefix}/lib/motd.d/
mkdir -p %{buildroot}%{_libexecdir}/%{pkgname}
install -Dpm0644 -t %{buildroot}%{_sysconfdir}/%{pkgname} etc/greenboot/greenboot.conf
install -D -t %{buildroot}%{_prefix}/lib/bootupd/grub2-static/configs.d grub2/08_greenboot.cfg
mkdir -p %{buildroot}%{_sysconfdir}/%{pkgname}/check/required.d
mkdir    %{buildroot}%{_sysconfdir}/%{pkgname}/check/wanted.d
mkdir    %{buildroot}%{_sysconfdir}/%{pkgname}/green.d
mkdir    %{buildroot}%{_sysconfdir}/%{pkgname}/red.d
mkdir -p %{buildroot}%{_prefix}/lib/%{pkgname}/check/required.d
mkdir    %{buildroot}%{_prefix}/lib/%{pkgname}/check/wanted.d
mkdir    %{buildroot}%{_prefix}/lib/%{pkgname}/green.d
mkdir    %{buildroot}%{_prefix}/lib/%{pkgname}/red.d
mkdir -p %{buildroot}%{_unitdir}
mkdir -p %{buildroot}%{_tmpfilesdir}
install -DpZm 0755 usr/lib/greenboot/check/required.d/* %{buildroot}%{_prefix}/lib/%{pkgname}/check/required.d
install -DpZm 0755 usr/lib/greenboot/check/wanted.d/* %{buildroot}%{_prefix}/lib/%{pkgname}/check/wanted.d
install -DpZm 0644 usr/lib/systemd/system/greenboot-healthcheck.service.d/10-network-online.conf %{buildroot}%{_unitdir}/greenboot-healthcheck.service.d/10-network-online.conf

%post -n %{pkgname}
%systemd_post greenboot-healthcheck.service
%systemd_post greenboot-set-rollback-trigger.service
%systemd_post greenboot-success.target

%preun -n %{pkgname}
%systemd_preun greenboot-healthcheck.service
%systemd_preun greenboot-set-rollback-trigger.service
%systemd_preun greenboot-success.target

%postun -n %{pkgname}
%systemd_postun greenboot-healthcheck.service
%systemd_postun greenboot-set-rollback-trigger.service
%systemd_postun greenboot-success.target

%files -n %{pkgname}
%license LICENSE LICENSE.dependencies
%doc README.md
%dir %{_libexecdir}/%{pkgname}
%{_libexecdir}/%{pkgname}/%{pkgname}
%{_unitdir}/greenboot-healthcheck.service
%{_unitdir}/greenboot-set-rollback-trigger.service
%{_unitdir}/greenboot-success.target
%config(noreplace) %{_sysconfdir}/%{pkgname}/greenboot.conf
%{_prefix}/lib/bootupd/grub2-static/configs.d/08_greenboot.cfg
%dir %{_prefix}/lib/%{pkgname}
%dir %{_prefix}/lib/%{pkgname}/check
%dir %{_prefix}/lib/%{pkgname}/check/required.d
%dir %{_prefix}/lib/%{pkgname}/check/wanted.d
%dir %{_prefix}/lib/%{pkgname}/green.d
%dir %{_prefix}/lib/%{pkgname}/red.d
%dir %{_sysconfdir}/%{pkgname}
%dir %{_sysconfdir}/%{pkgname}/check
%dir %{_sysconfdir}/%{pkgname}/check/required.d
%dir %{_sysconfdir}/%{pkgname}/check/wanted.d
%dir %{_sysconfdir}/%{pkgname}/green.d
%dir %{_sysconfdir}/%{pkgname}/red.d

%files -n %{pkgname}-default-health-checks
%dir %{_unitdir}/greenboot-healthcheck.service.d
%{_prefix}/lib/%{pkgname}/check/wanted.d/01_update_platforms_check.sh
%{_prefix}/lib/%{pkgname}/check/required.d/02_watchdog.sh
%{_prefix}/lib/%{pkgname}/check/required.d/01_repository_dns_check.sh
%{_unitdir}/greenboot-healthcheck.service.d/10-network-online.conf

%changelog
* Fri Aug 15 2025 Peter Robinson <pbrobinson@fedoraproject.org> - 0.16.0-3
- Various spec file cleanups

* Fri Jul 25 2025 Paul Whalen <pwhalen@fedoraproject.org> - 0.16.0-2
- Update src to greenboot-rs, binaries remain greenboot
- Obsoletes/Conflicts for bash greenboot, Provides greenboot

* Thu Jul 24 2025 Sayan Paul <saypaul@redhat.com> - 0.16.0-1
- Initial Package
- Switched to native Fedora dependencies, removing vendoring.
