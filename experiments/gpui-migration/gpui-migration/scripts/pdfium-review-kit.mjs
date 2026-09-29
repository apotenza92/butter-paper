#!/usr/bin/env node

import { createHash } from "node:crypto";
import { lstat, mkdir, open, readFile, readdir, rm, unlink, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const requiredTargets = [
  "aarch64-apple-darwin", "x86_64-apple-darwin",
  "aarch64-pc-windows-msvc", "x86_64-pc-windows-msvc",
  "aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu",
];
const canonicalTargets = requiredTargets;
const wrapperPin = { package: "pdfium-render", version: "0.9.4", revision: "6cee8b9a3951832ac0ff62ce4c32800278001cb8", feature: "pdfium_7881" };
const pdfiumRevision = "91b9d569b34be4f38eed7b3c49b227356c3aadad";
const patchSha256 = "1e521b48561c51a63425baeec7c74c1edaf65956b6e6297d98aff57f2cc2ee40";
const dependencyPatchSha256 = "54591df969f7a323c24f78e435340e8177081afc56dceaf1ec8102362a40601b";
const kinds = {
  redistributionReview: {
    type: "redistribution-legal",
    assertions: ["sourceRedistributionRightsConfirmed", "dependencyRedistributionRightsConfirmed", "noticeInventoryComplete", "shippingObligationsRecorded"],
  },
  supplierReview: {
    type: "supplier-provenance",
    assertions: ["sourceIdentityConfirmed", "dependencyOriginsConfirmed", "sbomComponentsResolved", "noticeOriginsConfirmed"],
  },
};
const placeholder = /\b(?:unreviewed|placeholder|incomplete|todo|tbd|example|sample)\b/i;
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const fail = (message) => { throw new Error(message); };

function safeRelative(value, label) {
  if (typeof value !== "string" || !value || isAbsolute(value) || /^[A-Za-z]:[\\/]/.test(value) || value.includes("\\") || value.split("/").some((part) => !part || part === "." || part === "..")) fail(`${label} must be a safe relative path`);
  return value;
}

async function safeFile(root, path, label) {
  safeRelative(path, label);
  let current = root;
  for (const part of path.split("/")) {
    current = join(current, part);
    const stat = await lstat(current);
    if (stat.isSymbolicLink()) fail(`${label} must not traverse a symlink`);
  }
  const stat = await lstat(current);
  if (!stat.isFile() || stat.nlink !== 1) fail(`${label} must be a regular single-link file`);
  return readFile(current);
}

async function inventory(root, prefix = "") {
  const files = [];
  for (const entry of await readdir(prefix ? join(root, prefix) : root, { withFileTypes: true })) {
    const path = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isSymbolicLink()) fail(`candidate contains symlink ${path}`);
    if (entry.isDirectory()) files.push(...await inventory(root, path));
    else if (entry.isFile()) {
      if ((await lstat(join(root, path))).nlink !== 1) fail(`candidate contains hard link ${path}`);
      files.push(path);
    } else fail(`candidate contains special file ${path}`);
  }
  return files.sort();
}

async function directoryInventory(root, prefix = "") {
  const directories = [];
  for (const entry of await readdir(prefix ? join(root, prefix) : root, { withFileTypes: true })) {
    const path = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isSymbolicLink()) fail(`directory tree contains symlink ${path}`);
    if (entry.isDirectory()) directories.push(path, ...await directoryInventory(root, path));
  }
  return directories.sort();
}

async function assertDirectory(root, path, label) {
  let current = root;
  for (const part of path.split("/")) {
    current = join(current, part);
    const stat = await lstat(current);
    if (stat.isSymbolicLink() || !stat.isDirectory()) fail(`${label} must be a real directory without symlink traversal`);
  }
  return current;
}

function assertRecord(record, label) {
  if (!record || typeof record !== "object" || Array.isArray(record)) fail(`${label} must be an object`);
  safeRelative(record.path, `${label}.path`);
  if (!Number.isSafeInteger(record.bytes) || record.bytes <= 0 || !/^[0-9a-f]{64}$/.test(record.sha256 ?? "")) fail(`${label} must contain positive bytes and a lowercase SHA-256`);
}

