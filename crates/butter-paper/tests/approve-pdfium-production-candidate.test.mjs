import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { approvePdfiumProductionCandidate, argumentsFrom } from "../scripts/approve-pdfium-production-candidate.mjs";
import { validateNonMacProductionManifest } from "../scripts/stage-nonmac-pdfium-production.mjs";

const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const target = "x86_64-unknown-linux-gnu";

async function fixture() {
  const temp = await mkdtemp(join(tmpdir(), "bp-pdfium-approval-"));
  const root = join(temp, "candidate");
  await mkdir(join(root, "notices"), { recursive: true });
  const data = new Map([
    ["libpdfium.so", Buffer.from("library bytes")],
    ["sbom.json", Buffer.from('{"bomFormat":"CycloneDX"}')],
    ["provenance.json", Buffer.from('{"schema":"butter-paper/pdfium-build-provenance"}')],
    ["gn-args.txt", Buffer.from("pdf_enable_v8 = false\npdf_enable_xfa = false\nis_debug = false\n")],
    ["notices/LICENSE", Buffer.from("PDFium licence\n")],
    ["reviews/redistribution-review.txt", Buffer.from("UNREVIEWED PLACEHOLDER. Confirm source rights.\n")],
    ["reviews/supplier-review.txt", Buffer.from("UNREVIEWED PLACEHOLDER. Confirm supplier.\n")],
  ]);
  for (const [path, bytes] of data) {
    await mkdir(join(root, path, ".."), { recursive: true });
    await writeFile(join(root, path), bytes);
  }
  const rec = (path) => ({ path, bytes: data.get(path).length, sha256: hash(data.get(path)) });
  const manifest = {
    schemaVersion: 1,
    purpose: "production-distribution",
    productionApproved: false,
    wrapper: { package: "pdfium-render", version: "0.9.4", revision: "6cee8b9a3951832ac0ff62ce4c32800278001cb8", feature: "pdfium_7881" },
    source: { repository: "https://pdfium.googlesource.com/pdfium", revision: "91b9d569b34be4f38eed7b3c49b227356c3aadad" },
    build: { apiBuild: 7881, v8: false, xfa: false, debug: false, sharedLibraryPatchSha256: "1e521b48561c51a63425baeec7c74c1edaf65956b6e6297d98aff57f2cc2ee40", dependencyPolicyPatchSha256: "54591df969f7a323c24f78e435340e8177081afc56dceaf1ec8102362a40601b", toolchain: { gn: "1", ninja: "1" } },
    redistributionReview: rec("reviews/redistribution-review.txt"),
    supplierReview: rec("reviews/supplier-review.txt"),
    artifacts: [{
      target,
      library: rec("libpdfium.so"),
      sbom: rec("sbom.json"),
      provenance: rec("provenance.json"),
      gnArgs: rec("gn-args.txt"),
      noticeRoot: "notices",
      notices: [{ ...rec("notices/LICENSE"), path: "LICENSE" }],
    }],
  };
  const manifestPath = join(root, "production-pdfium-candidate.json");
  const manifestBytes = Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`);
  await writeFile(manifestPath, manifestBytes);
  const manifestDigest = hash(manifestBytes);
  const binding = {
    candidateManifestSha256: manifestDigest,
    target,
    librarySha256: manifest.artifacts[0].library.sha256,
    sbomSha256: manifest.artifacts[0].sbom.sha256,
    provenanceSha256: manifest.artifacts[0].provenance.sha256,
    gnArgsSha256: manifest.artifacts[0].gnArgs.sha256,
    noticesSha256: hash(Buffer.from(JSON.stringify([{ path: "LICENSE", bytes: data.get("notices/LICENSE").length, sha256: hash(data.get("notices/LICENSE")) }]))),
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
  const review = (reviewType, reviewer) => ({
    schemaVersion: 1,
    reviewType,
    decision: "approved",
    summary: reviewType === "redistribution-legal"
      ? "Reviewed source and dependency distribution terms and recorded required notices."
      : "Traced the supplied library and resolved its components and notice origins.",
    evidenceReferences: [`evidence:${reviewType}:review-record`],
    unresolvedIssues: [],
    assertions: Object.fromEntries((reviewType === "redistribution-legal"
      ? ["sourceRedistributionRightsConfirmed", "dependencyRedistributionRightsConfirmed", "noticeInventoryComplete", "shippingObligationsRecorded"]
      : ["sourceIdentityConfirmed", "dependencyOriginsConfirmed", "sbomComponentsResolved", "noticeOriginsConfirmed"]
    ).map((field) => [field, true])),
    reviewer: { identity: reviewer, reference: `ticket:${reviewer}`, timestamp: "2026-09-29T01:02:03.000Z" },
    submitter: "release-coordinator",
    binding,
  });
  const redistributionPath = join(root, "reviews/redistribution-review.json");
  const supplierPath = join(root, "reviews/supplier-review.json");
  await writeFile(redistributionPath, `${JSON.stringify(review("redistribution-legal", "legal-reviewer"), null, 2)}\n`);
  await writeFile(supplierPath, `${JSON.stringify(review("supplier-provenance", "legal-reviewer"), null, 2)}\n`);
  return { temp, root, manifestPath, redistributionPath, supplierPath, manifest };
}

const approve = (f, outputPath = join(f.temp, "approved.json")) => approvePdfiumProductionCandidate({
  artifactRoot: f.root,
  manifestPath: f.manifestPath,
  redistributionReviewPath: f.redistributionPath,
  supplierReviewPath: f.supplierPath,
  outputPath,
});

test("maps CLI option names to the path fields used by the approval function", () => {
  assert.deepEqual(argumentsFrom([
    "--artifact-root", "/candidate",
    "--manifest", "/candidate/manifest.json",
    "--redistribution-review", "/candidate/redistribution.json",
    "--supplier-review", "/candidate/supplier.json",
    "--output", "/approved.json",
  ]), {
    artifactRoot: "/candidate",
    manifestPath: "/candidate/manifest.json",
    redistributionReviewPath: "/candidate/redistribution.json",
    supplierReviewPath: "/candidate/supplier.json",
    outputPath: "/approved.json",
  });
});

test("promotes only the exact candidate bound to two explicit reviews and preserves its artifact records", async (t) => {
  const f = await fixture();
  t.after(() => rm(f.temp, { recursive: true, force: true }));
  const approved = await approve(f);
  assert.equal(approved.productionApproved, true);
  assert.deepEqual(approved.artifacts, f.manifest.artifacts);
  assert.equal(approved.redistributionReview.path, "reviews/redistribution-review.json");
  assert.equal(approved.supplierReview.path, "reviews/supplier-review.json");
  validateNonMacProductionManifest(approved);
  assert.deepEqual(JSON.parse(await readFile(join(f.temp, "approved.json"), "utf8")), approved);
  await assert.rejects(approve(f), /EEXIST/);
});

test("rejects stale, non-approved, self-approved, mixed-target, extra-file, and unsafe candidates", async (t) => {
  const f = await fixture();
  t.after(() => rm(f.temp, { recursive: true, force: true }));
  const mutateReview = async (change) => {
    const review = JSON.parse(await readFile(f.redistributionPath, "utf8"));
    change(review);
    await writeFile(f.redistributionPath, JSON.stringify(review));
  };
  await mutateReview((review) => { review.binding.target = "aarch64-unknown-linux-gnu"; });
  await assert.rejects(approve(f), /binding target/);
  const fresh = await fixture();
  t.after(() => rm(fresh.temp, { recursive: true, force: true }));
  await writeFile(join(fresh.root, "unexpected.txt"), "extra");
  await assert.rejects(approve(fresh), /extra files/);
  const unsafe = await fixture();
  t.after(() => rm(unsafe.temp, { recursive: true, force: true }));
  unsafe.manifest.artifacts[0].library.path = "../escape.so";
  await writeFile(unsafe.manifestPath, `${JSON.stringify(unsafe.manifest, null, 2)}\n`);
  await assert.rejects(approve(unsafe), /safe relative path/);
});

test("rejects self approval and a candidate manifest containing multiple targets", async (t) => {
  const f = await fixture();
  t.after(() => rm(f.temp, { recursive: true, force: true }));
  const review = JSON.parse(await readFile(f.redistributionPath, "utf8"));
  review.reviewer.identity = review.submitter;
  await writeFile(f.redistributionPath, JSON.stringify(review));
  await assert.rejects(approve(f), /self-approved/);
  const g = await fixture();
  t.after(() => rm(g.temp, { recursive: true, force: true }));
  g.manifest.artifacts.push({ ...g.manifest.artifacts[0], target: "aarch64-unknown-linux-gnu" });
  await writeFile(g.manifestPath, `${JSON.stringify(g.manifest, null, 2)}\n`);
  await assert.rejects(approve(g), /exactly one target/);
});

test("requires substantive review content and every review-specific assertion", async (t) => {
  const cases = [
    ["bare approved assertion", (review) => { delete review.assertions; }],
    ["false review assertion", (review) => { review.assertions.noticeInventoryComplete = false; }],
    ["placeholder summary", (review) => { review.summary = "TODO"; }],
    ["placeholder evidence reference", (review) => { review.evidenceReferences = ["example evidence"]; }],
    ["unresolved issue", (review) => { review.unresolvedIssues = ["licence terms unresolved"]; }],
    ["missing unresolved issue declaration", (review) => { delete review.unresolvedIssues; }],
  ];
  for (const [label, change] of cases) {
    const f = await fixture();
    t.after(() => rm(f.temp, { recursive: true, force: true }));
    const review = JSON.parse(await readFile(f.redistributionPath, "utf8"));
    change(review);
    await writeFile(f.redistributionPath, JSON.stringify(review));
    await assert.rejects(approve(f), /review|assertion|unresolvedIssues/i, label);
  }
});

test("supplier-provenance review must affirm its own provenance assertions", async (t) => {
  const f = await fixture();
  t.after(() => rm(f.temp, { recursive: true, force: true }));
  const review = JSON.parse(await readFile(f.supplierPath, "utf8"));
  review.assertions.sourceIdentityConfirmed = false;
  await writeFile(f.supplierPath, JSON.stringify(review));
  await assert.rejects(approve(f), /supplier-provenance review must explicitly affirm/);
});

test("rejects source revisions and source patches outside the app-reviewed pins", async (t) => {
  const source = await fixture();
  t.after(() => rm(source.temp, { recursive: true, force: true }));
  source.manifest.source.revision = "c".repeat(40);
  await writeFile(source.manifestPath, `${JSON.stringify(source.manifest, null, 2)}\n`);
  await assert.rejects(approve(source), /source revision.*app-reviewed pin/);

  const patch = await fixture();
  t.after(() => rm(patch.temp, { recursive: true, force: true }));
  patch.manifest.build.sharedLibraryPatchSha256 = "d".repeat(64);
  await writeFile(patch.manifestPath, `${JSON.stringify(patch.manifest, null, 2)}\n`);
  await assert.rejects(approve(patch), /patch digest.*app-reviewed pin/);

  const dependencyPatch = await fixture();
  t.after(() => rm(dependencyPatch.temp, { recursive: true, force: true }));
  dependencyPatch.manifest.build.dependencyPolicyPatchSha256 = "e".repeat(64);
  await writeFile(dependencyPatch.manifestPath, `${JSON.stringify(dependencyPatch.manifest, null, 2)}\n`);
  await assert.rejects(approve(dependencyPatch), /patch digest.*app-reviewed pin/);
});
