#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: fetch-license-evidence.sh <evidence-root>" >&2
  exit 64
fi

EVIDENCE_ROOT=$1
LICENSE_DIR="$EVIDENCE_ROOT/licenses"
SOURCE_DIR="$EVIDENCE_ROOT/sources"
mkdir -p "$LICENSE_DIR" "$SOURCE_DIR"

ACTIVE_PART=''
cleanup_partial() {
  if [ -n "$ACTIVE_PART" ] && [ -f "$ACTIVE_PART" ]; then
    part_directory=$(dirname "$ACTIVE_PART")
    part_name=$(basename "$ACTIVE_PART")
    find "$part_directory" -maxdepth 1 -type f -name "$part_name" -delete
  fi
}
trap cleanup_partial EXIT
trap 'exit 130' HUP INT TERM

fetch_verified() {
  output_directory=$1
  file=$2
  expected=$3
  expected_bytes=$4
  shift 4
  if [ "$#" -eq 0 ]; then
    echo "no licence evidence source configured for $file" >&2
    exit 3
  fi
  destination="$output_directory/$file"
  ACTIVE_PART="$destination.part"
  for url in "$@"; do
    find "$output_directory" -maxdepth 1 -type f -name "$file.part" -delete
    if curl --fail --location --proto '=https' --tlsv1.2 \
      --connect-timeout 20 --max-time 900 --max-filesize "$expected_bytes" \
      --retry 3 --retry-delay 1 --output "$ACTIVE_PART" "$url"; then
      actual_bytes=$(wc -c < "$ACTIVE_PART" | tr -d '[:space:]')
      if command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$ACTIVE_PART" | awk '{print $1}')
      else
        actual=$(sha256sum "$ACTIVE_PART" | awk '{print $1}')
      fi
      if [ "$actual_bytes" = "$expected_bytes" ] && [ "$actual" = "$expected" ]; then
        mv "$ACTIVE_PART" "$destination"
        ACTIVE_PART=''
        return 0
      fi
    fi
    find "$output_directory" -maxdepth 1 -type f -name "$file.part" -delete
  done
  echo "no verified licence evidence source succeeded for $file" >&2
  exit 3
}

fetch_verified "$LICENSE_DIR" Apache-2.0.txt cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30 11358 https://www.apache.org/licenses/LICENSE-2.0.txt
fetch_verified "$LICENSE_DIR" Bouncy-Castle-Licence.html edbbb10380b1271998b867a2e36b1cbee226e03d438726e1a91f80c5dde11849 1175 https://raw.githubusercontent.com/bcgit/bc-java/r1rv83/LICENSE.html
fetch_verified "$LICENSE_DIR" BSD-3-Clause.txt 5a93d5831e1297ab10fe643e1a631e83be392896da14ee2951285a79012df69d 1460 https://raw.githubusercontent.com/spdx/license-list-data/v3.27.0/text/BSD-3-Clause.txt
fetch_verified "$LICENSE_DIR" LGPL-2.1.txt 20e50fe7aae3e56378ebf0417d9de904f55a0e61e4df315333e632a4d3555d95 26419 \
  https://ftp.gnu.org/gnu/Licenses/lgpl-2.1.txt \
  'https://archive.softwareheritage.org/browse/content/sha1_git:f6683e74e0f0130f246dbf7054f4cacc88ca84b0/raw/?filename=LGPL-2.1.txt' \
  https://www.gnu.org/licenses/old-licenses/lgpl-2.1.txt
fetch_verified "$LICENSE_DIR" MIT-SLF4J.txt 6fbe2eaf44b193b8a40eed9208f52848572224ad8d7672dd09418aa174847e73 1154 https://raw.githubusercontent.com/qos-ch/slf4j/v_2.0.17/LICENSE.txt
fetch_verified "$SOURCE_DIR" dss-6.4-source.tar.gz 5f2421d6bf1c6073aa1e3c1ed4b44d2f058c6d751a4d89dbf326082860b224a4 137227450 https://github.com/esig/dss/archive/refs/tags/6.4.tar.gz

DSS_SOURCE_ARCHIVE="$SOURCE_DIR/dss-6.4-source.tar.gz"
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) DSS_SOURCE_ARCHIVE=$(cygpath -u "$DSS_SOURCE_ARCHIVE") ;;
esac
if ! tar -tzf "$DSS_SOURCE_ARCHIVE" | awk '
  index($0, "dss-6.4/") != 1 { exit 2 }
  /(^|\/)\.\.?(\/|$)/ { exit 2 }
  { count += 1 }
  END { if (count != 10213) exit 2 }
'; then
  echo 'DSS corresponding-source archive structure is invalid' >&2
  exit 3
fi
