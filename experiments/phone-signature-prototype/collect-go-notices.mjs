#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { lstat, readFile, readdir, writeFile } from "node:fs/promises";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

function fail(message) {
  throw new Error(message);
}

export function renderGoNotices({ goVersion, goLicense, modules }) {
  if (!/^go\d+\.\d+(?:\.\d+)?$/.test(goVersion ?? "") || !String(goLicense).trim()) {
    fail("Go toolchain version and licence are required");
  }
  if (!Array.isArray(modules) || modules.length === 0) fail("at least one Go module is required");
  const seen = new Set();
  const blocks = [`# Phone signature helper third-party notices\n\n## Go standard library (${goVersion})\n\n${String(goLicense).trim()}\n`];
  for (const module of [...modules].sort((left, right) => left.path.localeCompare(right.path))) {
    if (!/^[A-Za-z0-9._~!$&'()*+,;=:@%/-]+$/.test(module?.path ?? "") || seen.has(module.path)) fail("Go module paths must be unique and safe");
    seen.add(module.path);
    if (!Array.isArray(module.licenses) || module.licenses.length === 0) fail(`Go module ${module.path} has no licence file`);
    const version = module.version || "main-source-pin";
    blocks.push(`## ${module.path} (${version})\n`);
    for (const license of [...module.licenses].sort((left, right) => left.name.localeCompare(right.name))) {
      if (!/^(?:licen[cs]e|copying|notice)(?:[._-].*)?$/i.test(license?.name ?? "") || !String(license.text).trim()) fail(`Go module ${module.path} has an invalid licence record`);
      blocks.push(`### ${license.name}\n\n${String(license.text).trim()}\n`);
    }
  }
  return `${blocks.join("\n")}\n`;
}

function run(command, args, cwd) {
  const result = spawnSync(command, args, { cwd, encoding: "utf8", env: { ...process.env, GOTOOLCHAIN: "local" }, maxBuffer: 16 * 1024 * 1024 });
  if (result.error || result.status !== 0) fail(`${command} failed while collecting Go notices`);
  return result.stdout.trim();
}

async function licenseRecords(directory) {
  const records = [];
  for (const name of (await readdir(directory)).sort()) {
    if (!/^(?:licen[cs]e|copying|notice)(?:[._-].*)?$/i.test(name)) continue;
    const path = join(directory, name);
    const stat = await lstat(path);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.nlink !== 1) fail(`unsafe Go licence file: ${name}`);
    records.push({ name, text: await readFile(path, "utf8") });
  }
  return records;
}

export async function collectGoNotices({ moduleRoot, outputPath }) {
  const root = resolve(moduleRoot);
  const rows = run("go", ["list", "-deps", "-f", "{{with .Module}}{{.Path}}\t{{.Version}}\t{{.Dir}}{{end}}", "./cmd/bp-prototype"], root)
    .split(/\r?\n/).filter(Boolean);
  const modulesByPath = new Map();
  for (const row of rows) {
    const [path, version, directory, ...extra] = row.split("\t");
    if (!path || !directory || extra.length) fail("Go dependency inventory is malformed");
    const existing = modulesByPath.get(path);
    if (existing && (existing.version !== version || existing.directory !== directory)) fail(`Go module identity changed within the dependency graph: ${path}`);
    modulesByPath.set(path, { path, version, directory });
  }
  const modules = [];
  for (const module of modulesByPath.values()) {
    modules.push({ path: module.path, version: module.version, licenses: await licenseRecords(module.directory) });
  }
  const goRoot = run("go", ["env", "GOROOT"], root);
  const goVersion = run("go", ["env", "GOVERSION"], root);
  const goLicenseCandidates = [join(goRoot, "LICENSE"), join(dirname(goRoot), "LICENSE")];
  let goLicensePath;
  for (const candidate of goLicenseCandidates) {
    try {
      const stat = await lstat(candidate);
      if (stat.isFile() && !stat.isSymbolicLink() && stat.nlink === 1) {
        goLicensePath = candidate;
        break;
      }
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }
  }
  if (!goLicensePath) fail("Go standard-library licence is unsafe or missing");
  const goLicenseStat = await lstat(goLicensePath);
  if (!goLicenseStat.isFile() || goLicenseStat.isSymbolicLink() || goLicenseStat.nlink !== 1) fail("Go standard-library licence is unsafe or missing");
  const notices = renderGoNotices({ goVersion, goLicense: await readFile(goLicensePath, "utf8"), modules });
  await writeFile(resolve(outputPath), notices, { flag: "wx", mode: 0o644 });
  return { output: basename(outputPath), modules: modules.length };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.length !== 4 || args[0] !== "--module-root" || args[2] !== "--output") fail("usage: collect-go-notices.mjs --module-root DIR --output FILE");
  collectGoNotices({ moduleRoot: args[1], outputPath: args[3] })
    .then((result) => process.stdout.write(`${JSON.stringify(result)}\n`))
    .catch((error) => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
}
