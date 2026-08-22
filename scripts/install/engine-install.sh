#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
INSTALL_DIR="$ROOT/scripts/install"

# ---------- UI ----------

if [[ -t 1 ]]; then
  GREEN=$'\033[0;32m'
  BLUE=$'\033[0;34m'
  YELLOW=$'\033[1;33m'
  RED=$'\033[0;31m'
  BOLD=$'\033[1m'
  RESET=$'\033[0m'
else
  GREEN="" BLUE="" YELLOW="" RED="" BOLD="" RESET=""
fi

step() { printf "\n${BLUE}▶${RESET} %s\n" "$1"; }
ok()   { printf "${GREEN}✓${RESET} %s\n" "$1"; }
warn() { printf "${YELLOW}⚠${RESET} %s\n" "$1"; }
fail() { printf "${RED}✗${RESET} %s\n" "$1"; exit 1; }

# ---------- Validation ----------

require_definition() {
  local file="$1"

  [[ -f "$file" ]] || fail "Missing service definition: $file"

  if grep -q "/CHANGE-ME" "$file"; then
    fail "$file still contains CHANGE-ME placeholders"
  fi
}

require_binary() {
  local bin="$1"

  [[ -n "$bin" ]] || fail "Engine binary not configured"
  [[ -x "$bin" ]] || fail "Engine binary not executable: $bin"
}

# ---------- Build ----------

step "Building Valqeron Engine"
cargo build --release -p valqeron-engine >/dev/null
ok "Release binary built"

case "$(uname -s)" in

# =============================================================================
# macOS / launchd
# =============================================================================

Darwin)

  LABEL="io.valqeron.engine"
  SERVICE="gui/$(id -u)/$LABEL"

  SRC="$INSTALL_DIR/$LABEL.plist"
  DST="$HOME/Library/LaunchAgents/$LABEL.plist"

  step "Validating LaunchAgent"

  require_definition "$SRC"
  plutil -lint "$SRC" >/dev/null || fail "Invalid plist"

  BIN="$(plutil -extract ProgramArguments.0 raw -o - "$SRC")"
  DB="$(plutil -extract EnvironmentVariables.VALQERON_DB raw -o - "$SRC")"
  SOCKET="$(plutil -extract EnvironmentVariables.VALQERON_SOCKET raw -o - "$SRC")"
  LOG="$(plutil -extract EnvironmentVariables.VALQERON_ENGINE_LOG_FILE raw -o - "$SRC")"

  require_binary "$BIN"

  [[ -n "$DB" ]] || fail "VALQERON_DB missing"
  [[ -n "$SOCKET" ]] || fail "VALQERON_SOCKET missing"
  [[ -n "$LOG" ]] || fail "VALQERON_ENGINE_LOG_FILE missing"

  ok "LaunchAgent is valid"

  step "Installing service"

  mkdir -p \
    "$HOME/Library/LaunchAgents" \
    "$(dirname "$DB")" \
    "$(dirname "$SOCKET")" \
    "$(dirname "$LOG")"

  launchctl bootout "$SERVICE" 2>/dev/null || true

  cp "$SRC" "$DST"

  launchctl bootstrap "gui/$(id -u)" "$DST"
  launchctl kickstart -k "$SERVICE"

  sleep 1

  PID="$(launchctl print "$SERVICE" 2>/dev/null | awk '/pid =/{print $3}')"

  [[ -n "${PID:-}" ]] || fail "Engine failed to start"

  ok "Engine running (PID $PID)"

  echo
  printf "${BOLD}Installed${RESET}\n"
  printf "  Binary    %s\n" "$BIN"
  printf "  Database  %s\n" "$DB"
  printf "  Socket    %s\n" "$SOCKET"
  printf "  Log       %s\n" "$LOG"

  echo
  printf "${BOLD}Useful commands${RESET}\n"
  printf "  launchctl print %s\n" "$SERVICE"
  printf "  tail -f '%s'\n" "$LOG"

  ;;

# =============================================================================
# Linux / systemd
# =============================================================================

Linux)

  UNIT="valqeron-engine.service"

  SRC="$INSTALL_DIR/$UNIT"
  UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
  DST="$UNIT_DIR/$UNIT"

  step "Validating systemd unit"

  require_definition "$SRC"

  BIN="$(sed -n 's/^ExecStart=//p' "$SRC" | tr -d '"' | awk '{print $1}')"

  DB="$(grep '^Environment=VALQERON_DB=' "$SRC" | cut -d= -f3-)"
  SOCKET="$(grep '^Environment=VALQERON_SOCKET=' "$SRC" | cut -d= -f3-)"
  LOG="$(grep '^Environment=VALQERON_ENGINE_LOG_FILE=' "$SRC" | cut -d= -f3-)"

  require_binary "$BIN"

  [[ -n "$DB" ]] || fail "VALQERON_DB missing"
  [[ -n "$SOCKET" ]] || fail "VALQERON_SOCKET missing"
  [[ -n "$LOG" ]] || fail "VALQERON_ENGINE_LOG_FILE missing"

  ok "systemd unit is valid"

  step "Installing service"

  mkdir -p "$UNIT_DIR"

  cp "$SRC" "$DST"

  systemctl --user daemon-reload
  systemctl --user enable "$UNIT" >/dev/null
  systemctl --user restart "$UNIT"

  sleep 1

  systemctl --user is-active --quiet "$UNIT" \
    || fail "Engine failed to start"

  ok "Engine running"

  echo
  printf "${BOLD}Installed${RESET}\n"
  printf "  Binary    %s\n" "$BIN"
  printf "  Database  %s\n" "$DB"
  printf "  Socket    %s\n" "$SOCKET"
  printf "  Log       %s\n" "$LOG"

  echo
  printf "${BOLD}Useful commands${RESET}\n"
  printf "  systemctl --user status %s\n" "$UNIT"
  printf "  journalctl --user -u %s -f\n" "$UNIT"

  ;;

*)

  fail "Unsupported platform: $(uname -s)"

  ;;

esac