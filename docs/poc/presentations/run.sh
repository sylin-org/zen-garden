#!/bin/bash

echo ""
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║                 ZEN GARDEN DIAGRAMS                       ║"
echo "║           Animated visualizations for video               ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""

# Check for Node.js
echo "[1/4] Checking for Node.js..."
if ! command -v node &> /dev/null; then
    echo ""
    echo "  ERROR: Node.js is not installed!"
    echo ""
    echo "  Install via:"
    echo "    macOS:  brew install node"
    echo "    Ubuntu: sudo apt install nodejs npm"
    echo "    Or:     https://nodejs.org/"
    echo ""
    exit 1
fi
echo "       Found Node.js $(node -v)"

# Check for npm
echo "[2/4] Checking for npm..."
if ! command -v npm &> /dev/null; then
    echo ""
    echo "  ERROR: npm is not installed!"
    echo ""
    exit 1
fi
echo "       Found npm v$(npm -v)"

# Install dependencies if needed
echo "[3/4] Checking dependencies..."
if [ ! -d "node_modules" ]; then
    echo "       Installing dependencies (first run, may take a minute)..."
    npm install --silent
    if [ $? -ne 0 ]; then
        echo ""
        echo "  ERROR: Failed to install dependencies!"
        echo "  Try running: npm install"
        echo ""
        exit 1
    fi
    echo "       Dependencies installed!"
else
    echo "       Dependencies already installed"
fi

# Start the dev server
echo "[4/4] Starting development server..."
echo ""
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║  Server starting at: http://localhost:5173                ║"
echo "║                                                           ║"
echo "║  - Use the menu to select diagrams                        ║"
echo "║  - Press F11 for fullscreen (for recording)               ║"
echo "║  - Press Ctrl+C to stop the server                        ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""

# Open browser (works on macOS and Linux)
if command -v xdg-open &> /dev/null; then
    xdg-open http://localhost:5173 &
elif command -v open &> /dev/null; then
    open http://localhost:5173 &
fi

npm run dev
