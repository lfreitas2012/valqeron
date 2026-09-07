#!/usr/bin/env bash
set -euo pipefail

LABEL="io.valqeron.engine"
SERVICE="gui/$(id -u)/$LABEL"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"

green='\033[0;32m'
blue='\033[0;34m'
yellow='\033[1;33m'
red='\033[0;31m'
bold='\033[1m'
reset='\033[0m'

step() { printf "\n${blue}▶${reset} %s\n" "$1"; }
ok()   { printf "${green}✓${reset} %s\n" "$1"; }
warn() { printf "${yellow}⚠${reset} %s\n" "$1"; }
fail() { printf "${red}✗${reset} %s\n" "$1"; exit 1; }

wait_for_unload() {
    local deadline=$((SECONDS + 30))
    while launchctl print "$SERVICE" >/dev/null 2>&1; do
        if [ "$SECONDS" -ge "$deadline" ]; then
            fail "Timed out waiting for launchd to unload $LABEL"
        fi
        sleep 0.2
    done
}

step "Stopping engine"

if launchctl print "$SERVICE" >/dev/null 2>&1; then
    launchctl bootout "$SERVICE"
    wait_for_unload
    ok "Engine stopped"
else
    warn "Engine is not running"
fi

step "Removing LaunchAgent"

if [[ -f "$PLIST" ]]; then
    rm -f "$PLIST"
    ok "Removed $PLIST"
else
    warn "LaunchAgent already removed"
fi

echo
printf "${bold}Engine successfully uninstalled.${reset}\n"