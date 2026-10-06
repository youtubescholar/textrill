#!/bin/sh
# Install the rust-stable SDK extension into the SDK used by the build.
set -eu
sdk-ext add org.freedesktop.Sdk 24.08 rust-stable
