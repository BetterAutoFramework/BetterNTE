#!/bin/bash
# Build the test FFI plugin as a shared library.
# Run from the plugin directory: bash build.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

case "$(uname -s)" in
    Linux*)
        gcc -shared -fPIC -o plugin.so plugin.c
        echo "Built plugin.so (Linux)"
        ;;
    Darwin*)
        cc -shared -fPIC -o plugin.dylib plugin.c
        echo "Built plugin.dylib (macOS)"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        gcc -shared -o plugin.dll plugin.c
        echo "Built plugin.dll (Windows/MinGW)"
        ;;
    *)
        echo "Unknown platform — compile manually:"
        echo "  gcc -shared -fPIC -o plugin.so plugin.c"
        exit 1
        ;;
esac