function bindings(manifest, artifact, digest) {
  const noticeRecords = artifact.notices.map(({ path, bytes, sha256: hash }) => ({ path, bytes, sha256: hash })).sort((a, b) => a.path.localeCompare(b.path));
  return {
    candidateManifestSha256: digest,
    target: artifact.target,
    librarySha256: artifact.library.sha256,
    sbomSha256: artifact.sbom.sha256,
    provenanceSha256: artifact.provenance.sha256,
    gnArgsSha256: artifact.gnArgs.sha256,
    noticesSha256: sha256(Buffer.from(JSON.stringify(noticeRecords))),
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

function validateReview(review, key, expected) {
  const { type, assertions } = kinds[key];
  if (!review || typeof review !== "object" || Array.isArray(review) || review.schemaVersion !== 1 || review.reviewType !== type || review.decision !== "approved") fail(`${type} review must be an explicit schemaVersion 1 approved decision`);
  if (typeof review.summary !== "string" || !review.summary.trim() || placeholder.test(review.summary)) fail(`${type} summary is missing or a placeholder`);
  if (!Array.isArray(review.evidenceReferences) || !review.evidenceReferences.length || review.evidenceReferences.some((item) => typeof item !== "string" || !item.trim() || placeholder.test(item))) fail(`${type} evidence references are missing or placeholders`);
  if (!Array.isArray(review.unresolvedIssues) || review.unresolvedIssues.length) fail(`${type} unresolvedIssues must be an empty array`);
  if (!review.assertions || typeof review.assertions !== "object" || Array.isArray(review.assertions) || assertions.some((field) => review.assertions[field] !== true)) fail(`${type} review must explicitly affirm every required assertion`);
  const reviewer = review.reviewer;
  if (!reviewer || typeof reviewer !== "object" || Array.isArray(reviewer)) fail(`${type} reviewer is required`);
  for (const field of ["identity", "reference", "timestamp"]) if (typeof reviewer[field] !== "string" || !reviewer[field].trim() || placeholder.test(reviewer[field])) fail(`${type} reviewer.${field} is missing or a placeholder`);
  if (!Number.isFinite(Date.parse(reviewer.timestamp)) || new Date(reviewer.timestamp).toISOString() !== reviewer.timestamp) fail(`${type} reviewer.timestamp must be an ISO-8601 UTC timestamp`);
  if (typeof review.submitter !== "string" || !review.submitter.trim() || placeholder.test(review.submitter)) fail(`${type} submitter identity is required`);
  if (reviewer.identity.toLowerCase() === review.submitter.toLowerCase()) fail(`${type} review cannot be self-approved`);
  const binding = review.binding;
  if (!binding || binding.candidateManifestSha256 !== expected.candidateManifestSha256) fail(`${type} review is stale or bound to another candidate manifest`);
  for (const field of ["target", "librarySha256", "sbomSha256", "provenanceSha256", "gnArgsSha256", "noticesSha256"]) if (binding[field] !== expected[field]) fail(`${type} review binding ${field} does not match candidate`);
  if (!binding.identities || Object.keys(expected.identities).some((field) => binding.identities[field] !== expected.identities[field])) fail(`${type} review pinned identities do not match candidate`);
}

async function readCandidate(target, root) {
  const candidateRoot = resolve(root);
  const rootStat = await lstat(candidateRoot);
  if (!rootStat.isDirectory() || rootStat.isSymbolicLink()) fail(`${target} artifact root must be a real directory`);
  const manifestBytes = await safeFile(candidateRoot, "production-pdfium-candidate.json", `${target} candidate manifest`);
  const manifest = JSON.parse(manifestBytes.toString("utf8"));
  if (manifest.schemaVersion !== 1 || manifest.purpose !== "production-distribution" || manifest.productionApproved !== false || !Array.isArray(manifest.artifacts) || manifest.artifacts.length !== 1 || manifest.artifacts[0].target !== target) fail(`${target} must be one unapproved single-target production candidate`);
  if (JSON.stringify(manifest.wrapper) !== JSON.stringify(wrapperPin) || manifest.source?.repository !== "https://pdfium.googlesource.com/pdfium" || manifest.source?.revision !== pdfiumRevision) fail(`${target} wrapper or PDFium source identity does not match reviewed pins`);
  if (manifest.build?.apiBuild !== 7881 || manifest.build?.v8 !== false || manifest.build?.xfa !== false || manifest.build?.debug !== false || manifest.build?.sharedLibraryPatchSha256 !== patchSha256 || manifest.build?.dependencyPolicyPatchSha256 !== dependencyPatchSha256) fail(`${target} build policy or source patch digests do not match reviewed pins`);
  const artifact = manifest.artifacts[0];
  for (const name of ["library", "sbom", "provenance", "gnArgs"]) assertRecord(artifact[name], `${target}.${name}`);
  if (!Array.isArray(artifact.notices) || !artifact.notices.length || new Set(artifact.notices.map((notice) => notice.path)).size !== artifact.notices.length) fail(`${target} notice inventory is empty or duplicated`);
  safeRelative(artifact.noticeRoot, `${target}.noticeRoot`);
  for (const notice of artifact.notices) assertRecord(notice, `${target}.notice`);
  const allowed = new Set(["production-pdfium-candidate.json"]);
  for (const name of ["redistributionReview", "supplierReview"]) {
    assertRecord(manifest[name], `${target}.${name}`);
    const bytes = await safeFile(candidateRoot, manifest[name].path, `${target}.${name}`);
    if (bytes.length !== manifest[name].bytes || sha256(bytes) !== manifest[name].sha256 || !/unreviewed|placeholder/i.test(bytes.toString("utf8"))) fail(`${target}.${name} must remain the digest-verified unreviewed placeholder`);
    allowed.add(manifest[name].path);
  }
  for (const record of [artifact.library, artifact.sbom, artifact.provenance, artifact.gnArgs]) {
    const bytes = await safeFile(candidateRoot, record.path, `${target} ${record.path}`);
    if (bytes.length !== record.bytes || sha256(bytes) !== record.sha256) fail(`${target} ${record.path} does not match candidate manifest digest`);
    allowed.add(record.path);
  }
  for (const notice of artifact.notices) {
    const path = `${artifact.noticeRoot}/${safeRelative(notice.path, `${target} notice path`)}`;
    const bytes = await safeFile(candidateRoot, path, `${target} notice ${notice.path}`);
    if (bytes.length !== notice.bytes || sha256(bytes) !== notice.sha256) fail(`${target} notice ${notice.path} does not match candidate manifest digest`);
    allowed.add(path);
  }
  const files = await inventory(candidateRoot);
  if (files.some((path) => !allowed.has(path)) || [...allowed].some((path) => !files.includes(path))) fail(`${target} candidate inventory mismatch`);
  const expectedDirectories = new Set();
  for (const path of allowed) {
    const parts = path.split("/");
    for (let i = 1; i < parts.length; i += 1) expectedDirectories.add(parts.slice(0, i).join("/"));
  }
  const actualDirectories = await directoryInventory(candidateRoot);
  if (actualDirectories.some((path) => !expectedDirectories.has(path)) || [...expectedDirectories].some((path) => !actualDirectories.includes(path))) fail(`${target} candidate directory inventory mismatch`);
  return { target, root: candidateRoot, manifest, manifestBytes, artifact, binding: bindings(manifest, artifact, sha256(manifestBytes)), files };
}

function template(key, binding) {
  const { type, assertions } = kinds[key];
  return {
    schemaVersion: 1,
    reviewType: type,
    decision: "pending",
    summary: "",
    evidenceReferences: [],
    unresolvedIssues: [],
    assertions: Object.fromEntries(assertions.map((field) => [field, false])),
    reviewer: { identity: "", reference: "", timestamp: "" },
    submitter: "",
    binding,
  };
}

export async function preparePdfiumReviewKit({ inputs, outputDirectory, sourceRunId = "", sourceRunAttempt = "" }) {
  if (!Array.isArray(inputs) || inputs.length !== requiredTargets.length) fail("prepare requires all six mandatory candidates");
  const selectedTargets = canonicalTargets;
  const byTarget = new Map();
  for (const input of inputs) {
    if (!input || !canonicalTargets.includes(input.target) || byTarget.has(input.target) || typeof input.artifactRoot !== "string") fail("prepare inputs must contain each exact production target once");
    byTarget.set(input.target, input.artifactRoot);
  }
  if (selectedTargets.some((target) => !byTarget.has(target)) || inputs.map(({ target }) => target).join("\n") !== selectedTargets.join("\n")) fail("prepare inputs must list all six mandatory targets in canonical order");
  const candidates = [];
  for (const target of selectedTargets) candidates.push(await readCandidate(target, byTarget.get(target)));
  const destination = resolve(outputDirectory);
  for (const candidate of candidates) {
    const within = relative(candidate.root, destination);
    if (within === "" || (!within.startsWith("..") && !isAbsolute(within))) fail("review packet output must be outside every candidate root");
  }
  await mkdir(destination, { recursive: false, mode: 0o700 });
  try {
    for (const candidate of candidates) {
      const targetRoot = join(destination, candidate.target);
      for (const path of candidate.files) {
        const out = join(targetRoot, "candidate", path);
        await mkdir(dirname(out), { recursive: true });
        await writeFile(out, await safeFile(candidate.root, path, `${candidate.target} ${path}`), { flag: "wx", mode: 0o644 });
      }
      const reviewRoot = join(targetRoot, "reviews");
      await mkdir(reviewRoot, { recursive: true });
      for (const key of Object.keys(kinds)) await writeFile(join(reviewRoot, `${key}.json`), `${JSON.stringify(template(key, candidate.binding), null, 2)}\n`, { flag: "wx", mode: 0o644 });
    }
    const packet = {
      schemaVersion: 1,
      purpose: "human-review-packet",
      approvalStatus: "pending",
      sourceRunId,
      sourceRunAttempt,
      targets: candidates.map(({ target, manifestBytes, binding }) => ({ target, candidateManifestSha256: sha256(manifestBytes), binding })),
      instructions: "Review the candidate evidence under each target/candidate directory. Complete both JSON files under each target/reviews directory. Set decision to approved only after review; record evidence, assertions, reviewer identity/reference, UTC timestamp, and submitter.",
    };
    await writeFile(join(destination, "review-kit.json"), `${JSON.stringify(packet, null, 2)}\n`, { flag: "wx", mode: 0o644 });
    await writeFile(join(destination, "README.md"), `# PDFium production candidate review\n\nStatus: pending human review. This packet does not approve or promote any candidate.\n\nThe packet contains all six mandatory targets. For each included target, inspect the files in \`candidate/\` and complete both review templates in \`reviews/\`. Keep the binding fields unchanged.\n\nAfter both reviews for every included target are complete, assemble them with \`pdfium-review-kit.mjs assemble --packet-dir <this-directory> --output <review-bundle.json> --base64-output <review-bundle.base64>\`.\n`, { flag: "wx", mode: 0o644 });
    return packet;
  } catch (error) {
    await rm(destination, { recursive: true, force: true });
    throw error;
  }
}

export async function assemblePdfiumReviewKit({ packetDirectory, outputPath, base64OutputPath }) {
  const root = resolve(packetDirectory);
  const rootStat = await lstat(root);
  if (!rootStat.isDirectory() || rootStat.isSymbolicLink()) fail("review packet root must be a real directory");
  const packetBytes = await safeFile(root, "review-kit.json", "review-kit.json");
  const packet = JSON.parse(packetBytes.toString("utf8"));
  const packetTargets = packet?.targets?.map(({ target }) => target);
  const selectedTargets = canonicalTargets;
  if (packet.schemaVersion !== 1 || packet.purpose !== "human-review-packet" || packet.approvalStatus !== "pending" || !Array.isArray(packet.targets) || packet.targets.length !== requiredTargets.length || packetTargets.join("\n") !== selectedTargets.join("\n")) fail("review packet must list all six mandatory targets in canonical order");
  const outputPaths = [resolve(outputPath), resolve(base64OutputPath)];
  if (outputPaths[0] === outputPaths[1]) fail("JSON and base64 output paths must be separate files");
  for (const path of outputPaths) {
    const within = relative(root, path);
    if (within === "" || (!within.startsWith("..") && !isAbsolute(within))) fail("assembled review outputs must be outside the review packet");
  }
  const bundle = {};
  for (const target of selectedTargets) {
    await assertDirectory(root, target, `${target} packet directory`);
    await assertDirectory(root, `${target}/candidate`, `${target} candidate root`);
    await assertDirectory(root, `${target}/reviews`, `${target} review directory`);
    const candidateRoot = join(root, target, "candidate");
    const candidate = await readCandidate(target, candidateRoot);
    const row = packet.targets.find((item) => item.target === target);
    if (row.candidateManifestSha256 !== sha256(candidate.manifestBytes) || JSON.stringify(row.binding) !== JSON.stringify(candidate.binding)) fail(`${target} candidate changed since packet preparation`);
    const reviews = {};
    const reviewDirectory = join(root, target, "reviews");
    const reviewFiles = await inventory(reviewDirectory);
    const expectedReviewFiles = Object.keys(kinds).map((key) => `${key}.json`).sort();
    if (reviewFiles.join("\n") !== expectedReviewFiles.join("\n") || (await directoryInventory(reviewDirectory)).length) fail(`${target} review directory must contain exactly the two review templates`);
    for (const key of Object.keys(kinds)) {
      const bytes = await safeFile(reviewDirectory, `${key}.json`, `${target} ${key}`);
      const review = JSON.parse(bytes.toString("utf8"));
      validateReview(review, key, candidate.binding);
      reviews[key] = review;
    }
    bundle[target] = reviews;
  }
  const json = `${JSON.stringify(bundle, null, 2)}\n`;
  const encoded = Buffer.from(JSON.stringify(bundle)).toString("base64");
  if (encoded.length > 60000) fail(`base64 review bundle exceeds workflow limit: ${encoded.length} > 60000`);
  const created = [];
  try {
    for (const [path, content] of [[resolve(outputPath), json], [resolve(base64OutputPath), `${encoded}\n`]]) {
      const handle = await open(path, "wx", 0o600);
      created.push(path);
      try { await handle.writeFile(content); } finally { await handle.close(); }
    }
  } catch (error) {
    await Promise.all(created.map((path) => unlink(path).catch(() => {})));
    throw error;
  }
  return { targets: selectedTargets.length, base64Length: encoded.length };
}

function parseArgs(argv) {
  const [mode, ...rest] = argv;
  const values = new Map();
  for (let i = 0; i < rest.length; i += 2) {
    const key = rest[i]; const value = rest[i + 1];
    if (!key?.startsWith("--") || !value || values.has(key)) fail("invalid arguments");
    values.set(key, value);
  }
  if (mode === "prepare" && values.has("--inputs") && values.has("--output-directory")) return { mode, inputsPath: values.get("--inputs"), outputDirectory: values.get("--output-directory"), sourceRunId: values.get("--source-run-id") ?? "", sourceRunAttempt: values.get("--source-run-attempt") ?? "" };
  if (mode === "assemble" && values.has("--packet-dir") && values.has("--output") && values.has("--base64-output")) return { mode, packetDirectory: values.get("--packet-dir"), outputPath: values.get("--output"), base64OutputPath: values.get("--base64-output") };
  fail("usage: pdfium-review-kit.mjs prepare --inputs inputs.json --output-directory DIR [--source-run-id ID --source-run-attempt N] | assemble --packet-dir DIR --output FILE --base64-output FILE");
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) {
  const args = parseArgs(process.argv.slice(2));
  const task = args.mode === "prepare"
    ? readFile(args.inputsPath, "utf8").then((text) => preparePdfiumReviewKit({ ...args, inputs: JSON.parse(text) }))
    : assemblePdfiumReviewKit(args);
  task.then((result) => process.stdout.write(`${JSON.stringify(result, null, 2)}\n`)).catch((error) => { console.error(error.message); process.exitCode = 1; });
}
