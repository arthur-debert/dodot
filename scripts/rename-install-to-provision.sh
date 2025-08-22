#!/bin/bash
set -euo pipefail

echo "Renaming install -> provision in Go files..."

# 1. Function and type names
find . -name "*.go" -type f | xargs sed -i '' \
  -e 's/InstallPacks/ProvisionPacks/g' \
  -e 's/InstallPacksOptions/ProvisionPacksOptions/g' \
  -e 's/InstallPacksDirect/ProvisionPacksDirect/g'

# 2. Command registration and messages
find . -name "*.go" -type f | xargs sed -i '' \
  -e 's/newInstallCmd/newProvisionCmd/g' \
  -e 's/MsgInstallShort/MsgProvisionShort/g' \
  -e 's/MsgInstallLong/MsgProvisionLong/g' \
  -e 's/MsgInstallExample/MsgProvisionExample/g' \
  -e 's/MsgErrInstallPacks/MsgErrProvisionPacks/g'

# 3. Command strings in code
find . -name "*.go" -type f | xargs sed -i '' \
  -e 's/"install"/"provision"/g' \
  -e "s/'install'/'provision'/g"

# 4. Package path for install command
mv pkg/commands/install pkg/commands/provision 2>/dev/null || true

# 5. Update imports
find . -name "*.go" -type f | xargs sed -i '' \
  -e 's|"github.com/arthur-debert/dodot/pkg/commands/install"|"github.com/arthur-debert/dodot/pkg/commands/provision"|g'

# 6. Message files
if [ -f "cmd/dodot/msgs/install-long.txt" ]; then
  mv cmd/dodot/msgs/install-long.txt cmd/dodot/msgs/provision-long.txt
fi

if [ -f "cmd/dodot/msgs/install-example.txt" ]; then
  mv cmd/dodot/msgs/install-example.txt cmd/dodot/msgs/provision-example.txt
fi

echo "Done!"