#!/usr/bin/env node

import { fileURLToPath } from "node:url";
import { verifyUnsignedWindowsProduction } from "./sign-verify-windows-production.mjs";

function fail(message) { throw new Error(message); }

async function main(args) {
  const options = {};
  for (let index = 0; index < args.length; index += 2) {
    const key = args[index];
    if (!key?.startsWith("--") || options[key.slice(2)] !== undefined || args[index + 1] === undefined) fail("invalid or duplicate option");
    options[key.slice(2)] = args[index + 1];
  }
  const required = ["input", "output", "package-manifest", "verification-receipt", "architecture", "version", "revision"];
  if (Object.keys(options).some((key) => ![...required, "artifact-path"].includes(key)) || required.some((key) => options[key] === undefined)) {
    fail("usage: verify-windows-unsigned-production.mjs --input ZIP --output ZIP --package-manifest JSON --verification-receipt JSON --architecture x86_64|arm64 --version VERSION --revision GIT_SHA [--artifact-path RELATIVE_PATH]");
  }
  console.log(JSON.stringify(await verifyUnsignedWindowsProduction({
    inputArchive: options.input,
    outputArchive: options.output,
    packageManifestPath: options["package-manifest"],
    verificationReceiptPath: options["verification-receipt"],
    architecture: options.architecture,
    version: options.version,
    revision: options.revision,
    artifactPath: options["artifact-path"],
  }), null, 2));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((error) => { console.error(error.message); process.exitCode = 1; });
}
