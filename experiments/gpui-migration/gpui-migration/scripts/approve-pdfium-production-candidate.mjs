#!/usr/bin/env node

import { createHash } from "node:crypto";
import {
  lstat,
  readFile,
  readdir,
  writeFile,
} from "node:fs/promises";
import { isAbsolute, join, relative, resolve } from "node:path";

const PINNED_WRAPPER = {
  package: "pdfium-render",
  version: "0.9.4",
  revision: "6cee8b9a3951832ac0ff62ce4c32800278001cb8",
  feature: "pdfium_7881",
};
const PINNED_PDFIUM_REVISION = "91b9d569b34be4f38eed7b3c49b227356c3aadad";
const PINNED_PATCH_SHA256 = "1e521b48561c51a63425baeec7c74c1edaf65956b6e6297d98aff57f2cc2ee40";
const PINNED_DEPENDENCY_PATCH_SHA256 = "54591df969f7a323c24f78e435340e8177081afc56dceaf1ec8102362a40601b";
const PLACEHOLDER = /\b(?:unreviewed|placeholder|incomplete|todo|tbd|example|sample)\b/i;

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function fail(message) {
  throw new Error(message);
}

function safeRelativePath(value, label) {
  if (
    typeof value !== "string" || !value || isAbsolute(value) ||
    /^[A-Za-z]:[\\/]/.test(value) || value.includes("\\") ||
    value.split("/").some((part) => !part || part === "." || part === "..")
  ) fail(`${label} must be a safe relative path`);
  return value;
}

function recordShape(record, label) {
  if (!record || typeof record !== "object" || Array.isArray(record)) fail(`${label} must be an object`);
  safeRelativePath(record.path, `${label}.path`);
  if (!Number.isSafeInteger(record.bytes) || record.bytes <= 0) fail(`${label}.bytes must be a positive safe integer`);
  if (!/^[0-9a-f]{64}$/.test(record.sha256 ?? "")) fail(`${label}.sha256 must be a lowercase SHA-256 digest`);
}

async function fileAt(root, path, label) {
  const safePath = safeRelativePath(path, label);
  const absolute = resolve(root, safePath);
  const rel = relative(root, absolute);
  if (!rel || rel === ".." || rel.startsWith(`..${process.platform === "win32" ? "\\" : "/"}`) || isAbsolute(rel)) fail(`${label} escapes the candidate root`);
  let current = root;
  for (const segment of safePath.split("/")) {
    current = join(current, segment);
    const metadata = await lstat(current);
    if (metadata.isSymbolicLink()) fail(`${label} must not traverse a symlink`);
  }
  const metadata = await lstat(absolute);
  if (!metadata.isFile() || metadata.nlink !== 1) fail(`${label} must be a regular single-link file`);
  return { path: safePath, bytes: await readFile(absolute) };
}

async function inventory(root, prefix = "") {
  const output = [];
  for (const item of await readdir(prefix ? join(root, prefix) : root, { withFileTypes: true })) {
    const path = prefix ? `${prefix}/${item.name}` : item.name;
    if (item.isSymbolicLink()) fail(`candidate contains symlink ${path}`);
    if (item.isDirectory()) output.push(...await inventory(root, path));
    else if (item.isFile()) {
      const metadata = await lstat(join(root, path));
      if (metadata.nlink !== 1) fail(`candidate contains hard link ${path}`);
      output.push(path);
    } else fail(`candidate contains special file ${path}`);
  }
  return output.sort();
}

async function directories(root, prefix = "") {
  const output = [];
  for (const item of await readdir(prefix ? join(root, prefix) : root, { withFileTypes: true })) {
    if (item.isSymbolicLink()) fail(`candidate contains symlink ${prefix ? `${prefix}/` : ""}${item.name}`);
    if (item.isDirectory()) {
      const path = prefix ? `${prefix}/${item.name}` : item.name;
      output.push(path, ...await directories(root, path));
    }
  }
  return output;
}

