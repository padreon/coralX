#!/usr/bin/env bash
set -e

OS="$(uname -s)"

# Linux: install system libraries required by PyQt6
if [ "$OS" = "Linux" ]; then
    if command -v apt-get &>/dev/null; then
        sudo apt-get update -qq
        sudo apt-get install -y \
            libegl1 libegl-mesa0 \
            libxcb-cursor0 libxcb-icccm4 libxcb-image0 libxcb-keysyms1 \
            libxcb-randr0 libxcb-render-util0 libxcb-shape0 libxcb-xfixes0 \
            libxcb-xkb1 libxkbcommon-x11-0 libxcb-util1 libxcb-xinerama0 \
            libxcb-xinput0 \
            fonts-noto-color-emoji fonts-noto
    else
        echo "Warning: apt-get not found. Install Qt system libraries manually."
    fi
fi

# macOS: ensure Xcode CLI tools are present (needed to build some pip packages)
if [ "$OS" = "Darwin" ]; then
    if ! xcode-select -p &>/dev/null; then
        echo "Installing Xcode Command Line Tools..."
        xcode-select --install
        echo "Re-run this script after the installation completes."
        exit 1
    fi
fi

# Python: use python3 on Linux/macOS, python on Windows Git Bash
PYTHON="python3"
if ! command -v python3 &>/dev/null; then
    PYTHON="python"
fi

# Create virtual environment
$PYTHON -m venv .venv

# Activate and install
if [ "$OS" = "MINGW"* ] || [ "$OS" = "MSYS"* ] || [ -n "$WINDIR" ]; then
    # Windows (Git Bash / MSYS2)
    .venv/Scripts/pip install --upgrade pip -q
    .venv/Scripts/pip install -r requirements.txt
    echo ""
    echo "Setup complete. Run the app with:"
    echo "  .venv\\Scripts\\python -m src.main"
else
    # Linux / macOS
    .venv/bin/pip install --upgrade pip -q
    .venv/bin/pip install -r requirements.txt
    echo ""
    echo "Setup complete. Run the app with:"
    if [ "$OS" = "Linux" ]; then
        echo "  .venv/bin/python -m src.main"
    else
        echo "  .venv/bin/python -m src.main"
    fi
fi
