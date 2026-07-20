.PHONY: build clean site-bootstrap site-build site-clean site-tools-clean

SITE_DIR := website
SITE_DIST := $(SITE_DIR)/dist
SITE_TOOLS := $(SITE_DIR)/.tools
TAILWIND_VERSION := 4.3.3

HOST_OS := $(shell uname -s)
HOST_ARCH := $(shell uname -m)

ifeq ($(HOST_OS),Darwin)
TAILWIND_OS := macos
else ifeq ($(HOST_OS),Linux)
TAILWIND_OS := linux
else
$(error Unsupported operating system for Tailwind standalone CLI: $(HOST_OS))
endif

ifeq ($(HOST_ARCH),arm64)
TAILWIND_ARCH := arm64
else ifeq ($(HOST_ARCH),aarch64)
TAILWIND_ARCH := arm64
else ifeq ($(HOST_ARCH),x86_64)
TAILWIND_ARCH := x64
else
$(error Unsupported architecture for Tailwind standalone CLI: $(HOST_ARCH))
endif

TAILWIND_BINARY := $(SITE_TOOLS)/tailwindcss-$(TAILWIND_VERSION)-$(TAILWIND_OS)-$(TAILWIND_ARCH)
TAILWIND_URL := https://github.com/tailwindlabs/tailwindcss/releases/download/v$(TAILWIND_VERSION)/tailwindcss-$(TAILWIND_OS)-$(TAILWIND_ARCH)

build:
	cargo build --release

clean:
	rm -rf target

site-bootstrap: $(TAILWIND_BINARY)

$(TAILWIND_BINARY):
	@command -v curl >/dev/null || { echo "curl is required to fetch Tailwind" >&2; exit 1; }
	mkdir -p "$(SITE_TOOLS)"
	curl --fail --location --retry 3 --silent --show-error "$(TAILWIND_URL)" --output "$@"
	chmod +x "$@"

site-build: site-bootstrap
	rm -rf "$(SITE_DIST)"
	mkdir -p "$(SITE_DIST)/assets"
	cp "$(SITE_DIR)/index.html" "$(SITE_DIST)/index.html"
	cp "$(SITE_DIR)/src/main.js" "$(SITE_DIST)/assets/main.js"
	"$(TAILWIND_BINARY)" -i "$(SITE_DIR)/src/input.css" -o "$(SITE_DIST)/assets/styles.css" --minify

site-clean:
	rm -rf "$(SITE_DIST)"

site-tools-clean:
	rm -rf "$(SITE_TOOLS)"
