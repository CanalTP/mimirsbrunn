#
#   Based on Makefile from https://github.com/mvanholsteijn/docker-makefile
#   Based on https://gist.github.com/mpneuried/0594963ad38e68917ef189b4e6a269db
#

# Import deploy config
# You can change the default deploy config with `make cnf="deploy_special.env" release`
dpl ?= deploy.env
include $(dpl)
export $(shell sed 's/=.*//' $(dpl))

# HELP
# This will output the help for each task
# thanks to https://marmelab.com/blog/2016/02/29/auto-documented-makefile.html
.PHONY: help

help: ## This help.
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

RELEASE_SUPPORT := $(shell dirname $(abspath $(lastword $(MAKEFILE_LIST))))/.make-release-support

VERSION=$(shell . $(RELEASE_SUPPORT) ; getVersion)
DOCKERS = $(patsubst ./docker/%/,%, $(dir $(wildcard ./docker/*/)))
DOCKER_TAGS=$(shell . $(RELEASE_SUPPORT) ; getDockerTags)
TAG=$(shell . $(RELEASE_SUPPORT); getTag)

SHELL=/bin/bash

.PHONY: \
	pre-build docker-build post-build build \
	release patch-release minor-release major-release tag check-status check-release \
	push pre-push do-push post-push

build: pre-build docker-build post-build ## Build one or more docker images

check: pre-build          ## Runs validity checks (fmt, lint, unit tests)

pre-build: fmt lint test

post-build:

pre-push:

post-push:

docker-build:
	$(info $$DOCKERS is [${DOCKERS}])
	@for DOCKER in $(DOCKERS); do \
		for ENV in $(BUILD_ENV); do \
			TAGS=""; \
			SPL=$${ENV/:/ }; \
			DEB=$$(echo $$SPL | awk '{print $$1;}'); \
			RST=$$(echo $$SPL | awk '{print $$2;}'); \
			echo "Building $$DOCKER for debian $$DEB / rust $$RST"; \
			ARG_DEB="--build-arg DEBIAN_VERSION=$$DEB"; \
			ARG_RST="--build-arg RUST_VERSION=$$RST"; \
			for DOCKER_TAG in $(DOCKER_TAGS); do \
			  TAGS=$$TAGS" --tag $$DOCKER_REPO/$$DOCKER:$$DOCKER_TAG-$$DEB"; \
			done; \
			FIRST_ENV=$$(echo $(BUILD_ENV) | awk '{print $$1;}'); \
			if [ $$FIRST_ENV = $$ENV ]; then \
				for DOCKER_TAG in $(DOCKER_TAGS); do \
				  TAGS=$$TAGS" --tag $$DOCKER_REPO/$$DOCKER:$$DOCKER_TAG"; \
				done; \
				TAGS=$$TAGS" --tag $$DOCKER_REPO/$$DOCKER:latest"; \
			fi; \
			echo "docker build $(DOCKER_BUILD_ARGS) $$ARG_DEB $$ARG_RST $$TAGS -f docker/$$DOCKER/Dockerfile ."; \
			docker build $(DOCKER_BUILD_ARGS) $$ARG_DEB $$ARG_RST $$TAGS -f docker/$$DOCKER/Dockerfile . ; \
		done; \
	done

release: check-status check-release build push

push: pre-push do-push post-push

do-push:
	@for DOCKER in $(DOCKERS); do \
		for ENV in $(BUILD_ENV); do \
			SPL=$${ENV/:/ }; \
			DEB=$$(echo $$SPL | awk '{print $$1;}'); \
			for DOCKER_TAG in $(DOCKER_TAGS); do \
			  docker push $$DOCKER_REPO/$$DOCKER:$$DOCKER_TAG-$$DEB; \
			done; \
			FIRST_ENV=$$(echo $(BUILD_ENV) | awk '{print $$1;}'); \
			if [ $$FIRST_ENV = $$ENV ]; then \
				for DOCKER_TAG in $(DOCKER_TAGS); do \
				docker push $$DOCKER_REPO/$$DOCKER:$$DOCKER_TAG; \
				done; \
				docker push $$DOCKER_REPO/$$DOCKER:latest; \
			fi; \
		done; \
	done

snapshot: build push

tag-patch-release: VERSION := $(shell . $(RELEASE_SUPPORT); nextPatchLevel)
tag-patch-release: tag

tag-minor-release: VERSION := $(shell . $(RELEASE_SUPPORT); nextMinorLevel)
tag-minor-release: tag

tag-major-release: VERSION := $(shell . $(RELEASE_SUPPORT); nextMajorLevel)
tag-major-release: tag

patch-release: tag-patch-release release ## Increment the patch version number and release
	@echo $(VERSION)

minor-release: tag-minor-release release ## Increment the minor version number and release
	@echo $(VERSION)

major-release: tag-major-release release ## Increment the major version number and release
	@echo $(VERSION)

tag: TAG=$(shell . $(RELEASE_SUPPORT); getTag $(VERSION))

tag: check-status ## Check that the tag does not already exist, changes the version in Cargo.toml, commit, and tag.
	@. $(RELEASE_SUPPORT) ; ! tagExists $(TAG) || (echo "ERROR: tag $(TAG) for version $(VERSION) already tagged in git" >&2 && exit 1) ;
	@. $(RELEASE_SUPPORT) ; setRelease $(VERSION)
	cargo check # We need to add this cargo check which will update Cargo.lock. Otherwise Cargo.lock will be modified after,
	            # and the release will seem dirty.
	git add .
	git commit -m "[VER] new version $(VERSION)" ;
	git tag $(TAG) ;
	@ if [ -n "$(shell git remote -v)" ] ; then git push --tags ; else echo 'no remote to push tags to' ; fi

check-status: ## Check that there are no outstanding changes. (uses git status)
	@. $(RELEASE_SUPPORT) ; ! hasChanges \
		|| (echo "Status ERROR: there are outstanding changes" >&2 && exit 1) \
		&& (echo "Status OK" >&2 ) ;

check-release: ## Check that the current git tag matches the one in Cargo.toml and there are no outstanding changes.
	@. $(RELEASE_SUPPORT) ; tagExists $(TAG) || (echo "ERROR: version not yet tagged in git. make [minor,major,patch]-release." >&2 && exit 1) ;
	@. $(RELEASE_SUPPORT) ; ! differsFromRelease $(TAG) || (echo "ERROR: current directory differs from tagged $(TAG). make [minor,major,patch]-release." ; exit 1)


######### Debug

check-tag:
	@echo $(TAG)

check-version:
	@echo $(VERSION)

### RUST related rules

fmt: format ## Check formatting of the code (alias for 'format')
format: ## Check formatting of the code
	cargo fmt --all -- --check

clippy: lint ## Check quality of the code (alias for 'lint')
lint: ## Check quality of the code
	cargo clippy --all-features --all-targets -- --warn clippy::cargo --allow clippy::multiple_crate_versions --deny warnings

test: ## Launch all tests
	cargo test --all-targets                 # `--all-targets` but no doctests


