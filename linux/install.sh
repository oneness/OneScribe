#!/usr/bin/env bash
# Build and install OneScribe for Linux, then wire it up as a user service.
#
# The build is a Nix package (default.nix), so it needs Nix but no manual
# toolchain setup: CMake, Vulkan headers and the shader compiler all come from
# the expression. The GGUF model is not bundled — the first start downloads it
# automatically (~697 MB, resumable).
set -euo pipefail

PREFIX="${ONESCRIBE_PREFIX:-$HOME/.local/opt/onescribe}"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
UNIT="$UNIT_DIR/onescribe.service"
BIN="$PREFIX/bin/onescribe-linux-core"

usage() {
  cat <<'EOF'
Usage: ./install.sh [--force-unit] [nix-build args...]

Builds OneScribe from this directory, installs it to $ONESCRIBE_PREFIX
(default: ~/.local/opt/onescribe), and runs it as the systemd user service
onescribe.service.

  --force-unit          rewrite ~/.config/systemd/user/onescribe.service
  --arg vulkan false    CPU-only build (the default build uses the GPU)
  -h, --help            show this help

Re-run the same command after pulling to rebuild and restart.
EOF
}

force_unit=0
passthrough=()
for arg in "$@"; do
  case "$arg" in
  -h | --help)
    usage
    exit 0
    ;;
  --force-unit) force_unit=1 ;;
  *) passthrough+=("$arg") ;;
  esac
done

cd "$(dirname "${BASH_SOURCE[0]}")"

if ! command -v nix-build >/dev/null 2>&1; then
  echo "error: nix-build not found — this installer needs Nix." >&2
  exit 1
fi
if [ ! -f default.nix ]; then
  echo "error: default.nix not found next to this script ($PWD)." >&2
  exit 1
fi

# Prefer systemd's %h specifier when the prefix lives under $HOME (survives a
# home rename); otherwise use the absolute path. Both forms are equivalent.
abs_exec="ExecStart=$BIN daemon"
if [[ "$BIN" == "$HOME"/* ]]; then
  wanted="ExecStart=%h/${BIN#"$HOME"/} daemon"
else
  wanted="$abs_exec"
fi

write_unit() {
  mkdir -p "$UNIT_DIR"
  cat >"$UNIT" <<EOF
[Unit]
Description=OneScribe Linux transcription daemon
After=network.target pipewire.service wireplumber.service

[Service]
Type=simple
$wanted
Restart=on-failure
RestartSec=3

[Install]
WantedBy=default.target
EOF
  echo "==> wrote $UNIT"
}

echo "==> building (a couple of minutes on a first build)"
nix-build default.nix --out-link "$PREFIX" "${passthrough[@]}"

if [ ! -x "$BIN" ]; then
  echo "error: build finished but $BIN is missing or not executable." >&2
  exit 1
fi
echo "==> installed $BIN"

if [ -f "$UNIT" ]; then
  current="$(grep -m1 '^ExecStart=' "$UNIT" || true)"
  if [ "$current" != "$wanted" ] && [ "$current" != "$abs_exec" ]; then
    if [ "$force_unit" -eq 1 ]; then
      write_unit
    else
      cat >&2 <<EOF

error: $UNIT already exists and points somewhere else:
  current:  ${current:-<no ExecStart line>}
  expected: $wanted

Update that line by hand, or re-run with --force-unit to rewrite the unit.
EOF
      exit 1
    fi
  fi
else
  write_unit
fi

echo "==> enabling onescribe.service"
systemctl --user daemon-reload
systemctl --user enable onescribe.service
systemctl --user restart onescribe.service
sleep 3

status="$(systemctl --user is-active onescribe.service || true)"
echo "==> onescribe.service: $status"
if [ "$status" != "active" ]; then
  echo "--- recent log ---" >&2
  journalctl --user -u onescribe.service -n 20 --no-pager >&2 || true
  exit 1
fi

# Look only at the running process's log: a previous run may have used the other
# backend, and its lines are still in the journal.
main_pid="$(systemctl --user show onescribe.service -p MainPID --value)"
startup_log="$(journalctl --user -u onescribe.service _PID="$main_pid" --no-pager 2>/dev/null || true)"
if grep -q 'using vulkan backend' <<<"$startup_log"; then
  echo "==> backend: Vulkan (GPU)"
elif grep -q 'CPU_Mapped' <<<"$startup_log"; then
  echo "==> backend: CPU (no Vulkan device found)"
fi
