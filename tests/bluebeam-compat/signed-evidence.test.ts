import { mkdir, mkdtemp, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  assertSignedByteRangesUnchanged,
  createSignedEvidenceManifest,
  inspectSignedPdfBytes,
  scanSecretAndPathLeakage,
  validateSignedManifest,
} from '../../scripts/bluebeam-compat/signed-evidence.mjs';
import { createOracleResult, loadSignedInteropContract, validateOracleResult } from '../../scripts/bluebeam-compat/signed-oracle.mjs';

function makeSignedPdf({ cms = Buffer.from([0x30, 0x03, 0x02, 0x01, 0x01]), widget = false, contentsBeforeByteRange = false } = {}) {
  const cmsHex = cms.toString('hex').padEnd(32, '0');
  const placeholder = '0000000000 0000000000 0000000000 0000000000';
  if (contentsBeforeByteRange) {
    const prefix = `%PDF-1.7\n1 0 obj\n<< /Type /Sig /Contents <${cmsHex}> /ByteRange [`;
    const suffix = `] >>\n${widget ? '/Subtype /Widget /FT /Sig\n' : ''}endobj\n%%EOF\n`;
    const first = Buffer.byteLength('%PDF-1.7\n1 0 obj\n<< /Type /Sig /Contents <', 'latin1');
    const second = first + cmsHex.length;
    const total = Buffer.byteLength(`${prefix}${placeholder}${suffix}`, 'latin1');
    const ranges = `${String(0).padStart(10, '0')} ${String(first).padStart(10, '0')} ${String(second).padStart(10, '0')} ${String(total - second).padStart(10, '0')}`;
    return Buffer.from(`${prefix}${ranges}${suffix}`, 'latin1');
  }
  const beforeContents = Buffer.from(`%PDF-1.7\n1 0 obj\n<< /Type /Sig /ByteRange [${placeholder}] /Contents `, 'latin1');
  const contents = Buffer.from(`<${cmsHex}>`, 'latin1');
  const afterContents = Buffer.from(` >>\n${widget ? '/Subtype /Widget /FT /Sig\n' : ''}endobj\n%%EOF\n`, 'latin1');
  const first = beforeContents.length;
  const second = first + contents.length;
  const total = first + contents.length + afterContents.length;
  const ranges = `${String(0).padStart(10, '0')} ${String(first).padStart(10, '0')} ${String(second).padStart(10, '0')} ${String(total - second).padStart(10, '0')}`;
  const prefix = Buffer.from(`%PDF-1.7\n1 0 obj\n<< /Type /Sig /ByteRange [${ranges}] /Contents `, 'latin1');
  return Buffer.concat([prefix, contents, afterContents]);
}

function validManifest(oracle = createOracleResult('pyhanko', 'not-run', { reason: 'pyHanko is unavailable in the approved local environment.' })) {
  return createSignedEvidenceManifest({
    flowId: 'A',
    source: { file: 'artifacts/source.pdf', bytes: 10, sha256: 'a'.repeat(64) },
    output: { file: 'artifacts/signed.pdf', bytes: 12, sha256: 'b'.repeat(64) },
    structuralInspection: { schema: 'butter-paper/signed-pdf-structural-inspection', claimsCryptographicValidity: false },
    oracle,
  });
}

