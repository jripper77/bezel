#!/usr/bin/env bash
# Builds Bezel in release mode and installs it for the current user, without
# sudo: the `bezel` CLI and `bezel-studio` into ~/.local/bin, the desktop entry
# and the icons into ~/.local/share. Every phase ends by running this, so the
# latest build can be tried on the real screen.
#
# The version is stamped from git: the last `vX.Y.Z` tag, plus `-dev.N+sha`
# when there are commits after it (0.0.0 before the first release).
#
# Linux permissions: the screen's serial port must be openable by the user.
# The packages install packaging/linux/60-bezel.rules; a from-source install
# prints the one sudo command that does it, and never runs it itself.
set -euo pipefail
export LC_ALL=C.UTF-8
cd "$(dirname "$0")/.."

prefix="${BEZEL_PREFIX:-$HOME/.local}"
bin_dir="$prefix/bin"
apps_dir="$prefix/share/applications"
icons_dir="$prefix/share/icons/hicolor"
app_id="io.github.slipalison.bezel"

version() {
  local tag base count sha
  tag=$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || true)
  base="${tag#v}"
  base="${base:-0.0.0}"
  if [ -n "$tag" ]; then
    count=$(git rev-list --count "$tag"..HEAD)
  else
    count=$(git rev-list --count HEAD)
  fi
  sha=$(git rev-parse --short HEAD)
  if [ "$count" = "0" ]; then
    printf '%s\n' "$base"
  else
    printf '%s-dev.%s+%s\n' "$base" "$count" "$sha"
  fi
}

BEZEL_VERSION="$(version)"
export BEZEL_VERSION
printf 'Building Bezel %s (release)...\n' "$BEZEL_VERSION"
cargo build --release --locked -p bezel -p bezel-studio

install -Dm755 target/release/bezel "$bin_dir/bezel"
install -Dm755 target/release/bezel-studio "$bin_dir/bezel-studio"

# Bundled themes and their fonts, where both `bezel` and the studio look for
# them (~/.local/share/bezel/themes); replaced whole so removed themes go too.
themes_dir="$prefix/share/bezel/themes"
rm -rf "$themes_dir"
mkdir -p "$(dirname "$themes_dir")"
cp -r themes "$themes_dir"

# The systemd user unit that runs a theme without a window (not enabled here).
install -Dm644 packaging/linux/bezel-run@.service "$HOME/.config/systemd/user/bezel-run@.service"
systemctl --user daemon-reload >/dev/null 2>&1 || true

icons_src=apps/bezel-studio/src-tauri/icons
install -Dm644 "$icons_src/32x32.png" "$icons_dir/32x32/apps/bezel.png"
install -Dm644 "$icons_src/64x64.png" "$icons_dir/64x64/apps/bezel.png"
install -Dm644 "$icons_src/128x128.png" "$icons_dir/128x128/apps/bezel.png"
install -Dm644 "$icons_src/128x128@2x.png" "$icons_dir/256x256/apps/bezel.png"
install -Dm644 "$icons_src/icon.svg" "$icons_dir/scalable/apps/bezel.svg"

mkdir -p "$apps_dir"
cat > "$apps_dir/$app_id.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Bezel
GenericName=Smart Screen Studio
GenericName[pt_BR]=Estúdio de Telas USB
Comment=Design themes for and drive USB smart screens (Turing, TURZX and compatible)
Comment[pt_BR]=Crie temas e controle telas USB (Turing, TURZX e compatíveis)
Exec=$bin_dir/bezel-studio
Icon=bezel
Terminal=false
Categories=Utility;System;Monitor;
Keywords=turing;turzx;smart screen;system monitor;lcd;sensor;
StartupNotify=true
StartupWMClass=bezel-studio
EOF

update-desktop-database "$apps_dir" >/dev/null 2>&1 || true
gtk-update-icon-cache -q -t "$icons_dir" >/dev/null 2>&1 || true

printf '\nInstalled Bezel %s:\n  %s\n  %s\n  %s\n  %s\n' \
  "$BEZEL_VERSION" "$bin_dir/bezel" "$bin_dir/bezel-studio" "$apps_dir/$app_id.desktop" "$themes_dir"
"$bin_dir/bezel" --version

if [ "$(uname -s)" = "Linux" ] && [ ! -f /etc/udev/rules.d/60-bezel.rules ] &&
   [ ! -f /usr/lib/udev/rules.d/60-bezel.rules ]; then
  printf '\nTo let your user open every supported screen without root, run once:\n'
  printf '  sudo install -m644 packaging/linux/60-bezel.rules /etc/udev/rules.d/ && sudo udevadm control --reload && sudo udevadm trigger\n'
fi
