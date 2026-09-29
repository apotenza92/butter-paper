import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { aggregatePdfiumApprovals } from "../scripts/aggregate-pdfium-approvals.mjs";

const targets = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "aarch64-pc-windows-msvc",
  "x86_64-pc-windows-msvc",
  "aarch64-unknown-linux-gnu",
  "x86_64-unknown-linux-gnu",
];
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");

async function fixture() {
  const temp = await mkdtemp(join(tmpdir(), "bp-pdfium-aggregate-"));
  const allTargets = targets;
  const inputs = [];
  for (const [index, target] of allTargets.entries()) {
    const root = join(temp, `input-${index}`);
    const reviewDir = join(root, "reviews");
    const noticeDir = join(root, "notices");
    await mkdir(reviewDir, { recursive: true });
    await mkdir(noticeDir, { recursive: true });
    const libraryName = target.includes("windows") ? "pdfium.dll" : target.includes("apple") ? "libpdfium.dylib" : "libpdfium.so";
    const data = new Map([
      [libraryName, Buffer.from(`library:${target}`)],
      ["sbom.json", Buffer.from(`{"target":"${target}","bomFormat":"CycloneDX"}`)],
      ["provenance.json", Buffer.from(`{"target":"${target}","schema":"butter-paper/pdfium-build-provenance"}`)],
      ["gn-args.txt", Buffer.from("pdf_enable_v8 = false\npdf_enable_xfa = false\nis_debug = false\n")],
      ["notices/LICENSE", Buffer.from(`PDFium licence for ${target}\n`)],
      ["reviews/redistribution-review.txt", Buffer.from("UNREVIEWED PLACEHOLDER. Confirm source and dependency rights.\n")],
      ["reviews/supplier-review.txt", Buffer.from("UNREVIEWED PLACEHOLDER. Confirm source and dependency origins.\n")],
    ]);
    for (const [path, bytes] of data) {
      const pathOnDisk = join(root, path);
      await mkdir(join(pathOnDisk, ".."), { recursive: true });
      await writeFile(pathOnDisk, bytes);
    }
    const record = (path) => ({ path, bytes: data.get(path).length, sha256: hash(data.get(path)) });
    const candidate = {
      schemaVersion: 1,
      purpose: "production-distribution",
      productionApproved: false,
      wrapper: { package: "pdfium-render", version: "0.9.4", revision: "6cee8b9a3951832ac0ff62ce4c32800278001cb8", feature: "pdfium_7881" },
      source: { repository: "https://pdfium.googlesource.com/pdfium", revision: "91b9d569b34be4f38eed7b3c49b227356c3aadad" },
      build: { apiBuild: 7881, v8: false, xfa: false, debug: false, sharedLibraryPatchSha256: "1e521b48561c51a63425baeec7c74c1edaf65956b6e6297d98aff57f2cc2ee40", dependencyPolicyPatchSha256: "54591df969f7a323c24f78e435340e8177081afc56dceaf1ec8102362a40601b", toolchain: { gn: "1", ninja: "1" }, ...(target.endsWith("apple-darwin") ? { minimumSystemVersion: "13.0" } : {}) },
      redistributionReview: record("reviews/redistribution-review.txt"),
      supplierReview: record("reviews/supplier-review.txt"),
      artifacts: [{
        target,
        library: record(libraryName),
        sbom: record("sbom.json"),
        provenance: record("provenance.json"),
        gnArgs: record("gn-args.txt"),
        noticeRoot: "notices",
        notices: [{ ...record("notices/LICENSE"), path: "LICENSE" }],
      }],
    };
    const candidateBytes = Buffer.from(`${JSON.stringify(candidate, null, 2)}\n`);
    await writeFile(join(root, "production-pdfium-candidate.json"), candidateBytes);
    const artifact = candidate.artifacts[0];
    const noticesBinding = [{ path: "LICENSE", bytes: data.get("notices/LICENSE").length, sha256: hash(data.get("notices/LICENSE")) }];
    const binding = {
      candidateManifestSha256: hash(candidateBytes),
      target,
      librarySha256: artifact.library.sha256,
      sbomSha256: artifact.sbom.sha256,
      provenanceSha256: artifact.provenance.sha256,
      gnArgsSha256: artifact.gnArgs.sha256,
      noticesSha256: hash(Buffer.from(JSON.stringify(noticesBinding))),
      identities: {
        pdfiumRevision: candidate.source.revision,
        wrapperPackage: candidate.wrapper.package,
        wrapperVersion: candidate.wrapper.version,
        wrapperRevision: candidate.wrapper.revision,
        wrapperFeature: candidate.wrapper.feature,
        patchSha256: candidate.build.sharedLibraryPatchSha256,
        dependencyPolicyPatchSha256: candidate.build.dependencyPolicyPatchSha256,
      },
    };
    const makeReview = (reviewType, identity) => ({
      schemaVersion: 1,
      reviewType,
      decision: "approved",
      summary: `Independent ${reviewType} review for ${target}.`,
      evidenceReferences: [`review-record:${target}:${reviewType}`],
      unresolvedIssues: [],
      assertions: Object.fromEntries((reviewType === "redistribution-legal"
        ? ["sourceRedistributionRightsConfirmed", "dependencyRedistributionRightsConfirmed", "noticeInventoryComplete", "shippingObligationsRecorded"]
        : ["sourceIdentityConfirmed", "dependencyOriginsConfirmed", "sbomComponentsResolved", "noticeOriginsConfirmed"]
      ).map((key) => [key, true])),
      reviewer: { identity, reference: `ticket:${identity}:${target}`, timestamp: "2026-09-29T01:02:03.000Z" },
      submitter: "release-coordinator",
      binding,
    });
    const redistribution = makeReview("redistribution-legal", `legal-${target}`);
    const supplier = makeReview("supplier-provenance", `supply-${target}`);
    const redistributionPath = "reviews/approved-redistribution.json";
    const supplierPath = "reviews/approved-supplier.json";
    const redistributionBytes = Buffer.from(`${JSON.stringify(redistribution, null, 2)}\n`);
    const supplierBytes = Buffer.from(`${JSON.stringify(supplier, null, 2)}\n`);
    await writeFile(join(root, redistributionPath), redistributionBytes);
    await writeFile(join(root, supplierPath), supplierBytes);
    const approved = {
      ...candidate,
      productionApproved: true,
      redistributionReview: { path: redistributionPath, bytes: redistributionBytes.length, sha256: hash(redistributionBytes) },
      supplierReview: { path: supplierPath, bytes: supplierBytes.length, sha256: hash(supplierBytes) },
    };
    const approvedManifestPath = join(temp, `approved-${index}.json`);
    await writeFile(approvedManifestPath, `${JSON.stringify(approved, null, 2)}\n`);
    inputs.push({ target, artifactRoot: root, approvedManifestPath });
  }
  return { temp, inputs };
}