describe('signed evidence contract and manifest boundary', () => {
  it('validates deterministic A-H flows and pinned oracle states', async () => {
    const contract = await loadSignedInteropContract();
    expect(contract.flows.map((flow: { id: string }) => flow.id)).toEqual(['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H']);
    expect(contract.claimPolicy).toMatchObject({ structuralInspection: 'descriptive-only', signatureWidget: 'not-proof-of-signature' });
    expect(validateOracleResult(createOracleResult('pyhanko', 'unavailable', { reason: 'Executable is not installed.' }))).toMatchObject({ status: 'unavailable', cryptographicValidity: null });
    expect(validateOracleResult(createOracleResult('dss', 'not-run', { reason: 'Approved Java/DSS environment was not available.' }))).toMatchObject({ status: 'not-run', cryptographicValidity: null });
  });

  it('allows a signed manifest only when structure stays non-cryptographic and commercial absence is explicit', () => {
    const manifest = validManifest();
    expect(validateSignedManifest(manifest)).toMatchObject({ passed: true });
    expect(manifest.commercialEvidence).toMatchObject({ status: 'not-run' });
    expect(() => validateSignedManifest({ ...manifest, structuralInspection: { claimsCryptographicValidity: true } })).toThrow(/cryptographic validity/);
    expect(() => validateSignedManifest({ ...manifest, commercialEvidence: { status: 'passed' } })).toThrow(/commercial evidence/);
  });

  it('rejects secret-bearing values and absolute or traversing paths', () => {
    const findings = scanSecretAndPathLeakage({ password: 'do-not-record', key: '-----BEGIN PRIVATE KEY-----', path: '/tmp/private.pdf', nested: '../outside.json' });
    expect(findings.map((finding) => finding.type)).toEqual(expect.arrayContaining(['secret', 'path']));
    expect(() => validateSignedManifest({ ...validManifest(), output: { file: '/tmp/signed.pdf', sha256: 'b'.repeat(64) } })).toThrow(/relative/);
    expect(() => validateSignedManifest({ ...validManifest(), output: { file: '../signed.pdf', sha256: 'b'.repeat(64) } })).toThrow(/traversal/);
  });

  it('rejects symlinked evidence artifacts when verifying a manifest on disk', async () => {
    const root = await mkdtemp(join(tmpdir(), 'signed-evidence-'));
    await mkdir(join(root, 'artifacts'));
    const source = join(root, 'artifacts', 'source.pdf');
    const output = join(root, 'artifacts', 'signed.pdf');
    await writeFile(source, 'source');
    await writeFile(output, 'output');
    await symlink(source, join(root, 'artifacts', 'source-link.pdf'));
    const manifest = { ...validManifest(), source: { file: 'artifacts/source-link.pdf', bytes: 6, sha256: 'a'.repeat(64) } };
    await expect(import('../../scripts/bluebeam-compat/signed-evidence.mjs').then(({ verifyManifestFiles }) => verifyManifestFiles(manifest, join(root, 'manifest.json')))).rejects.toThrow(/symlink|does not exist/);
    expect(dirname(source)).toBe(dirname(output));
  });
});

describe('signature-aware structural inspection', () => {
  it('reports ByteRange, CMS container shape, revisions, widget presence, and DSS/VRI without crypto claims', () => {
    const inspected = inspectSignedPdfBytes(makeSignedPdf({ widget: true }));
    expect(inspected).toMatchObject({
      schema: 'butter-paper/signed-pdf-structural-inspection',
      signatureCount: 1,
      claimsCryptographicValidity: false,
      revisionAncestry: { eofCount: 1, complete: true },
      dssVri: { status: 'absent' },
    });
    expect(inspected.signatures[0]).toMatchObject({ byteRangeValid: true, cms: { present: true, der: true, structuralStatus: 'present-structural-only' } });
    expect(inspected.signatureWidgetCount).toBe(1);
  });

  it('finds CMS contents larger than the legacy inspection window', () => {
    const cms = Buffer.concat([Buffer.from([0x30, 0x82, 0x20, 0x00]), Buffer.alloc(8_192)]);
    const inspected = inspectSignedPdfBytes(makeSignedPdf({ cms }));
    expect(inspected.signatures[0].cms).toMatchObject({
      present: true,
      der: true,
      lengthValid: true,
      structuralStatus: 'present-structural-only',
    });
  });

  it('finds contents that precede ByteRange in a PDF signature dictionary', () => {
    const inspected = inspectSignedPdfBytes(makeSignedPdf({ contentsBeforeByteRange: true }));
    expect(inspected.signatures[0].cms).toMatchObject({
      present: true,
      der: true,
      structuralStatus: 'present-structural-only',
    });
  });

  it('rejects malformed ranges and corrupt CMS structurally', () => {
    const valid = makeSignedPdf();
    const malformed = Buffer.from(valid.toString('latin1').replace(/\/ByteRange \[[^\]]+\]/, '/ByteRange [0 1 2]'), 'latin1');
    const corruptCms = makeSignedPdf({ cms: Buffer.from('not-der', 'latin1') });
    expect(inspectSignedPdfBytes(malformed).signatures[0]).toMatchObject({ byteRangeValid: false, byteRangeError: expect.stringContaining('four') });
    expect(inspectSignedPdfBytes(corruptCms).signatures[0].cms).toMatchObject({ present: true, structuralStatus: 'malformed' });
  });

  it('detects changed bytes inside a signed range independently of widgets', () => {
    const before = inspectSignedPdfBytes(makeSignedPdf());
    const changedBytes = makeSignedPdf();
    changedBytes[10] ^= 1;
    const after = inspectSignedPdfBytes(changedBytes);
    expect(before.signatures[0].signedByteRangeSha256).not.toBe(after.signatures[0].signedByteRangeSha256);
    expect(() => assertSignedByteRangesUnchanged(before, after)).toThrow(/signed bytes changed/);
  });
});
