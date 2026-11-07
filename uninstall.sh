#!/bin/sh
# Entfernt die Dateien, die install.sh abgelegt hat. Andere Dateien bleiben.
set -eu

PREFIX=${PREFIX:-${HOME}/.local}
BIN_DIR=${XDG_BIN_HOME:-${PREFIX}/bin}
DATA_HOME=${XDG_DATA_HOME:-${PREFIX}/share}
APP_ID=io.github.therealKoller.FancyRenamer

for file in \
    "$BIN_DIR/fancy-renamer" \
    "$DATA_HOME/applications/${APP_ID}.desktop" \
    "$DATA_HOME/icons/hicolor/scalable/apps/${APP_ID}.svg"
do
    if [ -e "$file" ]; then
        rm -f "$file"
        echo "Entfernt: $file"
    fi
done

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t "$DATA_HOME/icons/hicolor" 2>/dev/null || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q "$DATA_HOME/applications" 2>/dev/null || true
fi
