import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { assemblePdfiumReviewKit, preparePdfiumReviewKit } from "../scripts/pdfium-review-kit.mjs";

const targets = [
  "aarch64-apple-darwin", "x86_64-apple-darwin",
  "aarch64-pc-windows-msvc", "x86_64-pc-windows-msvc",
  "aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu",
];
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const roles = {
  "redistribution-legal": ["sourceRedistributionRightsConfirmed", "dependencyRedistributionRightsConfirmed", "noticeInventoryComplete", "shippingObligationsRecorded"],
  "supplier-provenance": ["sourceIdentityConfirmed", "dependencyOriginsConfirmed", "sbomComponentsResolved", "noticeOriginsConfirmed"],
};

async function fixture() {
  const temp = await mkdtemp(join(tmpdir(), "bp-pdfium-review-kit-"));
  const fixtureTargets = targets;
  const inputs = [];
  for (const [index, target] of fixtureTargets.entries()) {
    const root = join(temp, `candidate-${index}`);
    await mkdir(join(root, "notices"), { recursive: true });
    await mkdir(join(root, "reviews"), { recursive: true });
    const lib = target.includes("windows") ? "pdfium.dll" : target.includes("apple") ? "libpdfium.dylib" : "libpdfium.so";
    const files = new Map([
      [lib, Buffer.from(`library-${target}`)],
      ["sbom.json", Buffer.from(`{"target":"${target}"}`)],
      ["provenance.json", Buffer.from(`{"target":"${target}"}`)],
      ["gn-args.txt", Buffer.from("pdf_enable_v8 = false\npdf_enable_xfa = false\nis_debug = false\n")],
      ["notices/LICENSE", Buffer.from(`Licence for ${target}\n`)],
      ["reviews/redistribution-review.txt", Buffer.from("UNREVIEWED PLACEHOLDER\n")],
      ["reviews/supplier-review.txt", Buffer.from("UNREVIEWED PLACEHOLDER\n")],
    ]);
    for (const [path, bytes] of files) {
      const absolute = join(root, path);
      await mkdir(join(absolute, ".."), { recursive: true });
      await writeFile(absolute, bytes);
    }
    const rec = (path) => ({ path, bytes: files.get(path).length, sha256: hash(files.get(path)) });
    const manifest = {
      schemaVersion: 1, purpose: "production-distribution", productionApproved: false,
      wrapper: { package: "pdfium-render", version: "0.9.4", revision: "6cee8b9a3951832ac0ff62ce4c32800278001cb8", feature: "pdfium_7881" },
      source: { repository: "https://pdfium.googlesource.com/pdfium", revision: "91b9d569b34be4f38eed7b3c49b227356c3aadad" },
      build: { apiBuild: 7881, v8: false, xfa: false, debug: false, sharedLibraryPatchSha256: "1e521b48561c51a63425baeec7c74c1edaf65956b6e6297d98aff57f2cc2ee40", dependencyPolicyPatchSha256: "54591df969f7a323c24f78e435340e8177081afc56dceaf1ec8102362a40601b" },
      redistributionReview: rec("reviews/redistribution-review.txt"), supplierReview: rec("reviews/supplier-review.txt"),
      artifacts: [{ target, library: rec(lib), sbom: rec("sbom.json"), provenance: rec("provenance.json"), gnArgs: rec("gn-args.txt"), noticeRoot: "notices", notices: [{ ...rec("notices/LICENSE"), path: "LICENSE" }] }],
    };
    await writeFile(join(root, "production-pdfium-candidate.json"), `${JSON.stringify(manifest, null, 2)}\n`);
    inputs.push({ target, artifactRoot: root });
  }
  return { temp, inputs, targets: fixtureTargets, output: join(temp, "packet") };
}

async function prepare(f) {
  return preparePdfiumReviewKit({ inputs: f.inputs, outputDirectory: f.output, sourceRunId: "123", sourceRunAttempt: "1" });
}

