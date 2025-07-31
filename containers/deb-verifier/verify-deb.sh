#!/bin/bash
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Check if URL is provided
if [ $# -eq 0 ]; then
    echo -e "${RED}Error: No DEB URL provided${NC}"
    echo "Usage: docker run --rm deb-verifier <deb-url>"
    exit 1
fi

DEB_URL="$1"
DEB_FILE="package.deb"

echo -e "${BLUE}=== DEB Package Verification ===${NC}"
echo -e "${BLUE}URL: ${DEB_URL}${NC}"
echo

# Download the deb package
echo -e "${YELLOW}1. Downloading DEB package...${NC}"
if wget -q -O "$DEB_FILE" "$DEB_URL"; then
    echo -e "${GREEN}✓ Download successful${NC}"
else
    echo -e "${RED}✗ Download failed${NC}"
    exit 1
fi

# Show package information
echo -e "\n${YELLOW}2. Package information:${NC}"
dpkg-deb -I "$DEB_FILE"

# Install the package
echo -e "\n${YELLOW}3. Installing package...${NC}"
if dpkg -i "$DEB_FILE"; then
    echo -e "${GREEN}✓ Package installed with dpkg${NC}"
else
    echo -e "${YELLOW}⚠ dpkg failed, trying apt-get install -f${NC}"
    if apt-get install -f -y; then
        echo -e "${GREEN}✓ Dependencies resolved${NC}"
    else
        echo -e "${RED}✗ Installation failed${NC}"
        exit 1
    fi
fi

# Verify installation
echo -e "\n${YELLOW}4. Verifying installation...${NC}"

# Debug: Show where files were installed
echo -e "${BLUE}Checking for dodot binary...${NC}"
echo "PATH=$PATH"
echo "Looking for dodot in common locations:"
for location in /usr/bin/dodot /usr/local/bin/dodot /bin/dodot; do
    if [ -f "$location" ]; then
        echo -e "${GREEN}Found: $location${NC}"
        ls -la "$location"
    else
        echo -e "${YELLOW}Not found: $location${NC}"
    fi
done

# Check if binary exists
if command -v dodot &> /dev/null; then
    echo -e "${GREEN}✓ Binary found at: $(which dodot)${NC}"
else
    echo -e "${RED}✗ Binary not found in PATH${NC}"
    # Try to run it directly if we found it
    if [ -f /usr/bin/dodot ]; then
        echo -e "${YELLOW}Trying direct execution of /usr/bin/dodot${NC}"
        /usr/bin/dodot version || echo "Direct execution failed"
    fi
    exit 1
fi

# Check version
echo -e "\n${YELLOW}5. Version check:${NC}"
if dodot version; then
    echo -e "${GREEN}✓ Version command successful${NC}"
else
    echo -e "${RED}✗ Version command failed${NC}"
    exit 1
fi

# Check help
echo -e "\n${YELLOW}6. Help check:${NC}"
if dodot --help > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Help command successful${NC}"
else
    echo -e "${RED}✗ Help command failed${NC}"
    exit 1
fi

# Check man page
echo -e "\n${YELLOW}7. Man page check:${NC}"
if [ -f /usr/share/man/man1/dodot.1.gz ]; then
    echo -e "${GREEN}✓ Man page installed at /usr/share/man/man1/dodot.1.gz${NC}"
    # Test if man command works
    if man dodot >/dev/null 2>&1; then
        echo -e "${GREEN}✓ 'man dodot' command works${NC}"
    else
        echo -e "${YELLOW}⚠ Man page installed but 'man dodot' doesn't work (might need mandb update)${NC}"
    fi
else
    echo -e "${RED}✗ Man page not found at /usr/share/man/man1/dodot.1.gz${NC}"
fi

# Check completions
echo -e "\n${YELLOW}8. Shell completions check:${NC}"
COMPLETION_COUNT=0
if [ -f /usr/share/bash-completion/completions/dodot ]; then
    echo -e "${GREEN}✓ Bash completion installed${NC}"
    ((COMPLETION_COUNT++))
else
    echo -e "${YELLOW}⚠ Bash completion not found${NC}"
fi

if [ -f /usr/share/zsh/site-functions/_dodot ]; then
    echo -e "${GREEN}✓ Zsh completion installed${NC}"
    ((COMPLETION_COUNT++))
else
    echo -e "${YELLOW}⚠ Zsh completion not found${NC}"
fi

if [ -f /usr/share/fish/vendor_completions.d/dodot.fish ]; then
    echo -e "${GREEN}✓ Fish completion installed${NC}"
    ((COMPLETION_COUNT++))
else
    echo -e "${YELLOW}⚠ Fish completion not found${NC}"
fi

# List installed files
echo -e "\n${YELLOW}9. Installed files:${NC}"
echo "All files:"
dpkg -L dodot
echo -e "\nChecking man directory:"
ls -la /usr/share/man/man1/ | grep dodot || echo "No dodot man page found"

# Summary
echo -e "\n${BLUE}=== Summary ===${NC}"
echo -e "${GREEN}✓ Package installed successfully${NC}"
echo -e "${GREEN}✓ Binary is functional${NC}"
echo -e "${GREEN}✓ ${COMPLETION_COUNT}/3 shell completions installed${NC}"

echo -e "\n${GREEN}DEB package verification completed successfully!${NC}"