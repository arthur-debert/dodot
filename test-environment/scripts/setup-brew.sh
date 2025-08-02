#!/bin/bash
# Setup Homebrew on Linux
set -euo pipefail

echo "Installing Homebrew for Linux..."

# Install Homebrew dependencies
sudo apt-get update
sudo apt-get install -y build-essential procps curl file git

# Install Homebrew
NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Add Homebrew to PATH for current shell
eval "$(/home/linuxbrew/.linuxbrew/bin/brew shellenv)"

# Add Homebrew to zsh profile
echo 'eval "$(/home/linuxbrew/.linuxbrew/bin/brew shellenv)"' >> ~/.zprofile
echo 'eval "$(/home/linuxbrew/.linuxbrew/bin/brew shellenv)"' >> ~/.zshrc

# Verify installation
brew --version

echo "Homebrew installation complete!"