async function completeReviews(packet, target) {
  const base = join(packet, target, "reviews");
  for (const [name, type] of [["redistributionReview", "redistribution-legal"], ["supplierReview", "supplier-provenance"]]) {
    const path = join(base, `${name}.json`);
    const review = JSON.parse(await readFile(path, "utf8"));
    review.decision = "approved";
    review.summary = `Reviewed the ${type} evidence and recorded the outcome for ${target}.`;
    review.evidenceReferences = [`record:${target}:${type}`];
    review.assertions = Object.fromEntries(roles[type].map((field) => [field, true]));
    review.reviewer = { identity: `reviewer-${type}`, reference: `record:${target}:${type}`, timestamp: "2026-09-29T01:02:03.000Z" };
    review.submitter = "release-coordinator";
    await writeFile(path, `${JSON.stringify(review, null, 2)}\n`);
  }
}

test("prepares a deterministic six-target packet with pending, bound templates and no approvals", async (t) => {
  const f = await fixture(); t.after(() => rm(f.temp, { recursive: true, force: true }));
  const packet = await prepare(f);
  assert.equal(packet.approvalStatus, "pending");
  assert.deepEqual(packet.targets.map(({ target }) => target), f.targets);
  for (const target of f.targets) {
    const candidate = JSON.parse(await readFile(join(f.output, target, "candidate/production-pdfium-candidate.json"), "utf8"));
    assert.equal(candidate.productionApproved, false);
    const legal = JSON.parse(await readFile(join(f.output, target, "reviews/redistributionReview.json"), "utf8"));
    const supplier = JSON.parse(await readFile(join(f.output, target, "reviews/supplierReview.json"), "utf8"));
    assert.equal(legal.decision, "pending");
    assert.equal(supplier.decision, "pending");
    assert.equal(legal.binding.target, target);
    assert.equal(legal.binding.candidateManifestSha256, packet.targets.find((item) => item.target === target).candidateManifestSha256);
    assert.equal(Object.values(legal.assertions).every((value) => value === false), true);
  }
  assert.equal(await readFile(join(f.output, "README.md"), "utf8").then((text) => text.includes("does not approve")), true);
});

test("assembles completed reviews into the exact six-target workflow payload and bounded base64", async (t) => {
  const f = await fixture(); t.after(() => rm(f.temp, { recursive: true, force: true }));
  await prepare(f);
  for (const target of f.targets) await completeReviews(f.output, target);
  const jsonPath = join(f.temp, "bundle.json"); const base64Path = join(f.temp, "bundle.base64");
  const result = await assemblePdfiumReviewKit({ packetDirectory: f.output, outputPath: jsonPath, base64OutputPath: base64Path });
  const bundle = JSON.parse(await readFile(jsonPath, "utf8"));
  assert.deepEqual(Object.keys(bundle), f.targets);
  for (const target of f.targets) assert.deepEqual(Object.keys(bundle[target]), ["redistributionReview", "supplierReview"]);
  assert.deepEqual(JSON.parse(Buffer.from((await readFile(base64Path, "utf8")).trim(), "base64").toString("utf8")), bundle);
  assert.equal(result.base64Length <= 60000, true);
  assert.equal(JSON.parse(await readFile(join(f.output, "review-kit.json"), "utf8")).approvalStatus, "pending");
});

test("rejects missing/duplicate targets, tampered candidate evidence, stale bindings, and incomplete human reviews", async (t) => {
  const badInputs = await fixture(); t.after(() => rm(badInputs.temp, { recursive: true, force: true }));
  await assert.rejects(preparePdfiumReviewKit({ inputs: badInputs.inputs.slice(1), outputDirectory: badInputs.output }), /six mandatory/);
  await assert.rejects(preparePdfiumReviewKit({ inputs: [...badInputs.inputs, badInputs.inputs[0]], outputDirectory: badInputs.output }), /six mandatory/);

  const tampered = await fixture(); t.after(() => rm(tampered.temp, { recursive: true, force: true }));
  await writeFile(join(tampered.inputs[0].artifactRoot, "libpdfium.dylib"), "tampered");
  await assert.rejects(prepare(tampered), /does not match candidate manifest digest/);

  const pending = await fixture(); t.after(() => rm(pending.temp, { recursive: true, force: true }));
  await prepare(pending);
  for (const target of pending.targets) await completeReviews(pending.output, target);
  const reviewPath = join(pending.output, targets[0], "reviews/redistributionReview.json");
  const review = JSON.parse(await readFile(reviewPath, "utf8"));
  review.binding.target = "wrong-target";
  await writeFile(reviewPath, JSON.stringify(review));
  await assert.rejects(assemblePdfiumReviewKit({ packetDirectory: pending.output, outputPath: join(pending.temp, "x.json"), base64OutputPath: join(pending.temp, "x.b64") }), /binding target/);
});

