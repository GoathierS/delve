#!/bin/sh
# Installs delve for the current user, with an entry in the applications menu.
#   ./install.sh            build and install
#   ./install.sh --remove   uninstall (saves and gold are kept)
set -eu

cd "$(dirname "$0")"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
bin="$HOME/.local/bin"
apps="$data/applications"
icons="$data/icons/hicolor/256x256/apps"

refresh() {
    command -v update-desktop-database >/dev/null && update-desktop-database -q "$apps" || true
    command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$data/icons/hicolor" || true
}

if [ "${1:-}" = "--remove" ]; then
    rm -f "$bin/delve" "$bin/delve-term" "$apps/delve.desktop" "$icons/delve.png"
    refresh
    echo "delve removed. Saves and gold stay in $data/delve."
    exit 0
fi

cargo build --release --workspace
mkdir -p "$bin" "$apps" "$icons"
install -m 755 target/release/delve target/release/delve-term "$bin/"
install -m 644 crates/gui/assets/delve.png "$icons/delve.png"
cat > "$apps/delve.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=delve
GenericName=Roguelike
Comment=Go down the dungeon and seize the Amulet
Exec=$bin/delve
Icon=delve
Terminal=false
Categories=Game;RolePlaying;
Keywords=roguelike;dungeon;rpg;
StartupWMClass=delve
EOF
refresh
echo "delve installed: look for it in your applications (Super key, then type delve)."
