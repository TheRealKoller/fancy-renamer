#!/bin/sh
# Entfernt nur die Nautilus-Erweiterung; andere Dateien bleiben (idempotent).
set -eu

DATA_HOME=${XDG_DATA_HOME:-${HOME}/.local/share}
TARGET="$DATA_HOME/nautilus-python/extensions/fancy-renamer.py"

if [ -e "$TARGET" ]; then
    rm -f "$TARGET"
    echo "Entfernt: $TARGET"
fi
echo "Hinweis: Nautilus neu starten: nautilus -q"
