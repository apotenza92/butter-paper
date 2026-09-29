#!/usr/bin/env node

import { createHash } from "node:crypto";
import { lstat, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { validateNonMacProductionManifest } from "./stage-nonmac-pdfium-production.mjs";
import { validateProductionManifest } from "./stage-pdfium-production.mjs";

const scriptPath = fileURLToPath(import.meta.url);
const requiredTargets = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "aarch64-pc-windows-msvc",
  "x86_64-pc-windows-msvc",
  "aarch64-unknown-linux-gnu",
  "x86_64-unknown-linux-gnu",
];
const targetSet = new Set(requiredTargets);
const placeholder = /\b(?:unreviewed|placeholder|incomplete|todo|tbd|example|sample)\b/i;

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const fail = (message) => { throw new Error(message); };

function safeRelative(value, label) {
  if (typeof value !== "string" || !value || isAbsolute(value) || /^[A-Za-z]:[\\/]/.test(value) || value.includes("\\") || value.split("/").some((part) => !part || part === "." || part === "..")) {
    fail(`${label} must be a safe relative path`);
  }
  return value;
}

function verifyRecordShape(record, label) {
  if (!record || typeof record !== "object" || Array.isArray(record)) fail(`${label} must be an object`);
  safeRelative(record.path, `${label}.path`);
  if (!Number.isSafeInteger(record.bytes) || record.bytes <= 0) fail(`${label}.bytes must be a positive safe integer`);
  if (!/^[0-9a-f]{64}$/.test(record.sha256 ?? "")) fail(`${label}.sha256 must be a lowercase SHA-256 digest`);
}

async function safeFile(root, path, label) {
  safeRelative(path, `${label}.path`);
  let current = root;
  for (const part of path.split("/")) {
    current = join(current, part);
    const metadata = await lstat(current);
    if (metadata.isSymbolicLink()) fail(`${label} must not traverse a symlink`);
  }
  const metadata = await lstat(current);
  if (!metadata.isFile() || metadata.nlink !== 1) fail(`${label} must be a regular single-link file`);
  return readFile(current);
}

async function inventory(root, prefix = "") {
  const files = [];
  for (const entry of await readdir(prefix ? join(root, prefix) : root, { withFileTypes: true })) {
    const path = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isSymbolicLink()) fail(`input artifact contains symlink ${path}`);
    if (entry.isDirectory()) files.push(...await inventory(root, path));
    else if (entry.isFile()) {
      const metadata = await lstat(join(root, path));
      if (metadata.nlink !== 1) fail(`input artifact contains hard link ${path}`);
      files.push(path);
    } else fail(`input artifact contains special file ${path}`);
  }
  return files.sort();
}

function recordBytes(record, bytes, label) {
  verifyRecordShape(record, label);
  if (bytes.length !== record.bytes || sha256(bytes) !== record.sha256) fail(`${label} does not match its approved byte receipt`);
}

