# Written for BASH
SHELL := bash

# Set default shell flags
.SHELLFLAGS := -eu -o pipefail -c

# Delete target file if make rule fails
.DELETE_ON_ERROR:

# Warn about undefined vars early
MAKEFLAGS += --warn-undefined-variables

# Turn off built-in implicit rules
MAKEFLAGS += --no-builtin-rules

# Running `make` will trigger `make help`
.DEFAULT_GOAL := help

# Project settings
PROJECT_NAME := OneScribe
SCHEME := OneScribe
CONFIGURATION := Debug
TEST_DESTINATION := platform=macOS

# Get the actual app bundle path from xcodebuild
APP_BUNDLE := $(shell xcodebuild -project $(PROJECT_NAME).xcodeproj -scheme $(SCHEME) -configuration $(CONFIGURATION) -showBuildSettings 2>/dev/null | grep -m1 'BUILT_PRODUCTS_DIR' | awk '{print $$NF}')/$(PROJECT_NAME).app
INSTALL_DIR ?= /Applications
INSTALLED_APP_BUNDLE := $(INSTALL_DIR)/$(PROJECT_NAME).app

help: ## Show available targets
	@echo "Available targets:"
	@grep -E '^[a-zA-Z_-]+.*## .*' $(MAKEFILE_LIST) | awk 'BEGIN {FS = "## "}; {printf "  %-20s %s\n", $$1, $$2}'

setup: ## Prompt for API key and save config for Finder/Xcode/Terminal launches
	@config_dir="$$HOME/Library/Application Support/OneScribe"; \
	config_path="$$config_dir/config.json"; \
	if [[ -n "$${GROQ_API_KEY:-}" ]]; then \
		echo "GROQ_API_KEY is already set in the environment. Setup not needed."; \
		exit 0; \
	fi; \
	if [[ -f "$$config_path" ]]; then \
		existing_key="$$(sed -nE 's/.*"(groq_api_key|GROQ_API_KEY)"[[:space:]]*:[[:space:]]*"([^"]+)".*/\2/p' "$$config_path" | head -n1)"; \
		if [[ -n "$$existing_key" ]]; then \
			echo "API key is already configured at $$config_path. Setup not needed."; \
			exit 0; \
		fi; \
	fi; \
	mkdir -p "$$config_dir"; \
	printf "Enter GROQ API key: "; \
	IFS= read -r -s api_key; \
	printf "\n"; \
	if [[ -z "$$api_key" ]]; then \
		echo "API key cannot be empty."; \
		exit 1; \
	fi; \
	escaped_key="$$(printf '%s' "$$api_key" | sed 's/\\/\\\\/g; s/\"/\\"/g')"; \
	printf '{"groq_api_key":"%s"}\n' "$$escaped_key" > "$$config_path"; \
	chmod 600 "$$config_path"; \
	echo "Saved API key to $$config_path"; \
	echo "OneScribe will read this key when launched from Finder, Xcode, or Terminal."

compile: ## Build the Xcode project
	@echo "Building $(PROJECT_NAME)..."
	xcodebuild -project $(PROJECT_NAME).xcodeproj \
		-scheme $(SCHEME) \
		-configuration $(CONFIGURATION) \
		build

codesign: ## Code sign the built app
	@echo "Code signing $(PROJECT_NAME)..."
	codesign --force --deep --sign - "$(APP_BUNDLE)"

install: ## Copy built app into /Applications so Accessibility can reference a stable path
	@echo "Copying $(PROJECT_NAME).app to $(INSTALL_DIR)..."
	rm -rf "$(INSTALLED_APP_BUNDLE)"
	ditto "$(APP_BUNDLE)" "$(INSTALLED_APP_BUNDLE)"
	@echo "Copied to $(INSTALLED_APP_BUNDLE)"

clean: ## Clean build artifacts
	@echo "Cleaning build..."
	xcodebuild -project $(PROJECT_NAME).xcodeproj \
		-scheme $(SCHEME) \
		clean

test: ## Run test suite
	@echo "Running tests..."
	xcodebuild test -project $(PROJECT_NAME).xcodeproj \
		-scheme $(SCHEME) \
		-destination '$(TEST_DESTINATION)'

access: ## Open macOS Accessibility settings page
	@echo "Opening Accessibility settings..."
	open "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"

build: clean compile test codesign install access ## full clean build/test/sign/copy, then opens accessibility settings
	@echo "Build complete!"

run: ## Open installed app from /Applications, fallback to build output
	@echo "Opening $(PROJECT_NAME)..."
	@if [[ -d "$(INSTALLED_APP_BUNDLE)" ]]; then \
		open "$(INSTALLED_APP_BUNDLE)"; \
	else \
		open "$(APP_BUNDLE)"; \
	fi

.PHONY: help setup compile codesign install clean test access build run
