#!/usr/bin/env bash
# Starts a private PipeWire session with a virtual speaker ("null sink"), so
# audio code can be tested on a machine with no sound card. ALSA's default
# device then goes to PipeWire, exactly as on a desktop Linux system.
#
# Needs: pipewire pipewire-alsa pipewire-jack wireplumber dbus
# Prints the environment variables to export; in GitHub Actions they are also
# appended to $GITHUB_ENV for later steps.
set -euo pipefail

runtime_dir="${XDG_RUNTIME_DIR:-/tmp/virtual-audio-$(id -u)}"
mkdir -p "$runtime_dir"
chmod 700 "$runtime_dir"
export XDG_RUNTIME_DIR="$runtime_dir"
export DBUS_SESSION_BUS_ADDRESS="unix:path=$runtime_dir/bus"

dbus-daemon --session --fork --address="$DBUS_SESSION_BUS_ADDRESS"
pipewire >"$runtime_dir/pipewire.log" 2>&1 &
sleep 1
wireplumber >"$runtime_dir/wireplumber.log" 2>&1 &
sleep 2
pw-cli create-node adapter '{ factory.name=support.null-audio-sink node.name=test-speakers node.description="Test speakers" media.class=Audio/Sink object.linger=true audio.position=[FL FR] }' >/dev/null
sleep 1
wpctl status | sed -n '/^Audio/,/^Video/p'

if [ -n "${GITHUB_ENV:-}" ]; then
  echo "XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR" >>"$GITHUB_ENV"
  echo "DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS" >>"$GITHUB_ENV"
fi
echo "export XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS"