function expectedBindings(manifest, artifact, manifestDigest) {
  const notices = artifact.notices.map(({ path, bytes, sha256: digest }) => ({ path, bytes, sha256: digest })).sort((a, b) => a.path.localeCompare(b.path));
  return {
    candidateManifestSha256: manifestDigest,
    target: artifact.target,
    librarySha256: artifact.library.sha256,
    sbomSha256: artifact.sbom.sha256,
    provenanceSha256: artifact.provenance.sha256,
    gnArgsSha256: artifact.gnArgs.sha256,
    noticesSha256: sha256(Buffer.from(JSON.stringify(notices))),
    identities: {
      pdfiumRevision: manifest.source.revision,
      wrapperPackage: manifest.wrapper.package,
      wrapperVersion: manifest.wrapper.version,
      wrapperRevision: manifest.wrapper.revision,
      wrapperFeature: manifest.wrapper.feature,
      patchSha256: manifest.build.sharedLibraryPatchSha256,
      dependencyPolicyPatchSha256: manifest.build.dependencyPolicyPatchSha256,
    },
  };
}

function validateReview(review, kind, expected) {
  if (!review || typeof review !== "object" || Array.isArray(review) || review.schemaVersion !== 1 || review.reviewType !== kind || review.decision !== "approved") fail(`${kind} review must be an explicit schemaVersion 1 approved decision`);
  if (typeof review.summary !== "string" || !review.summary.trim() || PLACEHOLDER.test(review.summary)) fail(`${kind} review summary is missing or a placeholder`);
  if (!Array.isArray(review.evidenceReferences) || review.evidenceReferences.length === 0 || review.evidenceReferences.some((reference) => typeof reference !== "string" || !reference.trim() || PLACEHOLDER.test(reference))) fail(`${kind} review must include at least one non-placeholder evidence reference`);
  if (!Array.isArray(review.unresolvedIssues) || review.unresolvedIssues.length !== 0) fail(`${kind} review unresolvedIssues must be an empty array`);
  const assertions = kind === "redistribution-legal"
    ? ["sourceRedistributionRightsConfirmed", "dependencyRedistributionRightsConfirmed", "noticeInventoryComplete", "shippingObligationsRecorded"]
    : ["sourceIdentityConfirmed", "dependencyOriginsConfirmed", "sbomComponentsResolved", "noticeOriginsConfirmed"];
  if (!review.assertions || typeof review.assertions !== "object" || Array.isArray(review.assertions) || assertions.some((field) => review.assertions[field] !== true)) fail(`${kind} review must explicitly affirm every required assertion`);
  const reviewer = review.reviewer;
  if (!reviewer || typeof reviewer !== "object" || Array.isArray(reviewer)) fail(`${kind} review reviewer is required`);
  for (const [field, value] of Object.entries({ identity: reviewer.identity, reference: reviewer.reference, timestamp: reviewer.timestamp })) {
    if (typeof value !== "string" || !value.trim() || PLACEHOLDER.test(value)) fail(`${kind} review reviewer.${field} is missing or a placeholder`);
  }
  if (!Number.isFinite(Date.parse(reviewer.timestamp)) || new Date(reviewer.timestamp).toISOString() !== reviewer.timestamp) fail(`${kind} review reviewer.timestamp must be an ISO-8601 UTC timestamp`);
  if (typeof review.submitter !== "string" || !review.submitter.trim() || PLACEHOLDER.test(review.submitter)) fail(`${kind} review submitter identity is required`);
  if (reviewer.identity.toLowerCase() === review.submitter.toLowerCase()) fail(`${kind} review cannot be self-approved`);
  if (review.binding?.candidateManifestSha256 !== expected.candidateManifestSha256) fail(`${kind} review is stale or bound to another candidate manifest`);
  for (const field of ["target", "librarySha256", "sbomSha256", "provenanceSha256", "gnArgsSha256", "noticesSha256"]) {
    if (review.binding?.[field] !== expected[field]) fail(`${kind} review binding ${field} does not match candidate`);
  }
  if (!review.binding?.identities || Object.keys(expected.identities).some((field) => review.binding.identities[field] !== expected.identities[field])) fail(`${kind} review pinned identities do not match candidate`);
}

