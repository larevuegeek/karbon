#!/bin/sh
# Installs the `karbon` CLI from the prebuilt binaries of a GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/larevuegeek/karbon/main/install.sh | sh
#
# Environment:
#   KARBON_VERSION      release to install, e.g. 0.5.1 (default: the latest)
#   KARBON_INSTALL_DIR  where to put the binary (default: ~/.cargo/bin if it exists,
#                       else ~/.local/bin)
#   KARBON_DOWNLOAD_URL base URL holding the archives, for a mirror (default: the GitHub
#                       release)
#
# Karbon projects are Rust projects: you still need Rust (https://rustup.rs) to run them.
# This script only saves compiling the CLI itself (`cargo install karbon-cli`).

set -eu

REPO="larevuegeek/karbon"

say() { printf '%s\n' "$*"; }
fail() {
    say "error: $*" >&2
    say "You can always build the CLI from source: cargo install karbon-cli" >&2
    exit 1
}

case "$(uname -s)" in
    Linux) os="unknown-linux-gnu" ;;
    Darwin) os="apple-darwin" ;;
    *) fail "no prebuilt binary for $(uname -s) (on Windows, use install.ps1)" ;;
esac

case "$(uname -m)" in
    x86_64 | amd64) arch="x86_64" ;;
    aarch64 | arm64) arch="aarch64" ;;
    *) fail "no prebuilt binary for the $(uname -m) architecture" ;;
esac

# The Linux binaries are linked against glibc; musl systems (Alpine) cannot run them.
if [ "$os" = "unknown-linux-gnu" ] && ldd --version 2>&1 | grep -qi musl; then
    fail "no prebuilt binary for musl-based systems such as Alpine"
fi

target="${arch}-${os}"
archive="karbon-${target}.tar.gz"

if [ -n "${KARBON_DOWNLOAD_URL:-}" ]; then
    base="${KARBON_DOWNLOAD_URL%/}"
elif [ -n "${KARBON_VERSION:-}" ]; then
    base="https://github.com/${REPO}/releases/download/v${KARBON_VERSION#v}"
else
    base="https://github.com/${REPO}/releases/latest/download"
fi

if command -v curl >/dev/null 2>&1; then
    fetch() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
    fetch() { wget -q "$1" -O "$2"; }
else
    fail "curl or wget is required"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "Downloading ${base}/${archive}"
fetch "${base}/${archive}" "${tmp}/${archive}" || fail "download failed — is there a release with binaries for ${target}?"

# Verify the checksum published next to the archive.
if fetch "${base}/${archive}.sha256" "${tmp}/${archive}.sha256" 2>/dev/null; then
    expected="$(cut -d ' ' -f 1 <"${tmp}/${archive}.sha256")"
    if command -v sha256sum >/dev/null 2>&1; then
        actual="$(sha256sum "${tmp}/${archive}" | cut -d ' ' -f 1)"
    elif command -v shasum >/dev/null 2>&1; then
        actual="$(shasum -a 256 "${tmp}/${archive}" | cut -d ' ' -f 1)"
    else
        actual=""
        say "warning: no sha256sum or shasum — checksum not verified"
    fi
    if [ -n "$actual" ] && [ "$actual" != "$expected" ]; then
        fail "checksum mismatch for ${archive} (expected ${expected}, got ${actual})"
    fi
else
    say "warning: no checksum published for ${archive} — not verified"
fi

tar -xzf "${tmp}/${archive}" -C "$tmp"

if [ -n "${KARBON_INSTALL_DIR:-}" ]; then
    dir="$KARBON_INSTALL_DIR"
elif [ -d "${HOME}/.cargo/bin" ]; then
    dir="${HOME}/.cargo/bin"
else
    dir="${HOME}/.local/bin"
fi

mkdir -p "$dir"
install -m 755 "${tmp}/karbon-${target}/karbon" "${dir}/karbon"

say ""
say "Installed: ${dir}/karbon ($("${dir}/karbon" --version 2>/dev/null || echo 'version unknown'))"

case ":${PATH}:" in
    *":${dir}:"*) ;;
    *) say "Add ${dir} to your PATH to run \`karbon\` from anywhere." ;;
esac

command -v cargo >/dev/null 2>&1 || say "Rust is not installed yet — get it at https://rustup.rs, Karbon projects need it."

say ""
say "Next: karbon new my-app && cd my-app && karbon dev"
