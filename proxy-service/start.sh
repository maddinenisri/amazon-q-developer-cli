#!/bin/bash
# Start the Q CLI Bedrock Proxy Service

set -e

echo "Starting Q CLI Bedrock Proxy Service..."

# Check if Poetry is installed
if ! command -v poetry &> /dev/null; then
    echo "Error: Poetry is not installed. Please install Poetry first."
    echo "Visit: https://python-poetry.org/docs/#installation"
    exit 1
fi

# Install dependencies if needed
if [ ! -d ".venv" ]; then
    echo "Installing dependencies..."
    poetry install
fi

# Start the server
echo "Starting proxy server on http://localhost:8080"
poetry run proxy-server