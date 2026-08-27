import assert from "node:assert/strict";
import test from "node:test";

import { validateFoundationTruth } from "../scripts/foundation-truth.mjs";

const canonical = Object.freeze({
  componentRevision: "c27f5d5c8f70d534978c2f0739ad9e10d4e41eb4",
  zedRevision: "8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc",
  preparedDigest: "630e9459587c2fef7a791467de6e61351f067ec71ccba95f2ac00c95e967c7a3",
  componentLicenseSha256: "d1b0449e5478c574ba4f686c2656df7fe77d66821a61f8b6ed3378a58ed9a811",
  zedLicenseSha256: "752daf2fb234ca4a1fa372c073fe127f44b7b90fd2529ae44273a64f9d53da7a",
});

function acceptedInput() {
  return {
    canonical,
    documents: [
      {
        path: "FOUNDATION.md",
        contents: `Longbridge ${canonical.componentRevision}\nZed ${canonical.zedRevision}\nprepared ${canonical.preparedDigest}\nGPUI-CE gallery is historical evidence only.`,
      },
    ],
    sourcePreparationPolicy: {
      component: {
        revision: canonical.componentRevision,
        license: "Apache-2.0",
        licenseSha256: canonical.componentLicenseSha256,
      },
      zed: {
        revision: canonical.zedRevision,
        license: "Apache-2.0",
        licenseSha256: canonical.zedLicenseSha256,
      },
      prepared: { treeSha256: canonical.preparedDigest },
      sharedExperimentSources: [{ path: "../gpui-gallery/src/lib.rs", sha256: "a".repeat(64) }],
    },
    cargoManifest: `gpui = { git = "https://github.com/zed-industries/zed", rev = "${canonical.zedRevision}" }\ngpui-component = { path = ".prepared/gpui-component-c27f5d5c/crates/ui" }`,
    pdfiumManifest: { developmentOnly: true, productionApproved: false },
  };
}

test("the accepted foundation has one immutable GPUI Component source of truth", () => {
  assert.deepEqual(validateFoundationTruth(acceptedInput()), []);
});

test("moving or legacy candidate guidance is rejected", () => {
  const input = acceptedInput();
  input.documents = [{
    path: "FOUNDATION.md",
    contents: "Butter Paper accepts GPUI-CE as the foundation candidate and follows main.",
  }];
  assert.deepEqual(validateFoundationTruth(input), [
    "FOUNDATION.md does not name the reviewed Longbridge GPUI Component revision",
    "FOUNDATION.md does not name the exact Zed GPUI revision",
    "FOUNDATION.md does not name the prepared source digest",
    "FOUNDATION.md presents GPUI-CE as the current candidate",
    "FOUNDATION.md refers to a moving upstream branch",
  ]);
});

test("declarative GPUI-CE candidate wording and commented dependency pins are rejected", () => {
  const input = acceptedInput();
  input.documents = [{
    path: "README.md",
    contents: `${canonical.componentRevision}\n${canonical.zedRevision}\n${canonical.preparedDigest}\nGPUI-CE is the current application candidate.`,
  }];
  input.cargoManifest = `# gpui = { git = "https://github.com/zed-industries/zed", rev = "${canonical.zedRevision}" }\n# gpui-component = { path = ".prepared/gpui-component-c27f5d5c/crates/ui" }`;
  assert.deepEqual(validateFoundationTruth(input), [
    "README.md presents GPUI-CE as the current candidate",
    "compatibility manifest does not pin the exact Zed GPUI revision",
    "compatibility manifest does not use the prepared GPUI Component source",
  ]);
});

test("pin, receipt, and development-PDFium drift fail together", () => {
  const input = acceptedInput();
  input.sourcePreparationPolicy.component.revision = "0".repeat(40);
  input.sourcePreparationPolicy.zed.revision = "1".repeat(40);
  input.sourcePreparationPolicy.prepared.treeSha256 = "2".repeat(64);
  input.sourcePreparationPolicy.sharedExperimentSources = [{ path: "source.rs", sha256: "invalid" }];
  input.sourcePreparationPolicy.component.licenseSha256 = "3".repeat(64);
  input.sourcePreparationPolicy.zed.license = "unknown";
  input.cargoManifest = "gpui = { git = \"https://github.com/gpui-ce/gpui-ce\", branch = \"main\" }";
  input.pdfiumManifest = { developmentOnly: false, productionApproved: true };
  assert.deepEqual(validateFoundationTruth(input), [
    "source preparation component revision drifted",
    "source preparation Zed revision drifted",
    "prepared source digest drifted",
    "shared experiment source receipts are malformed",
    "Longbridge license provenance drifted",
    "Zed license provenance drifted",
    "compatibility manifest does not pin the exact Zed GPUI revision",
    "compatibility manifest does not use the prepared GPUI Component source",
    "compatibility manifest contains the forbidden GPUI-CE identity",
    "development PDFium is not marked development-only",
    "development PDFium is incorrectly approved for production",
  ]);
});
