RELEASE ?= 0
TARGETDIR ?= target
SRCDIR ?= .
VERSION = $(shell grep -oP '^Version:\s+\K\S+' greenboot-rs.spec)
COMMIT = $(shell (cd "$(SRCDIR)" && git rev-parse --short HEAD))
TIMESTAMP = $(shell date +%Y%m%d%H%M%S)

# Create unique filenames with version+commit for build isolation
RPM_SPECFILE=rpmbuild/SPECS/greenboot-rs-$(VERSION)-$(COMMIT).spec
RPM_TARBALL=rpmbuild/SOURCES/greenboot-rs-$(VERSION).tar.gz

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

$(RPM_TARBALL):
	mkdir -p $(CURDIR)/rpmbuild/SOURCES
	# Create tarball with directory name matching spec file expectations: greenboot-rs-<version>/
	git archive --prefix=greenboot-rs-$(VERSION)/ --format=tar.gz HEAD > $(RPM_TARBALL)

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