test("rejects extra directories and symlinked review roots, and keeps assemble outputs outside packet", async (t) => {
  const extra = await fixture(); t.after(() => rm(extra.temp, { recursive: true, force: true }));
  await mkdir(join(extra.inputs[0].artifactRoot, "empty-extra"));
  await assert.rejects(prepare(extra), /directory inventory mismatch/);

  const linked = await fixture(); t.after(() => rm(linked.temp, { recursive: true, force: true }));
  const targetRoot = linked.inputs[0].artifactRoot;
  await rm(join(targetRoot, "reviews"), { recursive: true });
  await symlink(join(targetRoot, "notices"), join(targetRoot, "reviews"));
  await assert.rejects(prepare(linked), /symlink/);

  const output = await fixture(); t.after(() => rm(output.temp, { recursive: true, force: true }));
  await prepare(output);
  for (const target of output.targets) await completeReviews(output.output, target);
  await assert.rejects(assemblePdfiumReviewKit({
    packetDirectory: output.output,
    outputPath: join(output.output, "bundle.json"),
    base64OutputPath: join(output.temp, "bundle.base64"),
  }), /outside the review packet/);
});

test("requires all six targets in canonical order", async (t) => {
  const six = await fixture(); t.after(() => rm(six.temp, { recursive: true, force: true }));
  const reordered = [...six.inputs].reverse();
  await assert.rejects(preparePdfiumReviewKit({ inputs: reordered, outputDirectory: six.output }), /canonical order/);
  const substitute = six.inputs.filter(({ target }) => target !== "aarch64-pc-windows-msvc");
  await assert.rejects(preparePdfiumReviewKit({ inputs: substitute, outputDirectory: six.output }), /six mandatory/);
});

test("validates packet metadata and exact review-directory inventory, and rolls back only its own first output", async (t) => {
  const f = await fixture(); t.after(() => rm(f.temp, { recursive: true, force: true }));
  await prepare(f);
  for (const target of f.targets) await completeReviews(f.output, target);
  const reviewDirectory = join(f.output, f.targets[0], "reviews");
  await writeFile(join(reviewDirectory, "extra.json"), "{}");
  await assert.rejects(assemblePdfiumReviewKit({ packetDirectory: f.output, outputPath: join(f.temp, "out.json"), base64OutputPath: join(f.temp, "out.b64") }), /exactly the two review templates/);
  await rm(join(reviewDirectory, "extra.json"));
  await mkdir(join(reviewDirectory, "empty-extra"));
  await assert.rejects(assemblePdfiumReviewKit({ packetDirectory: f.output, outputPath: join(f.temp, "out.json"), base64OutputPath: join(f.temp, "out.b64") }), /exactly the two review templates/);
  await rm(join(reviewDirectory, "empty-extra"), { recursive: true });

  const kitPath = join(f.output, "review-kit.json");
  const kitCopy = join(f.temp, "review-kit-copy.json");
  await writeFile(kitCopy, await readFile(kitPath));
  await rm(kitPath);
  await symlink(kitCopy, kitPath);
  await assert.rejects(assemblePdfiumReviewKit({ packetDirectory: f.output, outputPath: join(f.temp, "link.json"), base64OutputPath: join(f.temp, "link.b64") }), /symlink/);
  await rm(kitPath);
  await writeFile(kitPath, await readFile(kitCopy));

  const existingBase64 = join(f.temp, "existing.b64");
  await writeFile(existingBase64, "preserve me");
  const firstOutput = join(f.temp, "should-be-removed.json");
  await assert.rejects(assemblePdfiumReviewKit({ packetDirectory: f.output, outputPath: firstOutput, base64OutputPath: existingBase64 }), /EEXIST/);
  await assert.rejects(readFile(firstOutput), { code: "ENOENT" });
  assert.equal(await readFile(existingBase64, "utf8"), "preserve me");
});
