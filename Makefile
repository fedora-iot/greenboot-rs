RELEASE ?= 0
SPEC_RELEASE ?= $(shell rpmspec -q --srpm --qf '%{release}\n' greenboot-rs.spec | head -n1)
TARGETDIR ?= target
SRCDIR ?= .
VENDOR ?= false
VERSION = $(shell grep -oP '^Version:\s+\K\S+' greenboot-rs.spec)
COMMIT = $(shell (cd "$(SRCDIR)" && git rev-parse --short HEAD))
TIMESTAMP = $(shell date +%Y%m%d%H%M%S)
PLATFORMS = $(shell (echo {x86_64,aarch64,powerpc64le,s390x}-unknown-linux-gnu))
GREENBOOT_RUST_DEPENDENCIES = rust-anyhow+default-devel \
				rust-clap+derive-devel \
				rust-clap+default-devel \
				rust-config+default-devel \
				rust-env_logger+default-devel \
				rust-glob+default-devel \
				rust-log+default-devel \
				rust-nix+default-devel \
				rust-once_cell+default-devel \
				rust-pretty_env_logger+default-devel \
				rust-serde+default-devel \
				rust-serde_json+default-devel \
				rust-tempfile+default-devel \
				rust-thiserror+default-devel

# Create unique filenames with version+commit for build isolation
RPM_SPECFILE=rpmbuild/SPECS/greenboot-rs-$(VERSION)-$(COMMIT).spec
RPM_TARBALL=rpmbuild/SOURCES/greenboot-rs-$(VERSION)-$(SPEC_RELEASE).tar.gz
VENDOR_TARBALL=greenboot-rs-$(VERSION)-$(SPEC_RELEASE)-vendor-patched.tar.xz

ifeq ($(RELEASE),1)
	PROFILE ?= release
	CARGO_ARGS = --release
else
	PROFILE ?= debug
	CARGO_ARGS =
endif

.PHONY: all
all: build check

$(RPM_SPECFILE):
	mkdir -p $(CURDIR)/rpmbuild/SPECS
	# Copy spec file as-is, no modifications - let the existing release string be used
	cp greenboot-rs.spec $(RPM_SPECFILE)

$(VENDOR_TARBALL): vendor
	tar cJf $(VENDOR_TARBALL) vendor; \
	rm -rf .cargo vendor; \

.PHONY: vendor-tarball
vendor-tarball: $(VENDOR_TARBALL)

.PHONY: vendor
vendor:
	vendor_filterer_cmd=$$(command -v cargo-vendor-filterer||:) \
	[ -z "$$vendor_filterer_cmd" ] || rm -f $${vendor_filterer_cmd}; \
	. /etc/os-release; \
	if [[ "$$ID" = "fedora" ]] || [[ "$$ID" = "centos" ]]; then \
		sudo dnf install -y pkgconf-pkg-config openssl-devel; \
	fi; \
	cargo install --quiet cargo-vendor-filterer@0.5.16; \
	for platform in $(PLATFORMS); do  \
		args+="--platform $${platform} "; \
	done; \
	# https://issues.redhat.com/browse/RHEL-65521 \
	args+="--exclude-crate-path idna#tests "; \
	rm -rf vendor; \
	mkdir -p .cargo; \
	cargo vendor-filterer $${args} > ./.cargo/config.toml; \

RPM_VENDOR_TARBALL=rpmbuild/SOURCES/greenboot-rs-$(VERSION)-$(SPEC_RELEASE)-vendor-patched.tar.xz

$(RPM_TARBALL): $(VENDOR_TARBALL)
	mkdir -p $(CURDIR)/rpmbuild/SOURCES
	# Create tarball with directory name matching spec file expectations: greenboot-rs-<version>-<release>/
	git archive --prefix=greenboot-rs-$(VERSION)-$(SPEC_RELEASE)/ --format=tar.gz HEAD > $(RPM_TARBALL)
	mv $(VENDOR_TARBALL) $(RPM_VENDOR_TARBALL);

