#!/bin/sh
set -eu

if [ "$#" -ne 2 ]; then
  echo "usage: build-native-package.sh <darwin|win32|linux> <arm64|x64>" >&2
  exit 64
fi

PLATFORM=$1
ARCH=$2
BASE_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

if [ -z "${JAVA_HOME:-}" ] || [ ! -x "$JAVA_HOME/bin/jpackage" ]; then
  echo "JAVA_HOME must point to the Microsoft OpenJDK 21.0.12 JDK" >&2
  exit 2
fi
JAVA_SETTINGS=$($JAVA_HOME/bin/java -XshowSettings:properties -version 2>&1)
JAVA_VERSION=$(printf '%s\n' "$JAVA_SETTINGS" | sed -n 's/^[[:space:]]*java.version = //p')
if [ "$JAVA_VERSION" != "21.0.12" ]; then
  echo "native packages require Java 21.0.12; found $JAVA_VERSION" >&2
  exit 2
fi

JAVA_OS_NAME=$(printf '%s\n' "$JAVA_SETTINGS" | sed -n 's/^[[:space:]]*os.name = //p')
JAVA_OS_ARCH=$(printf '%s\n' "$JAVA_SETTINGS" | sed -n 's/^[[:space:]]*os.arch = //p')
case "$JAVA_OS_NAME" in
  'Mac OS X') HOST_PLATFORM=darwin ;;
  Linux) HOST_PLATFORM=linux ;;
  Windows*) HOST_PLATFORM=win32 ;;
  *) echo "unsupported Java host platform: $JAVA_OS_NAME" >&2; exit 2 ;;
esac
case "$JAVA_OS_ARCH" in
  arm64|aarch64) HOST_ARCH=arm64 ;;
  x86_64|amd64) HOST_ARCH=x64 ;;
  *) echo "unsupported Java build architecture: $JAVA_OS_ARCH" >&2; exit 2 ;;
esac
if [ "$PLATFORM" != "$HOST_PLATFORM" ] || [ "$ARCH" != "$HOST_ARCH" ]; then
  echo "jpackage is native-only: requested $PLATFORM-$ARCH on $HOST_PLATFORM-$HOST_ARCH" >&2
  exit 2
fi

cd "$BASE_DIR"
./mvnw clean package
node "$BASE_DIR/scripts/canonicalize-generated-source-inputs.mjs" \
  "$BASE_DIR/target/pdf-signature-core.cdx.json"

LICENSE_EVIDENCE_DIR="$BASE_DIR/target/license-evidence"
"$BASE_DIR/scripts/fetch-license-evidence.sh" "$LICENSE_EVIDENCE_DIR"
"$JAVA_HOME/bin/java" -cp "$BASE_DIR/target/pdf-signature-core.jar" \
  com.butterpaper.signaturecore.LicenseEvidenceVerifier \
  "$BASE_DIR/target/pdf-signature-core.cdx.json" \
  "$BASE_DIR/src/license/license-evidence.json" \
  "$BASE_DIR/target/lib" \
  "$LICENSE_EVIDENCE_DIR/licenses" \
  "$LICENSE_EVIDENCE_DIR/sources" \
  "$LICENSE_EVIDENCE_DIR/jar-notices" \
  "$LICENSE_EVIDENCE_DIR/inventory.json"

INPUT_DIR="$BASE_DIR/build/jpackage-input/$PLATFORM-$ARCH"
OUTPUT_DIR="$BASE_DIR/build/package/$PLATFORM-$ARCH"
STAGING_DIR="$BASE_DIR/build/package-staging/$PLATFORM-$ARCH"
RUNTIME_DIR="$BASE_DIR/build/runtime/$PLATFORM-$ARCH"
RELINK_SMOKE_DIR="$BASE_DIR/build/relink-smoke/$PLATFORM-$ARCH"
SOURCE_ARTIFACT_DIR="$BASE_DIR/build/source-artifact/$PLATFORM-$ARCH"
SOURCE_EXTRACT_DIR="$BASE_DIR/build/source-extract/$PLATFORM-$ARCH"
for CLEAN_DIR in \
  "$INPUT_DIR" "$OUTPUT_DIR" "$STAGING_DIR" "$RUNTIME_DIR" "$RELINK_SMOKE_DIR" \
  "$SOURCE_ARTIFACT_DIR" "$SOURCE_EXTRACT_DIR"; do
  case "$CLEAN_DIR" in
    "$BASE_DIR/build/"*) ;;
    *) echo "refusing to clean a path outside the sidecar build directory" >&2; exit 2 ;;
  esac
  if [ -d "$CLEAN_DIR" ]; then
    find "$CLEAN_DIR" -depth -mindepth 1 -delete
  fi
done
for FRESH_DIR in "$RELINK_SMOKE_DIR" "$SOURCE_EXTRACT_DIR"; do
  if [ -d "$FRESH_DIR" ]; then
    rmdir "$FRESH_DIR"
  fi
done
if [ -d "$RUNTIME_DIR" ]; then
  rmdir "$RUNTIME_DIR"
fi
mkdir -p \
  "$INPUT_DIR/lib" "$OUTPUT_DIR" "$STAGING_DIR" "$SOURCE_ARTIFACT_DIR" \
  "$(dirname -- "$RELINK_SMOKE_DIR")" \
  "$(dirname -- "$SOURCE_EXTRACT_DIR")"