export async function approvePdfiumProductionCandidate({ artifactRoot, manifestPath, redistributionReviewPath, supplierReviewPath, outputPath }) {
  const root = resolve(artifactRoot);
  const rootStat = await lstat(root);
  if (!rootStat.isDirectory() || rootStat.isSymbolicLink()) fail("candidate artifact root must be a real directory");
  const manifestRelative = relative(root, resolve(manifestPath));
  if (!manifestRelative || manifestRelative.startsWith("..") || isAbsolute(manifestRelative)) fail("candidate manifest must be inside the artifact root");
  const manifestFile = await fileAt(root, manifestRelative, "candidate manifest");
  const manifest = JSON.parse(manifestFile.bytes.toString("utf8"));
  if (manifest.schemaVersion !== 1 || manifest.purpose !== "production-distribution" || manifest.productionApproved !== false) fail("input must be one exact unapproved production candidate manifest");
  if (JSON.stringify(manifest.wrapper) !== JSON.stringify(PINNED_WRAPPER)) fail("candidate wrapper pins do not match reviewed PDFium wrapper identity");
  if (manifest.source?.repository !== "https://pdfium.googlesource.com/pdfium" || manifest.source?.revision !== PINNED_PDFIUM_REVISION) fail("candidate PDFium source revision does not match the app-reviewed pin");
  if (manifest.build?.apiBuild !== 7881 || manifest.build?.v8 !== false || manifest.build?.xfa !== false || manifest.build?.debug !== false || manifest.build?.sharedLibraryPatchSha256 !== PINNED_PATCH_SHA256 || manifest.build?.dependencyPolicyPatchSha256 !== PINNED_DEPENDENCY_PATCH_SHA256) fail("candidate build policy or source patch digests do not match the app-reviewed pins");
  if (!Array.isArray(manifest.artifacts) || manifest.artifacts.length !== 1) fail("candidate manifest must contain exactly one target artifact");
  const artifact = manifest.artifacts[0];
  if (!/^(?:aarch64|x86_64)-(?:apple-darwin|pc-windows-msvc|unknown-linux-gnu)$/.test(artifact.target ?? "")) fail("candidate target is unsupported");
  for (const [name, record] of Object.entries({ library: artifact.library, sbom: artifact.sbom, provenance: artifact.provenance, gnArgs: artifact.gnArgs, ...Object.fromEntries((artifact.notices ?? []).map((notice, index) => [`notice ${index}`, notice])) })) recordShape(record, `${artifact.target}.${name}`);
  if (!Array.isArray(artifact.notices) || artifact.notices.length === 0 || new Set(artifact.notices.map((n) => n.path)).size !== artifact.notices.length) fail("candidate notice inventory is empty or duplicated");
  safeRelativePath(artifact.noticeRoot, "noticeRoot");
  for (const [label, record] of [["library", artifact.library], ["SBOM", artifact.sbom], ["provenance", artifact.provenance], ["GN args", artifact.gnArgs], ...artifact.notices.map((n) => [`notice ${n.path}`, { ...n, path: `${artifact.noticeRoot}/${n.path}` }])]) {
    const file = await fileAt(root, record.path, label);
    if (file.bytes.length !== record.bytes || sha256(file.bytes) !== record.sha256) fail(`${label} does not match the candidate manifest digest`);
  }
  for (const name of ["redistributionReview", "supplierReview"]) {
    recordShape(manifest[name], name);
    const old = await fileAt(root, manifest[name].path, name);
    if (old.bytes.length !== manifest[name].bytes || sha256(old.bytes) !== manifest[name].sha256 || !PLACEHOLDER.test(old.bytes.toString("utf8"))) fail(`${name} must be the workflow's unreviewed placeholder`);
  }
  const manifestDigest = sha256(manifestFile.bytes);
  const expected = expectedBindings(manifest, artifact, manifestDigest);
  const supplied = {};
  const reviewPaths = {};
  for (const [key, kind, path] of [["redistributionReview", "redistribution-legal", redistributionReviewPath], ["supplierReview", "supplier-provenance", supplierReviewPath]]) {
    const relativePath = relative(root, resolve(path));
    if (!relativePath || relativePath.startsWith("..") || isAbsolute(relativePath)) fail(`${key} JSON record must be supplied inside the candidate artifact root`);
    const file = await fileAt(root, relativePath, key);
    const review = JSON.parse(file.bytes.toString("utf8"));
    if (PLACEHOLDER.test(file.bytes.toString("utf8"))) fail(`${key} JSON contains placeholder text`);
    validateReview(review, kind, expected);
    supplied[key] = { path: relativePath, bytes: file.bytes.length, sha256: sha256(file.bytes) };
    reviewPaths[key] = relativePath;
  }
  if (reviewPaths.redistributionReview === reviewPaths.supplierReview) fail("the two reviews must be separate records");
  const allowed = new Set([manifestRelative, manifest.redistributionReview.path, manifest.supplierReview.path, ...Object.values(reviewPaths)]);
  for (const record of [artifact.library, artifact.sbom, artifact.provenance, artifact.gnArgs]) allowed.add(record.path);
  for (const notice of artifact.notices) allowed.add(`${artifact.noticeRoot}/${notice.path}`);
  const files = await inventory(root);
  const extras = files.filter((path) => !allowed.has(path));
  const missing = [...allowed].filter((path) => !files.includes(path));
  const expectedDirectories = new Set();
  for (const path of allowed) {
    const parts = path.split("/");
    for (let i = 1; i < parts.length; i += 1) expectedDirectories.add(parts.slice(0, i).join("/"));
  }
  const extraDirectories = (await directories(root)).filter((path) => !expectedDirectories.has(path));
  if (extras.length || missing.length || extraDirectories.length) fail(`candidate inventory mismatch${extras.length ? `; extra files: ${extras.join(", ")}` : ""}${missing.length ? `; missing files: ${missing.join(", ")}` : ""}${extraDirectories.length ? `; extra directories: ${extraDirectories.join(", ")}` : ""}`);
  const destination = resolve(outputPath);
  if (!relative(root, destination).startsWith("..") && relative(root, destination) !== "") fail("approved manifest output must be outside candidate root");
  const out = { ...manifest, productionApproved: true, redistributionReview: supplied.redistributionReview, supplierReview: supplied.supplierReview };
  await writeFile(destination, `${JSON.stringify(out, null, 2)}\n`, { flag: "wx", mode: 0o644 });
  return out;
}

export function argumentsFrom(argv) {
  const values = new Map();
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i];
    const value = argv[i + 1];
    if (!key?.startsWith("--") || !value || values.has(key)) fail("usage: approve-pdfium-production-candidate.mjs --artifact-root DIR --manifest FILE --redistribution-review FILE --supplier-review FILE --output FILE");
    values.set(key, value);
  }
  const keys = ["--artifact-root", "--manifest", "--redistribution-review", "--supplier-review", "--output"];
  if (values.size !== keys.length || keys.some((key) => !values.has(key))) fail("usage: approve-pdfium-production-candidate.mjs --artifact-root DIR --manifest FILE --redistribution-review FILE --supplier-review FILE --output FILE");
  return {
    artifactRoot: values.get("--artifact-root"),
    manifestPath: values.get("--manifest"),
    redistributionReviewPath: values.get("--redistribution-review"),
    supplierReviewPath: values.get("--supplier-review"),
    outputPath: values.get("--output"),
  };
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(new URL(import.meta.url).pathname)) {
  approvePdfiumProductionCandidate(argumentsFrom(process.argv.slice(2))).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
