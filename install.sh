#!/usr/bin/env bash
#
# Install zcode from this repository.
#
# This is intended for source builds (requires Rust/cargo). For release binaries,
# prefer a dedicated installer in the release pipeline.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./install.sh [OPTIONS]

Install zcode into a bin directory.

Options:
  -u, --user           Install to ~/.local/bin (default when not root)
  -s, --system         Install to /usr/local/bin (default when running as root)
  --bin-dir DIR        Install to a custom bin directory (overrides --user/--system)
  --debug              Install target/debug/zcode (default: release)
  --binary PATH        Install a specific binary (skips cargo build unless missing and --no-build is not set)
  --no-build           Do not run cargo build; require the binary to exist
  -f, --force          Overwrite existing zcode without prompting
  -h, --help           Show this help

Examples:
  ./install.sh --user
  sudo ./install.sh --system
  ./install.sh --bin-dir /tmp/bin --binary ./target/release/zcode --no-build --force
EOF
}

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

mode="release"
no_build=0
force=0
bin_dir=""
binary_path=""

# Default install target: system for root, user otherwise.
default_target="user"
if [[ "${EUID}" -eq 0 ]]; then
  default_target="system"
fi

target="${default_target}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    -u|--user)
      target="user"
      shift
      ;;
    -s|--system)
      target="system"
      shift
      ;;
    --bin-dir)
      bin_dir="${2:-}"
      shift 2
      ;;
    --debug)
      mode="debug"
      shift
      ;;
    --binary)
      binary_path="${2:-}"
      shift 2
      ;;
    --no-build)
      no_build=1
      shift
      ;;
    -f|--force)
      force=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      echo "" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "${bin_dir}" ]]; then
  if [[ "${target}" == "system" ]]; then
    bin_dir="/usr/local/bin"
  else
    bin_dir="${HOME}/.local/bin"
  fi
fi

if [[ "${bin_dir}" == "/usr/local/bin" ]] && [[ "${EUID}" -ne 0 ]]; then
  echo "error: system install requires root permissions (try: sudo ./install.sh --system)" >&2
  exit 1
fi

mkdir -p "${bin_dir}"

if [[ -z "${binary_path}" ]]; then
  if [[ "${mode}" == "debug" ]]; then
    binary_path="${script_dir}/target/debug/zcode"
  else
    binary_path="${script_dir}/target/release/zcode"
  fi
fi

if [[ ! -f "${binary_path}" ]]; then
  if [[ "${no_build}" -eq 1 ]]; then
    echo "error: binary not found: ${binary_path}" >&2
    echo "hint: build it first or omit --no-build" >&2
    exit 1
  fi

  if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found in PATH" >&2
    echo "hint: install Rust toolchain from https://rustup.rs/ and retry" >&2
    exit 1
  fi

  echo "building zcode (${mode})..."
  (
    cd "${script_dir}"
    if [[ "${mode}" == "debug" ]]; then
      cargo build
    else
      cargo build --release
    fi
  )
fi

if [[ ! -f "${binary_path}" ]]; then
  echo "error: expected binary after build but not found: ${binary_path}" >&2
  exit 1
fi

dest="${bin_dir}/zcode"
if [[ -e "${dest}" && "${force}" -ne 1 ]]; then
  if [[ -t 0 ]]; then
    read -r -p "zcode already exists at ${dest}. Overwrite? [y/N] " ans
    if [[ "${ans}" != "y" && "${ans}" != "Y" ]]; then
      echo "aborted"
      exit 0
    fi
  else
    echo "error: ${dest} already exists (use --force to overwrite)" >&2
    exit 1
  fi
fi

if command -v install >/dev/null 2>&1; then
  install -m 755 "${binary_path}" "${dest}"
else
  cp "${binary_path}" "${dest}"
  chmod 755 "${dest}"
fi

echo "installed: ${dest}"

if [[ ":${PATH}:" != *":${bin_dir}:"* ]]; then
  echo "note: ${bin_dir} is not in PATH"
  echo "      add this to your shell rc:"
  echo "      export PATH=\"${bin_dir}:\$PATH\""
fi