function validateHumanReview(review, reviewType, candidate, artifact, candidateDigest) {
  if (!review || typeof review !== "object" || Array.isArray(review) || review.schemaVersion !== 1 || review.reviewType !== reviewType || review.decision !== "approved") fail(`${reviewType} evidence is not an explicit schemaVersion 1 approved review`);
  if (typeof review.summary !== "string" || !review.summary.trim() || placeholder.test(review.summary)) fail(`${reviewType} summary is missing or a placeholder`);
  if (!Array.isArray(review.evidenceReferences) || review.evidenceReferences.length === 0 || review.evidenceReferences.some((item) => typeof item !== "string" || !item.trim() || placeholder.test(item))) fail(`${reviewType} evidence references are missing or placeholders`);
  if (!Array.isArray(review.unresolvedIssues) || review.unresolvedIssues.length !== 0) fail(`${reviewType} unresolvedIssues must be an empty array`);
  const assertions = reviewType === "redistribution-legal"
    ? ["sourceRedistributionRightsConfirmed", "dependencyRedistributionRightsConfirmed", "noticeInventoryComplete", "shippingObligationsRecorded"]
    : ["sourceIdentityConfirmed", "dependencyOriginsConfirmed", "sbomComponentsResolved", "noticeOriginsConfirmed"];
  if (!review.assertions || assertions.some((key) => review.assertions[key] !== true)) fail(`${reviewType} review assertions are incomplete`);
  const reviewer = review.reviewer;
  if (!reviewer || typeof reviewer.identity !== "string" || !reviewer.identity.trim() || placeholder.test(reviewer.identity) || typeof reviewer.reference !== "string" || !reviewer.reference.trim() || placeholder.test(reviewer.reference) || typeof reviewer.timestamp !== "string" || !Number.isFinite(Date.parse(reviewer.timestamp)) || new Date(reviewer.timestamp).toISOString() !== reviewer.timestamp) fail(`${reviewType} reviewer identity, reference, and UTC timestamp are required`);
  if (typeof review.submitter !== "string" || !review.submitter.trim() || placeholder.test(review.submitter) || reviewer.identity.toLowerCase() === review.submitter.toLowerCase()) fail(`${reviewType} review must be performed by someone other than its submitter`);
  const binding = review.binding;
  if (binding?.candidateManifestSha256 !== candidateDigest || binding?.target !== artifact.target || binding?.librarySha256 !== artifact.library.sha256 || binding?.sbomSha256 !== artifact.sbom.sha256 || binding?.provenanceSha256 !== artifact.provenance.sha256 || binding?.gnArgsSha256 !== artifact.gnArgs.sha256) fail(`${reviewType} evidence is stale or bound to different candidate inputs`);
  const noticeBindings = artifact.notices.map(({ path, bytes, sha256: digest }) => ({ path, bytes, sha256: digest })).sort((a, b) => a.path.localeCompare(b.path));
  if (binding.noticesSha256 !== sha256(Buffer.from(JSON.stringify(noticeBindings)))) fail(`${reviewType} evidence notice binding does not match candidate`);
  const identities = {
    pdfiumRevision: candidate.source.revision,
    wrapperPackage: candidate.wrapper.package,
    wrapperVersion: candidate.wrapper.version,
    wrapperRevision: candidate.wrapper.revision,
    wrapperFeature: candidate.wrapper.feature,
    patchSha256: candidate.build.sharedLibraryPatchSha256,
    dependencyPolicyPatchSha256: candidate.build.dependencyPolicyPatchSha256,
  };
  for (const [key, value] of Object.entries(identities)) if (binding.identities?.[key] !== value) fail(`${reviewType} pinned identity ${key} does not match candidate`);
}

