import { readFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const probeDirectory = resolve(scriptDirectory, "..");
const migrationDirectory = resolve(probeDirectory, "..");

export const CANONICAL_FOUNDATION = Object.freeze({
  componentRevision: "c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4",
  zedRevision: "8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc",
  preparedDigest: "630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3",
  componentLicenseSha256: "d1b0449e5478c574ba4f686c2656df7fe77d66821a61f8b6ed3378a58ed9a811",
  zedLicenseSha256: "752daf2fb234ca4a1fa372c073fe127f44b7b90fd2529ae44273a64f9d53da7a",
});

export const ACTIVE_FOUNDATION_DOCUMENTS = Object.freeze([
  Object.freeze({
    path: "gpui-gallery/FOUNDATION.md",
    requiredTruths: Object.freeze(["componentRevision", "zedRevision", "preparedDigest"]),
  }),
  Object.freeze({
    path: "gpui-component-compat/README.md",
    requiredTruths: Object.freeze(["componentRevision", "zedRevision", "preparedDigest"]),
  }),
  Object.freeze({ path: "README.md", requiredTruths: Object.freeze([]) }),
]);

function documentErrors(document, canonical) {
  const errors = [];
  const requiredTruths = document.requiredTruths
    ?? ["componentRevision", "zedRevision", "preparedDigest"];
  if (requiredTruths.includes("componentRevision")
      && !document.contents.includes(canonical.componentRevision)) {
    errors.push(`${document.path} does not name the reviewed Longbridge GPUI Component revision`);
  }
  if (requiredTruths.includes("zedRevision")
      && !document.contents.includes(canonical.zedRevision)) {
    errors.push(`${document.path} does not name the exact Zed GPUI revision`);
  }
  if (requiredTruths.includes("preparedDigest")
      && !document.contents.includes(canonical.preparedDigest)) {
    errors.push(`${document.path} does not name the prepared source digest`);
  }
  if (/(?:accepts?\s+(?:the\s+)?(?:community-maintained\s+)?GPUI-CE\b[^.\n]*(?:candidate|foundation)|\bGPUI-CE\b[^.\n]*(?:is|remains)\s+(?:the\s+)?(?:current|accepted|exact|application|foundation)[^.\n]*candidate|\b(?:current|accepted|exact)\s+GPUI-CE\s+candidate)/i.test(document.contents)) {
    errors.push(`${document.path} presents GPUI-CE as the current candidate`);
  }
  const movingBranchStatement = document.contents.split(/\n|\./).some((statement) =>
    /\b(?:follow|follows|following|track|tracks|tracking|use|uses|using)\b[^\n]*\bmain\b/i.test(statement)
      && !/\b(?:do not|don't|never|must not|cannot|can't)\b/i.test(statement));
  if (movingBranchStatement) {
    errors.push(`${document.path} refers to a moving upstream branch`);
  }
  return errors;
}

export function validateFoundationTruth(input) {
  const {
    canonical,
    documents,
    sourcePreparationPolicy,
    cargoManifest,
    pdfiumManifest,
  } = input;
  const errors = documents.flatMap((document) => documentErrors(document, canonical));

  if (sourcePreparationPolicy.component?.revision !== canonical.componentRevision) {
    errors.push("source preparation component revision drifted");
  }
  if (sourcePreparationPolicy.zed?.revision !== canonical.zedRevision) {
    errors.push("source preparation Zed revision drifted");
  }
  if (sourcePreparationPolicy.prepared?.treeSha256 !== canonical.preparedDigest) {
    errors.push("prepared source digest drifted");
  }
  if (!Array.isArray(sourcePreparationPolicy.sharedExperimentSources)
      || sourcePreparationPolicy.sharedExperimentSources.length === 0) {
    errors.push("shared experiment source receipts are empty");
  } else if (sourcePreparationPolicy.sharedExperimentSources.some((receipt) =>
    typeof receipt?.path !== "string"
      || receipt.path.length === 0
      || !/^[0-9a-f]{64}$/.test(receipt.sha256 ?? ""))) {
    errors.push("shared experiment source receipts are malformed");
  }
  if (sourcePreparationPolicy.component?.license !== "Apache-2.0"
      || sourcePreparationPolicy.component?.licenseSha256 !== canonical.componentLicenseSha256) {
    errors.push("Longbridge license provenance drifted");
  }
  if (sourcePreparationPolicy.zed?.license !== "Apache-2.0"
      || sourcePreparationPolicy.zed?.licenseSha256 !== canonical.zedLicenseSha256) {
    errors.push("Zed license provenance drifted");
  }

  const activeManifestLines = cargoManifest
    .split("\n")
    .map((line) => line.replace(/\s+#.*$/, ""))
    .filter((line) => line.trim().length > 0);
  const zedDependencyLines = activeManifestLines.filter((line) =>
    /^\s*(?:gpui|gpui_platform)\s*=/.test(line));
  const exactZedPin = `rev = "${canonical.zedRevision}"`;
  if (zedDependencyLines.length === 0
      || zedDependencyLines.some((line) =>
        !line.includes("https://github.com/zed-industries/zed")
          || !line.includes(exactZedPin))) {
    errors.push("compatibility manifest does not pin the exact Zed GPUI revision");
  }
  const componentDependency = activeManifestLines.find((line) =>
    /^\s*gpui-component\s*=/.test(line));
  if (!componentDependency?.includes(".prepared/gpui-component-c27f5d5c/crates/ui")) {
    errors.push("compatibility manifest does not use the prepared GPUI Component source");
  }
  if (activeManifestLines.some((line) => /github\.com\/gpui-ce\/gpui-ce/i.test(line))) {
    errors.push("compatibility manifest contains the forbidden GPUI-CE identity");
  }

  const developmentOnly = pdfiumManifest.developmentOnly === true
    || (typeof pdfiumManifest.purpose === "string"
      && pdfiumManifest.purpose.includes("development-only"));
  if (!developmentOnly) {
    errors.push("development PDFium is not marked development-only");
  }
  if (pdfiumManifest.productionApproved !== false) {
    errors.push("development PDFium is incorrectly approved for production");
  }
  return errors;
}

async function readJson(path) {
  return JSON.parse(await readFile(path, "utf8"));
}

export async function validateCurrentFoundation() {
  const documents = await Promise.all(ACTIVE_FOUNDATION_DOCUMENTS.map(async (document) => ({
    ...document,
    contents: await readFile(join(migrationDirectory, document.path), "utf8"),
  })));

  return validateFoundationTruth({
    canonical: CANONICAL_FOUNDATION,
    documents,
    sourcePreparationPolicy: await readJson(join(probeDirectory, "source-preparation-policy.json")),
    cargoManifest: await readFile(join(probeDirectory, "Cargo.toml"), "utf8"),
    pdfiumManifest: await readJson(join(migrationDirectory, "gpui-gallery", "pdfium-development-binaries.json")),
  });
}

async function main() {
  const errors = await validateCurrentFoundation();
  if (errors.length > 0) {
    process.stderr.write(`${JSON.stringify({ status: "rejected", errors }, null, 2)}\n`);
    process.exitCode = 1;
    return;
  }
  process.stdout.write(`${JSON.stringify({
    status: "verified",
    ...CANONICAL_FOUNDATION,
    pdfiumApproval: "development-only",
  }, null, 2)}\n`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