test("merges the six required independently approved targets into a stageable exact root", async (t) => {
  const f = await fixture();
  t.after(() => rm(f.temp, { recursive: true, force: true }));
  const outputRoot = join(f.temp, "aggregate");
  const manifest = await aggregatePdfiumApprovals({ inputs: f.inputs, outputRoot });
  assert.equal(manifest.productionApproved, true);
  assert.deepEqual(manifest.artifacts.map(({ target }) => target), targets);
  assert.match(manifest.redistributionReview.path, /redistribution-review-index\.json$/);
  assert.match(manifest.supplierReview.path, /supplier-review-index\.json$/);
  assert.notEqual(manifest.redistributionReview.path, manifest.supplierReview.path);
  for (const artifact of manifest.artifacts) {
    assert.match(artifact.library.path, new RegExp(`^targets/${artifact.target}/`));
    assert.equal(artifact.noticeRoot, `targets/${artifact.target}/notices`);
    assert.deepEqual(artifact.notices.map(({ path }) => path), ["LICENSE"]);
    const noticeBytes = await readFile(join(outputRoot, artifact.noticeRoot, artifact.notices[0].path));
    assert.equal(hash(noticeBytes), artifact.notices[0].sha256);
  }
  const supplier = JSON.parse(await readFile(join(outputRoot, manifest.supplierReview.path), "utf8"));
  const redistribution = JSON.parse(await readFile(join(outputRoot, manifest.redistributionReview.path), "utf8"));
  assert.equal(supplier.targets.length, 6);
  assert.equal(redistribution.targets.length, 6);
  assert.notEqual(supplier.reviewType, redistribution.reviewType);
  for (const row of supplier.targets) assert.equal(row.reviewSha256.length, 64);
  assert.equal(JSON.parse(await readFile(join(outputRoot, "production-pdfium-approved.json"), "utf8")).artifacts.length, 6);
});

