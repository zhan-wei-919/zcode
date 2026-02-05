#!/usr/bin/env bash
#
# Uninstall zcode from a bin directory.
#
# Safety rule: only remove the exact target path we manage; never `which zcode`
# and remove that, to avoid deleting unrelated binaries.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./uninstall.sh [OPTIONS]

Remove zcode from a bin directory.

Options:
  -u, --user           Uninstall from ~/.local/bin (default when not root)
  -s, --system         Uninstall from /usr/local/bin (default when running as root)
  --bin-dir DIR        Uninstall from a custom bin directory (overrides --user/--system)
  -f, --force          Do not prompt
  --remove-config      Also remove config/cache directories (~/.cache/.zcode or ~/Library/Caches/.zcode)
  -h, --help           Show this help

Examples:
  ./uninstall.sh --user
  sudo ./uninstall.sh --system
  ./uninstall.sh --bin-dir /tmp/bin --force
EOF
}

force=0
remove_config=0
bin_dir=""

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
    -f|--force)
      force=1
      shift
      ;;
    --remove-config)
      remove_config=1
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
  echo "error: system uninstall requires root permissions (try: sudo ./uninstall.sh --system)" >&2
  exit 1
fi

dest="${bin_dir}/zcode"

if [[ ! -e "${dest}" ]]; then
  echo "not installed: ${dest}"
  exit 0
fi

if [[ "${force}" -ne 1 ]]; then
  if [[ -t 0 ]]; then
    read -r -p "remove ${dest}? [y/N] " ans
    if [[ "${ans}" != "y" && "${ans}" != "Y" ]]; then
      echo "aborted"
      exit 0
    fi
  else
    echo "error: refusing to uninstall without --force in non-interactive mode" >&2
    exit 1
  fi
fi

rm -f -- "${dest}"
echo "removed: ${dest}"

if [[ "${remove_config}" -eq 1 ]]; then
  if [[ "${OSTYPE}" == "darwin"* ]]; then
    cfg_dir="${HOME}/Library/Caches/.zcode"
  else
    cfg_dir="${HOME}/.cache/.zcode"
  fi

  if [[ -d "${cfg_dir}" ]]; then
    rm -rf -- "${cfg_dir}"
    echo "removed: ${cfg_dir}"
  fi
fi