cp "$BASE_DIR/target/pdf-signature-core.jar" "$INPUT_DIR/pdf-signature-core.jar"
cp "$BASE_DIR"/target/lib/*.jar "$INPUT_DIR/lib/"

"$JAVA_HOME/bin/jlink" \
  --add-modules java.base,java.desktop,java.naming,java.sql,jdk.crypto.ec,jdk.unsupported \
  --no-header-files \
  --no-man-pages \
  --strip-debug \
  --output "$RUNTIME_DIR"

"$JAVA_HOME/bin/jpackage" \
  --type app-image \
  --name pdf-signature-core \
  --app-version 1.0.0 \
  --input "$INPUT_DIR" \
  --dest "$STAGING_DIR" \
  --main-jar pdf-signature-core.jar \
  --main-class com.butterpaper.signaturecore.Main \
  --runtime-image "$RUNTIME_DIR" \
  --java-options -Dfile.encoding=UTF-8 \
  --java-options -Duser.language=en \
  --java-options -Duser.country=AU \
  --java-options -Duser.timezone=UTC

case "$PLATFORM" in
  darwin)
    APP_IMAGE="$STAGING_DIR/pdf-signature-core.app"
    cp -RL "$APP_IMAGE" "$OUTPUT_DIR/"
    LAUNCHER="pdf-signature-core.app/Contents/MacOS/pdf-signature-core"
    ;;
  win32)
    APP_IMAGE="$STAGING_DIR/pdf-signature-core"
    cp -RL "$APP_IMAGE" "$OUTPUT_DIR/"
    LAUNCHER="pdf-signature-core/pdf-signature-core.exe"
    ;;
  linux)
    APP_IMAGE="$STAGING_DIR/pdf-signature-core"
    cp -RL "$APP_IMAGE" "$OUTPUT_DIR/"
    LAUNCHER="pdf-signature-core/bin/pdf-signature-core"
    ;;
  *) echo "unsupported platform" >&2; exit 2 ;;
esac

mkdir -p "$OUTPUT_DIR/notices/dependencies" "$OUTPUT_DIR/sbom"
cp "$BASE_DIR"/notices/* "$OUTPUT_DIR/notices/"
cp -R "$LICENSE_EVIDENCE_DIR/licenses" "$OUTPUT_DIR/notices/dependencies/"
cp -R "$LICENSE_EVIDENCE_DIR/jar-notices" "$OUTPUT_DIR/notices/dependencies/"
cp "$BASE_DIR/target/pdf-signature-core.cdx.json" "$OUTPUT_DIR/sbom/pdf-signature-core.cdx.json"
cp "$BASE_DIR/src/license/runtime-cve-scan-input.json" "$OUTPUT_DIR/sbom/runtime-cve-scan-input.json"
cp "$JAVA_HOME/legal/java.base/LICENSE" "$OUTPUT_DIR/notices/MICROSOFT-OPENJDK-LICENSE.txt"
if [ -f "$JAVA_HOME/legal/java.base/ADDITIONAL_LICENSE_INFO" ]; then
  cp "$JAVA_HOME/legal/java.base/ADDITIONAL_LICENSE_INFO" "$OUTPUT_DIR/notices/MICROSOFT-OPENJDK-ADDITIONAL-LICENSE-INFO.txt"
fi
if [ -f "$JAVA_HOME/legal/java.base/ASSEMBLY_EXCEPTION" ]; then
  cp "$JAVA_HOME/legal/java.base/ASSEMBLY_EXCEPTION" "$OUTPUT_DIR/notices/MICROSOFT-OPENJDK-ASSEMBLY-EXCEPTION.txt"
fi

SOURCE_DESCRIPTOR="$OUTPUT_DIR/complete-source-artifact.json"
node "$BASE_DIR/scripts/complete-source-artifact.mjs" create \
  --base-directory "$BASE_DIR" \
  --license-evidence-directory "$LICENSE_EVIDENCE_DIR" \
  --output-directory "$SOURCE_ARTIFACT_DIR" \
  --descriptor "$SOURCE_DESCRIPTOR"
SOURCE_ARTIFACT_NAME=$(node -e \
  'const fs=require("node:fs"); const value=JSON.parse(fs.readFileSync(process.argv[1], "utf8")); process.stdout.write(value.delivery.canonicalFileName);' \
  "$SOURCE_DESCRIPTOR")
SOURCE_ARTIFACT="$SOURCE_ARTIFACT_DIR/$SOURCE_ARTIFACT_NAME"
node "$BASE_DIR/scripts/complete-source-artifact.mjs" verify \
  --artifact "$SOURCE_ARTIFACT" \
  --descriptor "$SOURCE_DESCRIPTOR" \
  --extraction-root "$SOURCE_EXTRACT_DIR"
COMPLETE_SOURCE_ROOT="$SOURCE_EXTRACT_DIR/butter-paper-pdf-signature-core-complete-source-v1-0.1.0"
"$COMPLETE_SOURCE_ROOT/scripts/rebuild-from-package-source.sh" \
  "$OUTPUT_DIR" "$COMPLETE_SOURCE_ROOT" "$RELINK_SMOKE_DIR"
find "$RELINK_SMOKE_DIR" -depth -delete
find "$SOURCE_EXTRACT_DIR" -depth -delete

"$JAVA_HOME/bin/java" -cp "$BASE_DIR/target/pdf-signature-core.jar" \
  com.butterpaper.signaturecore.PackageManifestWriter manifest "$OUTPUT_DIR" "$PLATFORM" "$ARCH" "$LAUNCHER"
"$JAVA_HOME/bin/java" -cp "$BASE_DIR/target/pdf-signature-core.jar" \
  com.butterpaper.signaturecore.PackageManifestWriter post-sign "$OUTPUT_DIR"

echo "native runtime package and separate complete-source artifact built: $OUTPUT_DIR $SOURCE_ARTIFACT"
