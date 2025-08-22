#!/bin/bash
set -euo pipefail

echo "Renaming off -> unlink in Go files..."

# 1. Function and type names
find . -name "*.go" -type f | xargs sed -i '' \
  -e 's/OffPacks/UnlinkPacks/g' \
  -e 's/OffPacksOptions/UnlinkPacksOptions/g' \
  -e 's/OffResult/UnlinkResult/g'

# 2. Command registration and messages
find . -name "*.go" -type f | xargs sed -i '' \
  -e 's/newOffCmd/newUnlinkCmd/g' \
  -e 's/MsgOffShort/MsgUnlinkShort/g' \
  -e 's/MsgOffLong/MsgUnlinkLong/g' \
  -e 's/MsgOffExample/MsgUnlinkExample/g' \
  -e 's/MsgErrOffPacks/MsgErrUnlinkPacks/g'

# 3. Command strings in code
find . -name "*.go" -type f | xargs sed -i '' \
  -e 's/"off"/"unlink"/g' \
  -e "s/'off'/'unlink'/g"

# 4. Package path for off command
mv pkg/commands/off pkg/commands/unlink 2>/dev/null || true

# 5. Update imports
find . -name "*.go" -type f | xargs sed -i '' \
  -e 's|"github.com/arthur-debert/dodot/pkg/commands/off"|"github.com/arthur-debert/dodot/pkg/commands/unlink"|g'

# 6. Message files
if [ -f "cmd/dodot/msgs/off-long.txt" ]; then
  mv cmd/dodot/msgs/off-long.txt cmd/dodot/msgs/unlink-long.txt
fi

if [ -f "cmd/dodot/msgs/off-example.txt" ]; then
  mv cmd/dodot/msgs/off-example.txt cmd/dodot/msgs/unlink-example.txt
fi

echo "Done!"