test("requires macOS Intel with the other release targets", async (t) => {
  const f = await fixture();
  t.after(() => rm(f.temp, { recursive: true, force: true }));
  const withoutIntel = f.inputs.filter(({ target }) => target !== "x86_64-apple-darwin");
  await assert.rejects(aggregatePdfiumApprovals({ inputs: withoutIntel, outputRoot: join(f.temp, "aggregate") }), /missing required PDFium approval targets: x86_64-apple-darwin/);
});

test("rejects missing, duplicate, and unknown target approvals", async (t) => {
  const f = await fixture();
  t.after(() => rm(f.temp, { recursive: true, force: true }));
  await assert.rejects(aggregatePdfiumApprovals({ inputs: f.inputs.slice(1), outputRoot: join(f.temp, "missing") }), /missing required/);
  await assert.rejects(aggregatePdfiumApprovals({ inputs: [...f.inputs, f.inputs[0]], outputRoot: join(f.temp, "duplicate") }), /duplicate/);
  await assert.rejects(aggregatePdfiumApprovals({ inputs: [...f.inputs, { ...f.inputs[0], target: "mips-unknown-linux-gnu" }], outputRoot: join(f.temp, "unknown") }), /unsupported/);
});

test("rejects artifact hash mismatches, extra files, traversal, and reviews bound to another candidate", async (t) => {
  const hashCase = await fixture();
  t.after(() => rm(hashCase.temp, { recursive: true, force: true }));
  await writeFile(join(hashCase.inputs[0].artifactRoot, "libpdfium.dylib"), "tampered");
  await assert.rejects(aggregatePdfiumApprovals({ inputs: hashCase.inputs, outputRoot: join(hashCase.temp, "hash-output") }), /does not match its approved byte receipt/);

  const extraCase = await fixture();
  t.after(() => rm(extraCase.temp, { recursive: true, force: true }));
  await writeFile(join(extraCase.inputs[1].artifactRoot, "unexpected.bin"), "extra");
  await assert.rejects(aggregatePdfiumApprovals({ inputs: extraCase.inputs, outputRoot: join(extraCase.temp, "extra-output") }), /inventory mismatch/);

  const traversalCase = await fixture();
  t.after(() => rm(traversalCase.temp, { recursive: true, force: true }));
  const approved = JSON.parse(await readFile(traversalCase.inputs[0].approvedManifestPath, "utf8"));
  approved.artifacts[0].library.path = "../libpdfium.dylib";
  await writeFile(traversalCase.inputs[0].approvedManifestPath, JSON.stringify(approved));
  await assert.rejects(aggregatePdfiumApprovals({ inputs: traversalCase.inputs, outputRoot: join(traversalCase.temp, "traversal-output") }), /safe relative path/);

  const staleCase = await fixture();
  t.after(() => rm(staleCase.temp, { recursive: true, force: true }));
  const reviewPath = join(staleCase.inputs[0].artifactRoot, "reviews/approved-supplier.json");
  const review = JSON.parse(await readFile(reviewPath, "utf8"));
  review.binding.candidateManifestSha256 = "f".repeat(64);
  const reviewBytes = Buffer.from(JSON.stringify(review));
  await writeFile(reviewPath, reviewBytes);
  const approvedStale = JSON.parse(await readFile(staleCase.inputs[0].approvedManifestPath, "utf8"));
  approvedStale.supplierReview.bytes = reviewBytes.length;
  approvedStale.supplierReview.sha256 = hash(reviewBytes);
  await writeFile(staleCase.inputs[0].approvedManifestPath, JSON.stringify(approvedStale));
  await assert.rejects(aggregatePdfiumApprovals({ inputs: staleCase.inputs, outputRoot: join(staleCase.temp, "stale-output") }), /stale or bound/);
});
