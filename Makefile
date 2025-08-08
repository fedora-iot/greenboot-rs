RELEASE ?= 0
TARGETDIR ?= target
SRCDIR ?= .
COMMIT = $(shell (cd "$(SRCDIR)" && git rev-parse HEAD))

RPM_SPECFILE=rpmbuild/SPECS/greenboot-$(COMMIT).spec
RPM_TARBALL=rpmbuild/SOURCES/greenboot-$(COMMIT).tar.gz

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
	(echo "%global commit $(COMMIT)"; git show HEAD:greenboot.spec) > $(RPM_SPECFILE)

$(RPM_TARBALL):
	mkdir -p $(CURDIR)/rpmbuild/SOURCES
	git archive --prefix=greenboot-$(COMMIT)/ --format=tar.gz HEAD > $(RPM_TARBALL)

.PHONY: build
build:
	cargo build "--target-dir=${TARGETDIR}" ${CARGO_ARGS}

.PHONY: install
install: build
	install -D -t ${DESTDIR}/usr/libexec "${TARGETDIR}/${PROFILE}/greenboot"
	install -D -m 644 -t ${DESTDIR}/usr/lib/systemd/system dist/systemd/system/*.service

.PHONY: check
check:
	cargo test "--target-dir=${TARGETDIR}" -- --test-threads=1

.PHONY: srpm
srpm: $(RPM_SPECFILE) $(RPM_TARBALL)
	rpmbuild -bs \
		--define "_topdir $(CURDIR)/rpmbuild" \
		$(RPM_SPECFILE)

.PHONY: rpm
rpm: $(RPM_SPECFILE) $(RPM_TARBALL)
	rpmbuild -bb \
		--define "_topdir $(CURDIR)/rpmbuild" \
		$(RPM_SPECFILE)

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
	if [ "$$ID" = "fedora" ] && { [ "$$VERSION_ID" = "rawhide" ] || [ "$$VERSION_ID" = "43" ]; }; then \
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
