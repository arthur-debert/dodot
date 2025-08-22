#!/bin/bash
set -euo pipefail

echo "Renaming deploy -> link in Go files..."

# 1. Function and type names
find . -name "*.go" -type f | xargs sed -i '' \
  -e 's/DeployPacks/LinkPacks/g' \
  -e 's/DeployPacksOptions/LinkPacksOptions/g' \
  -e 's/DeployPacksDirect/LinkPacksDirect/g'

# 2. Command registration and messages
find . -name "*.go" -type f | xargs sed -i '' \
  -e 's/newDeployCmd/newLinkCmd/g' \
  -e 's/MsgDeployShort/MsgLinkShort/g' \
  -e 's/MsgDeployLong/MsgLinkLong/g' \
  -e 's/MsgDeployExample/MsgLinkExample/g' \
  -e 's/MsgErrDeployPacks/MsgErrLinkPacks/g'

# 3. Command strings in code
find . -name "*.go" -type f | xargs sed -i '' \
  -e 's/"deploy"/"link"/g' \
  -e "s/'deploy'/'link'/g"

# 4. Package path for deploy command
mv pkg/commands/deploy pkg/commands/link 2>/dev/null || true

# 5. Update imports
find . -name "*.go" -type f | xargs sed -i '' \
  -e 's|"github.com/arthur-debert/dodot/pkg/commands/deploy"|"github.com/arthur-debert/dodot/pkg/commands/link"|g'

# 6. Message files
if [ -d "cmd/dodot/msgs/deploy" ]; then
  mv cmd/dodot/msgs/deploy cmd/dodot/msgs/link
fi

# 7. Test directories
if [ -d "tests/deploy" ]; then
  mv tests/deploy tests/link
fi

echo "Done!"