#!/bin/sh
set -eu

if [ "$#" -ne 3 ]; then
  echo 'usage: rebuild-from-package-source.sh <runtime-package-root> <complete-source-root> <empty-output-directory>' >&2
  exit 64
fi

PACKAGE_ROOT=$(CDPATH= cd -- "$1" && pwd)
COMPLETE_SOURCE_ROOT=$(CDPATH= cd -- "$2" && pwd)
OUTPUT_PARENT=$(CDPATH= cd -- "$(dirname -- "$3")" && pwd)
OUTPUT_ROOT="$OUTPUT_PARENT/$(basename -- "$3")"
if [ -e "$OUTPUT_ROOT" ]; then
  echo 'rebuild output must not already exist' >&2
  exit 2
fi
if [ -z "${JAVA_HOME:-}" ] || [ ! -x "$JAVA_HOME/bin/javac" ] || [ ! -x "$JAVA_HOME/bin/jar" ]; then
  echo 'rebuild requires JAVA_HOME pointing to a Java 21 JDK' >&2
  exit 2
fi

SOURCE_ROOT="$COMPLETE_SOURCE_ROOT/src/main/java"
if [ ! -f "$SOURCE_ROOT/com/butterpaper/signaturecore/Main.java" ]; then
  echo 'complete-source Butter Paper sidecar source tree is incomplete' >&2
  exit 2
fi
for REQUIRED_SOURCE_FILE in \
  "$COMPLETE_SOURCE_ROOT/SOURCE-MANIFEST.json" \
  "$COMPLETE_SOURCE_ROOT/generated/dependency-inventory.json" \
  "$COMPLETE_SOURCE_ROOT/generated/pdf-signature-core.cdx.json" \
  "$COMPLETE_SOURCE_ROOT/src/license/license-evidence.json" \
  "$COMPLETE_SOURCE_ROOT/source/upstream/dss-6.4-source.tar.gz"; do
  if [ ! -f "$REQUIRED_SOURCE_FILE" ]; then
    echo "complete-source relink kit is missing: $REQUIRED_SOURCE_FILE" >&2
    exit 2
  fi
done

APP_JARS=$(find "$PACKAGE_ROOT" -type f -path '*/app/pdf-signature-core.jar' -print)
APP_JAR_COUNT=$(printf '%s\n' "$APP_JARS" | awk 'NF { count += 1 } END { print count + 0 }')
LIBRARY_DIRECTORIES=$(find "$PACKAGE_ROOT" -type d -path '*/app/lib' -print)
LIBRARY_COUNT=$(printf '%s\n' "$LIBRARY_DIRECTORIES" | awk 'NF { count += 1 } END { print count + 0 }')
if [ "$APP_JAR_COUNT" -ne 1 ] || [ "$LIBRARY_COUNT" -ne 1 ]; then
  echo "expected one packaged application jar and library directory; found $APP_JAR_COUNT and $LIBRARY_COUNT" >&2
  exit 2
fi
APP_JAR=$APP_JARS
LIBRARY_ROOT=$LIBRARY_DIRECTORIES
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    JAVA_APP_JAR=$(cygpath -w "$APP_JAR")
    JAVA_LIBRARY_ROOT=$(cygpath -w "$LIBRARY_ROOT")
    PACKAGE_CLASSPATH="$JAVA_APP_JAR;$JAVA_LIBRARY_ROOT/*"
    ;;
  *)
    JAVA_APP_JAR=$APP_JAR
    JAVA_LIBRARY_ROOT=$LIBRARY_ROOT
    PACKAGE_CLASSPATH="$JAVA_APP_JAR:$JAVA_LIBRARY_ROOT/*"
    ;;
esac

mkdir -p "$OUTPUT_ROOT/classes"
"$JAVA_HOME/bin/java" -cp "$PACKAGE_CLASSPATH" \
  com.butterpaper.signaturecore.LicenseEvidenceVerifier \
  "$COMPLETE_SOURCE_ROOT/generated/pdf-signature-core.cdx.json" \
  "$COMPLETE_SOURCE_ROOT/src/license/license-evidence.json" \
  "$LIBRARY_ROOT" \
  "$COMPLETE_SOURCE_ROOT/notices/dependencies/licenses" \
  "$COMPLETE_SOURCE_ROOT/source/upstream" \
  "$COMPLETE_SOURCE_ROOT/notices/dependencies/jar-notices" \
  "$OUTPUT_ROOT/reconciled-dependency-inventory.json"
if ! cmp -s \
  "$OUTPUT_ROOT/reconciled-dependency-inventory.json" \
  "$COMPLETE_SOURCE_ROOT/generated/dependency-inventory.json"; then
  echo 'complete-source dependency inventory does not reconcile against the packaged runtime libraries' >&2
  exit 2
fi
(
  cd "$SOURCE_ROOT"
  find . -type f -name '*.java' -print | LC_ALL=C sort > "$OUTPUT_ROOT/sources.list"
  [ -s "$OUTPUT_ROOT/sources.list" ]
  "$JAVA_HOME/bin/javac" --release 21 -encoding UTF-8 \
    -cp "$JAVA_LIBRARY_ROOT/*" -d "$OUTPUT_ROOT/classes" @"$OUTPUT_ROOT/sources.list"
)
"$JAVA_HOME/bin/jar" --create --file "$OUTPUT_ROOT/pdf-signature-core-rebuilt.jar" \
  -C "$OUTPUT_ROOT/classes" .
"$JAVA_HOME/bin/jar" --list --file "$OUTPUT_ROOT/pdf-signature-core-rebuilt.jar" |
  grep -Fx 'com/butterpaper/signaturecore/Main.class' >/dev/null

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    REBUILT_JAR_FOR_JAVA=$(cygpath -w "$OUTPUT_ROOT/pdf-signature-core-rebuilt.jar")
    REBUILT_CLASSPATH="$REBUILT_JAR_FOR_JAVA;$JAVA_LIBRARY_ROOT/*"
    ;;
  *)
    REBUILT_CLASSPATH="$OUTPUT_ROOT/pdf-signature-core-rebuilt.jar:$JAVA_LIBRARY_ROOT/*"
    ;;
esac
REBUILT_HANDSHAKE=$(printf '%s\n' \
  '{"protocolVersion":1,"requestId":"packaged-source-rebuild-smoke","operation":"handshake","payload":{}}' |
  "$JAVA_HOME/bin/java" -cp "$REBUILT_CLASSPATH" \
    com.butterpaper.signaturecore.Main)
printf '%s\n' "$REBUILT_HANDSHAKE" | grep -F '"requestId":"packaged-source-rebuild-smoke"' >/dev/null
printf '%s\n' "$REBUILT_HANDSHAKE" | grep -F '"event":"result"' >/dev/null
printf '%s\n' "$REBUILT_HANDSHAKE" | grep -F '"certificateSign":false' >/dev/null
printf '%s\n' "$REBUILT_HANDSHAKE" | grep -F '"inspect":true' >/dev/null

echo "separate complete-source artifact rebuilt, reconciled, and executed successfully: $OUTPUT_ROOT/pdf-signature-core-rebuilt.jar"