function sameJson(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

async function readInput(input, expectedTarget) {
  if (!input || typeof input !== "object" || Array.isArray(input) || input.target !== expectedTarget) fail(`approval input target must be ${expectedTarget}`);
  const root = resolve(input.artifactRoot);
  const rootStat = await lstat(root);
  if (!rootStat.isDirectory() || rootStat.isSymbolicLink()) fail(`${expectedTarget} artifact root must be a real directory`);
  const approvedPath = resolve(input.approvedManifestPath);
  const approvedStat = await lstat(approvedPath);
  if (!approvedStat.isFile() || approvedStat.isSymbolicLink() || approvedStat.nlink !== 1) fail(`${expectedTarget} approved manifest must be a regular single-link file`);
  const approvedManifestBytes = await readFile(approvedPath);
  const approvedManifest = JSON.parse(approvedManifestBytes.toString("utf8"));
  const validate = expectedTarget.endsWith("apple-darwin") ? validateProductionManifest : validateNonMacProductionManifest;
  validate(approvedManifest);
  if (approvedManifest.artifacts.length !== 1 || approvedManifest.artifacts[0].target !== expectedTarget) fail(`${expectedTarget} approval must contain exactly its own target`);
  const artifact = approvedManifest.artifacts[0];

  const candidatePath = resolve(input.candidateManifestPath ?? join(root, "production-pdfium-candidate.json"));
  const candidateRelative = relative(root, candidatePath);
  if (!candidateRelative || candidateRelative.startsWith("..") || isAbsolute(candidateRelative)) fail(`${expectedTarget} candidate manifest must be inside its artifact root`);
  const candidateBytes = await safeFile(root, candidateRelative, `${expectedTarget} candidate manifest`);
  const candidate = JSON.parse(candidateBytes.toString("utf8"));
  if (candidate.productionApproved !== false || candidate.artifacts?.length !== 1 || candidate.artifacts[0].target !== expectedTarget) fail(`${expectedTarget} must retain its single-target unapproved source candidate`);
  const expectedApproved = { ...candidate, productionApproved: true, redistributionReview: approvedManifest.redistributionReview, supplierReview: approvedManifest.supplierReview };
  if (!sameJson(expectedApproved, approvedManifest)) fail(`${expectedTarget} approved manifest does not exactly promote its retained candidate`);
  const originalArtifact = candidate.artifacts[0];
  if (!sameJson({ ...originalArtifact, library: artifact.library, sbom: artifact.sbom, provenance: artifact.provenance, gnArgs: artifact.gnArgs, noticeRoot: artifact.noticeRoot, notices: artifact.notices }, artifact)) fail(`${expectedTarget} approved artifact differs from the candidate artifact`);

  const paths = new Set([candidateRelative]);
  const loaded = new Map();
  for (const name of ["redistributionReview", "supplierReview"]) {
    const record = candidate[name];
    const bytes = await safeFile(root, record.path, `${expectedTarget} original ${name} placeholder`);
    recordBytes(record, bytes, `${expectedTarget} original ${name} placeholder`);
    if (!placeholder.test(bytes.toString("utf8"))) fail(`${expectedTarget} original ${name} must remain the candidate workflow placeholder`);
    paths.add(record.path);
    loaded.set(record.path, bytes);
  }
  for (const [name, record] of Object.entries({ library: artifact.library, sbom: artifact.sbom, provenance: artifact.provenance, gnArgs: artifact.gnArgs })) {
    const bytes = await safeFile(root, record.path, `${expectedTarget}.${name}`);
    recordBytes(record, bytes, `${expectedTarget}.${name}`);
    paths.add(record.path);
    loaded.set(record.path, bytes);
  }
  for (const notice of artifact.notices) {
    const path = `${artifact.noticeRoot}/${safeRelative(notice.path, `${expectedTarget} notice.path`)}`;
    const bytes = await safeFile(root, path, `${expectedTarget} notice ${notice.path}`);
    recordBytes({ ...notice, path }, bytes, `${expectedTarget} notice ${notice.path}`);
    paths.add(path);
    loaded.set(path, bytes);
  }
  const reviews = {};
  for (const [name, type] of [["redistributionReview", "redistribution-legal"], ["supplierReview", "supplier-provenance"]]) {
    const record = approvedManifest[name];
    const bytes = await safeFile(root, record.path, `${expectedTarget} ${name}`);
    recordBytes(record, bytes, `${expectedTarget} ${name}`);
    const review = JSON.parse(bytes.toString("utf8"));
    validateHumanReview(review, type, candidate, artifact, sha256(candidateBytes));
    paths.add(record.path);
    loaded.set(record.path, bytes);
    reviews[name] = { record, review, bytes };
  }
  if (approvedManifest.redistributionReview.path === approvedManifest.supplierReview.path) fail(`${expectedTarget} supplier and redistribution reviews must be separate records`);
  const existing = await inventory(root);
  const missing = [...paths].filter((path) => !existing.includes(path));
  const extra = existing.filter((path) => !paths.has(path));
  if (missing.length || extra.length) fail(`${expectedTarget} artifact inventory mismatch${missing.length ? `; missing: ${missing.join(", ")}` : ""}${extra.length ? `; extra: ${extra.join(", ")}` : ""}`);
  return { target: expectedTarget, root, approvedManifest, approvedManifestBytes, candidate, candidateBytes, artifact, reviews, files: loaded };
}

function rebaseRecord(record, prefix) {
  return { ...record, path: `${prefix}/${record.path}` };
}

export async function aggregatePdfiumApprovals({ inputs, outputRoot }) {
  if (!Array.isArray(inputs)) fail("inputs must be an array of target approval records");
  const byTarget = new Map();
  for (const input of inputs) {
    if (!targetSet.has(input?.target)) fail(`unsupported PDFium approval target ${input?.target}`);
    if (byTarget.has(input.target)) fail(`duplicate PDFium approval target ${input.target}`);
    byTarget.set(input.target, input);
  }
  const missing = requiredTargets.filter((target) => !byTarget.has(target));
  if (missing.length) fail(`missing required PDFium approval targets: ${missing.join(", ")}`);
  const ordered = requiredTargets;
  const approvals = [];
  for (const target of ordered) approvals.push(await readInput(byTarget.get(target), target));
  const first = approvals[0].approvedManifest;
  for (const entry of approvals.slice(1)) {
    if (!sameJson(entry.approvedManifest.wrapper, first.wrapper) || !sameJson(entry.approvedManifest.source, first.source)) fail("PDFium approvals do not share one pinned source and wrapper identity");
    for (const field of ["apiBuild", "v8", "xfa", "debug", "sharedLibraryPatchSha256", "dependencyPolicyPatchSha256"]) if (entry.approvedManifest.build[field] !== first.build[field]) fail(`PDFium approvals disagree on build policy ${field}`);
    if (entry.target.endsWith("apple-darwin") && approvals[0].target.endsWith("apple-darwin") && entry.approvedManifest.build.minimumSystemVersion !== first.build.minimumSystemVersion) fail("macOS PDFium approvals disagree on minimum system version");
  }

  const destination = resolve(outputRoot);
  for (const input of approvals) {
    const withinInput = relative(input.root, destination);
    if (withinInput === "" || (!withinInput.startsWith("..") && !isAbsolute(withinInput))) fail("output artifact root must be outside every input artifact root");
  }
  try { await lstat(destination); fail("output artifact root must not already exist"); } catch (error) { if (error.code !== "ENOENT") throw error; }
  await mkdir(destination, { recursive: false, mode: 0o700 });
  try {
    const artifacts = [];
    const supplierIndex = { schemaVersion: 1, reviewType: "supplier-provenance", targets: [] };
    const redistributionIndex = { schemaVersion: 1, reviewType: "redistribution-legal", targets: [] };
    const expectedOutputFiles = new Set(["production-pdfium-approved.json"]);
    for (const entry of approvals) {
      const prefix = `targets/${entry.target}`;
      const approvalPrefix = `approval-evidence/${entry.target}`;
      for (const [sourcePath, bytes] of entry.files) {
        const destinationPath = `${prefix}/${sourcePath}`;
        const targetPath = join(destination, destinationPath);
        await mkdir(dirname(targetPath), { recursive: true });
        await writeFile(targetPath, bytes, { flag: "wx", mode: 0o644 });
        expectedOutputFiles.add(destinationPath);
      }
      for (const [name, bytes] of [["production-pdfium-candidate.json", entry.candidateBytes], ["production-pdfium-approved.json", entry.approvedManifestBytes]]) {
        const destinationPath = `${approvalPrefix}/${name}`;
        const targetPath = join(destination, destinationPath);
        await mkdir(dirname(targetPath), { recursive: true });
        await writeFile(targetPath, bytes, { flag: "wx", mode: 0o644 });
        expectedOutputFiles.add(destinationPath);
      }
      for (const [name, index] of [["supplierReview", supplierIndex], ["redistributionReview", redistributionIndex]]) {
        const item = entry.reviews[name];
        const evidenceName = name === "supplierReview" ? "supplier-review.json" : "redistribution-review.json";
        const evidencePath = `${approvalPrefix}/${evidenceName}`;
        const evidenceFile = join(destination, evidencePath);
        await mkdir(dirname(evidenceFile), { recursive: true });
        await writeFile(evidenceFile, item.bytes, { flag: "wx", mode: 0o644 });
        expectedOutputFiles.add(evidencePath);
        // The review is already copied under targets/<target>; this index binds that exact copy.
        index.targets.push({
          target: entry.target,
          approvedManifestSha256: sha256(entry.approvedManifestBytes),
          candidateManifestSha256: sha256(entry.candidateBytes),
          reviewPath: `${prefix}/${item.record.path}`,
          reviewBytes: item.record.bytes,
          reviewSha256: item.record.sha256,
          reviewer: item.review.reviewer,
          submitter: item.review.submitter,
          assertions: item.review.assertions,
          evidenceReferences: item.review.evidenceReferences,
          approvalEvidencePath: evidencePath,
        });
      }
      const artifact = structuredClone(entry.artifact);
      artifact.library = rebaseRecord(artifact.library, prefix);
      artifact.sbom = rebaseRecord(artifact.sbom, prefix);
      artifact.provenance = rebaseRecord(artifact.provenance, prefix);
      artifact.gnArgs = rebaseRecord(artifact.gnArgs, prefix);
      artifact.noticeRoot = `${prefix}/${artifact.noticeRoot}`;
      // Notice paths remain relative to noticeRoot because both production
      // stagers inventory and verify each record from that directory.
      artifact.notices = artifact.notices.map((notice) => ({ ...notice }));
      artifacts.push(artifact);
    }
    const writeIndex = async (name, value) => {
      const bytes = Buffer.from(`${JSON.stringify(value, null, 2)}\n`);
      await writeFile(join(destination, name), bytes, { flag: "wx", mode: 0o644 });
      expectedOutputFiles.add(name);
      return { path: name, bytes: bytes.length, sha256: sha256(bytes) };
    };
    const supplierReview = await writeIndex("approval-evidence/supplier-review-index.json", supplierIndex);
    const redistributionReview = await writeIndex("approval-evidence/redistribution-review-index.json", redistributionIndex);
    const manifest = {
      ...first,
      productionApproved: true,
      build: { ...first.build, toolchain: { approvals: "Each target retains its separately reviewed provenance record; see artifacts[].provenance." } },
      redistributionReview,
      supplierReview,
      artifacts,
    };
    // The combined manifest is the exact root consumed by stable-candidate workflow.
    validateNonMacProductionManifest({ ...manifest, artifacts: artifacts.filter((item) => !item.target.endsWith("apple-darwin")) });
    validateProductionManifest({ ...manifest, artifacts: artifacts.filter((item) => item.target.endsWith("apple-darwin")) });
    const manifestBytes = Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`);
    await writeFile(join(destination, "production-pdfium-approved.json"), manifestBytes, { flag: "wx", mode: 0o644 });
    const actual = await inventory(destination);
    if (!sameJson(actual, [...expectedOutputFiles].sort())) fail("aggregated PDFium artifact root has an unexpected file inventory");
    return manifest;
  } catch (error) {
    await rm(destination, { recursive: true, force: true });
    throw error;
  }
}

function parseArgs(argv) {
  const values = new Map();
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i];
    const value = argv[i + 1];
    if (!key?.startsWith("--") || !value || values.has(key)) fail("usage: aggregate-pdfium-approvals.mjs --inputs approved-targets.json --output-root DIR");
    values.set(key, value);
  }
  if (values.size !== 2 || !values.has("--inputs") || !values.has("--output-root")) fail("usage: aggregate-pdfium-approvals.mjs --inputs approved-targets.json --output-root DIR");
  return { inputsPath: values.get("--inputs"), outputRoot: values.get("--output-root") };
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(scriptPath)) {
  const args = parseArgs(process.argv.slice(2));
  const inputs = JSON.parse(await readFile(args.inputsPath, "utf8"));
  aggregatePdfiumApprovals({ inputs, outputRoot: args.outputRoot }).then((manifest) => process.stdout.write(`${JSON.stringify({ outputRoot: resolve(args.outputRoot), targetCount: manifest.artifacts.length }, null, 2)}\n`)).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
