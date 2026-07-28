const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');

function extractReleaseNotes(changelog, version) {
  const escapedVersion = version.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const heading = new RegExp(`^## \\[${escapedVersion}\\]\\s*$`, 'm');
  const match = heading.exec(changelog);
  if (match == null) {
    throw new Error(`CHANGELOG.md does not contain a [${version}] section.`);
  }

  const sectionStart = match.index + match[0].length;
  const remaining = changelog.slice(sectionStart);
  const nextHeading = /^## /m.exec(remaining);
  const notes = remaining.slice(0, nextHeading?.index ?? remaining.length).trim();
  if (notes === '') {
    throw new Error(`CHANGELOG.md section [${version}] is empty.`);
  }
  return notes;
}

if (require.main === module) {
  const version = process.argv[2];
  if (!version) {
    throw new Error('Usage: node scripts/release-notes.cjs <version> [changelog-path]');
  }
  const changelogPath = resolve(process.argv[3] ?? 'CHANGELOG.md');
  process.stdout.write(`${extractReleaseNotes(readFileSync(changelogPath, 'utf8'), version)}\n`);
}

module.exports = { extractReleaseNotes };