.PHONY: build
build:
	cargo build "--target-dir=${TARGETDIR}" ${CARGO_ARGS}

.PHONY: check
check:
	cargo test "--target-dir=${TARGETDIR}" ${CARGO_ARGS}

.PHONY: fmt
fmt:
	cargo fmt

.PHONY: srpm
srpm: $(RPM_SPECFILE) $(RPM_TARBALL)
	rpmbuild -bs $(RPM_SPECFILE) \
		--define "_topdir $(CURDIR)/rpmbuild" \
		--define "_sourcedir $(CURDIR)/rpmbuild/SOURCES" \
		--define "_specdir $(CURDIR)/rpmbuild/SPECS" \
		--define "_srcrpmdir $(CURDIR)/rpmbuild/SRPMS"

.PHONY: rpm
rpm: $(RPM_SPECFILE) $(RPM_TARBALL)
	. /etc/os-release; \
	if [ "$$ID" = "fedora" ]; then \
		sudo dnf install -y $(GREENBOOT_RUST_DEPENDENCIES); \
	fi; \
	rpmbuild -bb $(RPM_SPECFILE) \
		--define "_topdir $(CURDIR)/rpmbuild" \
		--define "_sourcedir $(CURDIR)/rpmbuild/SOURCES" \
		--define "_specdir $(CURDIR)/rpmbuild/SPECS" \
		--define "_builddir $(CURDIR)/rpmbuild/BUILD" \
		--define "_rpmdir $(CURDIR)/rpmbuild/RPMS"

.PHONY: clean
clean:
	cargo clean "--target-dir=${TARGETDIR}"
	rm -rf rpmbuild

# integration-test: Run the bootc image integration tests (requires QUAY credentials)
#
# Prerequisites:
#   - Must be executed on Fedora Rawhide OR CentOS Stream 10
#   - Requires QUAY_USERNAME and QUAY_PASSWORD environment variables
#
# Usage:
#   QUAY_USERNAME=<your_quay_username> QUAY_PASSWORD=<your_quay_pass> make integration-test
.PHONY: integration-test
integration-test:
	@# Verify required environment variables are set
	@if [ -z "$$QUAY_USERNAME" ]; then \
		echo "ERROR: QUAY_USERNAME environment variable not set"; \
		echo "Usage: QUAY_USERNAME=quay_user QUAY_PASSWORD=quay_pass make integration-test"; \
		exit 1; \
	fi
	@if [ -z "$$QUAY_PASSWORD" ]; then \
		echo "ERROR: QUAY_PASSWORD environment variable not set"; \
		echo "Usage: QUAY_USERNAME=quay_user QUAY_PASSWORD=quay_pass make integration-test"; \
		exit 1; \
	fi

	@# Verify supported operating system
	@. /etc/os-release; \
	if [ "$$ID" = "fedora" ] && { [ "$$VERSION_ID" = "rawhide" ] || [ "$$VERSION_ID" = "43" ] || [ "$$VERSION_ID" = "44" ]; }; then \
		echo "Running on Fedora $$VERSION_ID"; \
	elif [ "$$ID" = "centos" ] && [ "$$VERSION_ID" = "10" ]; then \
		echo "Running on CentOS Stream $$VERSION_ID"; \
	else \
		echo "Unsupported OS: $$ID $$VERSION_ID"; \
		echo "This test requires Fedora Rawhide or CentOS Stream 10"; \
		exit 1; \
	fi

	@# Run test script and report results
	@echo "Starting integration test"; \
	cd tests && ./greenboot-bootc-qcow2.sh; \
	TEST_EXIT=$$?; \
	if [ $$TEST_EXIT -eq 0 ]; then \
		echo "SUCCESS: Integration test passed"; \
	else \
		echo "FAILURE: Integration test failed"; \
		exit $$TEST_EXIT; \
	fi
