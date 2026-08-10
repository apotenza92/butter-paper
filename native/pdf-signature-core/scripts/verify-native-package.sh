#!/bin/sh
set -eu

if [ "$#" -ne 2 ]; then
  echo "usage: verify-native-package.sh <package-directory> <complete-source-artifact>" >&2
  exit 64
fi

PACKAGE_DIR=$1
SOURCE_ARTIFACT=$2
if [ -z "${JAVA_HOME:-}" ] || [ ! -x "$JAVA_HOME/bin/java" ]; then
  echo "JAVA_HOME must point to the exact package Java 21 JDK" >&2
  exit 2
fi
if [ ! -f "$SOURCE_ARTIFACT" ]; then
  echo "complete-source artifact is missing" >&2
  exit 2
fi

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
REPOSITORY_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd -P)
MANIFEST_IDENTITY=$(node -e '
const fs = require("node:fs");
const manifest = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
if (!["darwin", "linux", "win32"].includes(manifest.platform)
  || !["arm64", "x64"].includes(manifest.arch)) process.exit(2);
process.stdout.write(`${manifest.platform}\n${manifest.arch}\n`);
' "$PACKAGE_DIR/manifest.json") || {
  echo "package manifest target identity is invalid" >&2
  exit 2
}
PLATFORM=$(printf '%s\n' "$MANIFEST_IDENTITY" | sed -n '1p')
ARCH=$(printf '%s\n' "$MANIFEST_IDENTITY" | sed -n '2p')

exec node "$REPOSITORY_ROOT/scripts/verify-pdf-signature-core-package.mjs" \
  --package-root "$PACKAGE_DIR" \
  --platform "$PLATFORM" \
  --arch "$ARCH" \
  --source-artifact "$SOURCE_ARTIFACT" \
  --verification-mode proof
