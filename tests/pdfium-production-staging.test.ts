import { createHash } from "node:crypto";
import {
  link,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  symlink,
  writeFile,
} from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterEach, describe, expect, it } from "vitest";
import {
  stageProductionPdfium,
  validateProductionManifest,
} from "../experiments/gpui-migration/gpui-migration/scripts/stage-pdfium-production.mjs";

const roots: string[] = [];
const digest = (bytes: Uint8Array | string) =>
  createHash("sha256").update(bytes).digest("hex");

function macho(cpu: number, minimumSystemVersion = "13.0", fileType = 6) {
  const bytes = Buffer.alloc(56);
  bytes.writeUInt32LE(0xfeedfacf, 0);
  bytes.writeUInt32LE(cpu, 4);
  bytes.writeUInt32LE(fileType, 12);
  bytes.writeUInt32LE(1, 16);
  bytes.writeUInt32LE(24, 20);
  bytes.writeUInt32LE(0x32, 32);
  bytes.writeUInt32LE(24, 36);
  bytes.writeUInt32LE(1, 40);
  const [major, minor] = minimumSystemVersion.split(".").map(Number);
  bytes.writeUInt32LE((major << 16) | (minor << 8), 44);
  bytes.writeUInt32LE((26 << 16) | (5 << 8), 48);
  return bytes;
}

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "bp-pdfium-production-"));
  roots.push(root);
  const artifacts = join(root, "artifacts");
  await mkdir(join(artifacts, "notices/arm"), { recursive: true });
  await mkdir(join(artifacts, "notices/x64"), { recursive: true });
  const files = new Map<string, Buffer>([
    ["reviews/redistribution.txt", Buffer.from("approved redistribution")],
    ["reviews/supplier.txt", Buffer.from("approved supplier")],
    ["arm/libpdfium.dylib", macho(0x0100000c)],
    ["x64/libpdfium.dylib", macho(0x01000007)],
    ["arm/sbom.json", Buffer.from('{"bomFormat":"CycloneDX"}')],
    ["x64/sbom.json", Buffer.from('{"bomFormat":"CycloneDX"}')],
    ["arm/provenance.json", Buffer.from('{"builder":"reviewed"}')],
    ["x64/provenance.json", Buffer.from('{"builder":"reviewed"}')],
    [
      "arm/args.gn",
      Buffer.from("pdf_enable_v8=false pdf_enable_xfa=false is_debug=false"),
    ],
    [
      "x64/args.gn",
      Buffer.from("pdf_enable_v8=false pdf_enable_xfa=false is_debug=false"),
    ],
    ["notices/arm/LICENSE", Buffer.from("PDFium licence")],
    ["notices/arm/third_party/NOTICE", Buffer.from("third party")],
    ["notices/x64/LICENSE", Buffer.from("PDFium licence")],
    ["notices/x64/third_party/NOTICE", Buffer.from("third party")],
  ]);
  for (const [path, bytes] of files) {
    await mkdir(join(artifacts, path, ".."), { recursive: true });
    await writeFile(join(artifacts, path), bytes);
  }
  const record = (path: string) => {
    const bytes = files.get(path)!;
    return { path, bytes: bytes.length, sha256: digest(bytes) };
  };
  const manifest = {
    schemaVersion: 1,
    purpose: "production-distribution",
    productionApproved: true,
    wrapper: {
      package: "pdfium-render",
      version: "0.9.4",
      revision: "6cee8b9a3951832ac0ff62ce4c32800278001cb8",
      feature: "pdfium_7881",
    },
    source: {
      repository: "https://pdfium.googlesource.com/pdfium",
      revision: "91b9d569b34be4f38eed7b3c49b227356c3aadad",
    },
    build: {
      apiBuild: 7881,
      v8: false,
      xfa: false,
      sharedLibraryPatchSha256: "1e521b48561c51a63425baeec7c74c1edaf65956b6e6297d98aff57f2cc2ee40",
      dependencyPolicyPatchSha256: "54591df969f7a323c24f78e435340e8177081afc56dceaf1ec8102362a40601b",
      minimumSystemVersion: "13.0",
      toolchain: { clang: "22.0.0", sdk: "macosx27.0" },
    },
    redistributionReview: record("reviews/redistribution.txt"),
    supplierReview: record("reviews/supplier.txt"),
    artifacts: [
      ["aarch64-apple-darwin", "arm"],
      ["x86_64-apple-darwin", "x64"],
    ].map(([target, directory]) => ({
      target,
      library: record(`${directory}/libpdfium.dylib`),
      sbom: record(`${directory}/sbom.json`),
      provenance: record(`${directory}/provenance.json`),
      gnArgs: record(`${directory}/args.gn`),
      noticeRoot: `notices/${directory}`,
      notices: [
        { ...record(`notices/${directory}/LICENSE`), path: "LICENSE" },
        {
          ...record(`notices/${directory}/third_party/NOTICE`),
          path: "third_party/NOTICE",
        },
      ],
    })),
  };
  const manifestPath = join(root, "manifest.json");
  await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  return { root, artifacts, manifest, manifestPath };
}

afterEach(async () => {
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true, force: true })),
  );
});

describe("production PDFium staging", () => {
  it("rejects the reviewed development manifest and network-backed artifact entries", async () => {
    const development = JSON.parse(
      await readFile(
        new URL(
          "../experiments/gpui-migration/gpui-migration/pdfium-development-binaries.json",
          import.meta.url,
        ),
        "utf8",
      ),
    );
    expect(() => validateProductionManifest(development)).toThrow(
      "not explicitly approved",
    );
    const { manifest } = await fixture();
    manifest.artifacts[0] = {
      ...manifest.artifacts[0],
      url: "https://example.invalid/pdfium",
    } as never;
    expect(() => validateProductionManifest(manifest)).toThrow("local inputs");
  });

  it("stages an exact local macOS artifact, notice tree and deterministic receipt", async () => {
    const { root, artifacts, manifestPath } = await fixture();
    const first = join(root, "stage-arm");
    const second = join(root, "stage-arm-2");
    const firstReceipt = await stageProductionPdfium({
      manifestPath,
      artifactRoot: artifacts,
      target: "aarch64-apple-darwin",
      outputDirectory: first,
    });
    const secondReceipt = await stageProductionPdfium({
      manifestPath,
      artifactRoot: artifacts,
      target: "aarch64-apple-darwin",
      outputDirectory: second,
    });
    expect(secondReceipt).toEqual(firstReceipt);
    expect(firstReceipt.files.map(({ file }) => file)).toEqual([
      "Frameworks/libpdfium.dylib",
      "Resources/Licenses/PDFium/LICENSE",
      "Resources/Licenses/PDFium/third_party/NOTICE",
      "Resources/PDFium/gn-args.txt",
      "Resources/PDFium/provenance",
      "Resources/PDFium/redistribution-review",
      "Resources/PDFium/sbom",
      "Resources/PDFium/supplier-review",
    ]);
    expect(
      await readFile(join(first, "Resources/PDFium/receipt.json"), "utf8"),
    ).toBe(`${JSON.stringify(firstReceipt, null, 2)}\n`);
  });

  it("accepts an arm64-only release manifest and rejects targets it does not contain", async () => {
    const setup = await fixture();
    setup.manifest.artifacts = [setup.manifest.artifacts[0]];
    await writeFile(
      setup.manifestPath,
      `${JSON.stringify(setup.manifest, null, 2)}\n`,
    );

    expect(validateProductionManifest(setup.manifest)).toEqual(setup.manifest);
    await expect(
      stageProductionPdfium({
        manifestPath: setup.manifestPath,
        artifactRoot: setup.artifacts,
        target: "aarch64-apple-darwin",
        outputDirectory: join(setup.root, "arm-only"),
      }),
    ).resolves.toMatchObject({ target: "aarch64-apple-darwin" });
    await expect(
      stageProductionPdfium({
        manifestPath: setup.manifestPath,
        artifactRoot: setup.artifacts,
        target: "x86_64-apple-darwin",
        outputDirectory: join(setup.root, "missing-x64"),
      }),
    ).rejects.toThrow("target is not approved");
  });

  it("accepts the exact combined six-target approval manifest for macOS staging", async () => {
    const setup = await fixture();
    const nonMac = structuredClone(setup.manifest.artifacts[0]);
    nonMac.target = "aarch64-pc-windows-msvc";
    nonMac.library.path = "arm/pdfium.dll";
    setup.manifest.artifacts.push(nonMac);
    await writeFile(
      setup.manifestPath,
      `${JSON.stringify(setup.manifest, null, 2)}\n`,
    );

    expect(validateProductionManifest(setup.manifest)).toEqual(setup.manifest);
    await expect(
      stageProductionPdfium({
        manifestPath: setup.manifestPath,
        artifactRoot: setup.artifacts,
        target: "aarch64-apple-darwin",
        outputDirectory: join(setup.root, "combined-manifest"),
      }),
    ).resolves.toMatchObject({ target: "aarch64-apple-darwin" });
  });

  it("rejects empty, duplicate and unsupported production target sets", async () => {
    const empty = await fixture();
    empty.manifest.artifacts = [];
    expect(() => validateProductionManifest(empty.manifest)).toThrow(
      "nonempty set of unique approved targets",
    );

    const duplicate = await fixture();
    duplicate.manifest.artifacts.push(duplicate.manifest.artifacts[0]);
    expect(() => validateProductionManifest(duplicate.manifest)).toThrow(
      "nonempty set of unique approved targets",
    );

    const unsupported = await fixture();
    unsupported.manifest.artifacts[0].target = "riscv64-unknown-linux-gnu";
    expect(() => validateProductionManifest(unsupported.manifest)).toThrow(
      "unsupported production PDFium target",
    );
  });

  it("fails closed on architecture, bytes, missing or extra notices and pre-existing output", async () => {
    const { root, artifacts, manifest, manifestPath } = await fixture();
    const wrongArchitecture = macho(0x01000007);
    await writeFile(join(artifacts, "arm/libpdfium.dylib"), wrongArchitecture);
    manifest.artifacts[0].library = {
      path: "arm/libpdfium.dylib",
      bytes: wrongArchitecture.length,
      sha256: digest(wrongArchitecture),
    };
    await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
    await expect(
      stageProductionPdfium({
        manifestPath,
        artifactRoot: artifacts,
        target: "aarch64-apple-darwin",
        outputDirectory: join(root, "bad-arch"),
      }),
    ).rejects.toThrow("architecture does not match");

    const fresh = await fixture();
    await writeFile(join(fresh.artifacts, "notices/arm/EXTRA"), "extra");
    await expect(
      stageProductionPdfium({
        manifestPath: fresh.manifestPath,
        artifactRoot: fresh.artifacts,
        target: "aarch64-apple-darwin",
        outputDirectory: join(fresh.root, "extra-notice"),
      }),
    ).rejects.toThrow("missing or extra");
    const existingFixture = await fixture();
    await mkdir(join(existingFixture.root, "existing"));
    await expect(
      stageProductionPdfium({
        manifestPath: existingFixture.manifestPath,
        artifactRoot: existingFixture.artifacts,
        target: "aarch64-apple-darwin",
        outputDirectory: join(existingFixture.root, "existing"),
      }),
    ).rejects.toMatchObject({ code: "EEXIST" });
  });

  it("rejects a non-dylib, too-new minimum OS and noncanonical library name", async () => {
    const executable = await fixture();
    const wrongFileType = macho(0x0100000c, "13.0", 2);
    await writeFile(
      join(executable.artifacts, "arm/libpdfium.dylib"),
      wrongFileType,
    );
    executable.manifest.artifacts[0].library = {
      path: "arm/libpdfium.dylib",
      bytes: wrongFileType.length,
      sha256: digest(wrongFileType),
    };
    await writeFile(
      executable.manifestPath,
      `${JSON.stringify(executable.manifest, null, 2)}\n`,
    );
    await expect(
      stageProductionPdfium({
        manifestPath: executable.manifestPath,
        artifactRoot: executable.artifacts,
        target: "aarch64-apple-darwin",
        outputDirectory: join(executable.root, "wrong-file-type"),
      }),
    ).rejects.toThrow("wrong Mach-O file type");

    const tooNew = await fixture();
    const macos14 = macho(0x0100000c, "14.0");
    await writeFile(join(tooNew.artifacts, "arm/libpdfium.dylib"), macos14);
    tooNew.manifest.artifacts[0].library = {
      path: "arm/libpdfium.dylib",
      bytes: macos14.length,
      sha256: digest(macos14),
    };
    await writeFile(
      tooNew.manifestPath,
      `${JSON.stringify(tooNew.manifest, null, 2)}\n`,
    );
    await expect(
      stageProductionPdfium({
        manifestPath: tooNew.manifestPath,
        artifactRoot: tooNew.artifacts,
        target: "aarch64-apple-darwin",
        outputDirectory: join(tooNew.root, "too-new"),
      }),
    ).rejects.toThrow("requires macOS 14.0");

    const wrongName = await fixture();
    wrongName.manifest.artifacts[0].library.path = "arm/pdfium.dylib";
    expect(() => validateProductionManifest(wrongName.manifest)).toThrow(
      "must be named libpdfium.dylib",
    );
  });

  it("rejects symlinked and hard-linked reviewed inputs", async () => {
    const symlinkFixture = await fixture();
    await rm(join(symlinkFixture.artifacts, "arm/sbom.json"));
    await symlink(
      join(symlinkFixture.artifacts, "x64/sbom.json"),
      join(symlinkFixture.artifacts, "arm/sbom.json"),
    );
    await expect(
      stageProductionPdfium({
        manifestPath: symlinkFixture.manifestPath,
        artifactRoot: symlinkFixture.artifacts,
        target: "aarch64-apple-darwin",
        outputDirectory: join(symlinkFixture.root, "symlink"),
      }),
    ).rejects.toThrow("symlink");

    const hardLinkFixture = await fixture();
    await link(
      join(hardLinkFixture.artifacts, "notices/arm/LICENSE"),
      join(hardLinkFixture.artifacts, "notices/arm/LICENSE.alias"),
    );
    await expect(
      stageProductionPdfium({
        manifestPath: hardLinkFixture.manifestPath,
        artifactRoot: hardLinkFixture.artifacts,
        target: "aarch64-apple-darwin",
        outputDirectory: join(hardLinkFixture.root, "hard-link"),
      }),
    ).rejects.toThrow("hard link");
  });

  it("rejects tampered notices, unsafe paths and unreviewed GN feature policy", async () => {
    const tampered = await fixture();
    await writeFile(join(tampered.artifacts, "notices/arm/LICENSE"), "changed");
    await expect(
      stageProductionPdfium({
        manifestPath: tampered.manifestPath,
        artifactRoot: tampered.artifacts,
        target: "aarch64-apple-darwin",
        outputDirectory: join(tampered.root, "tampered"),
      }),
    ).rejects.toThrow("byte receipt");

    const unsafe = await fixture();
    unsafe.manifest.artifacts[0].library.path = "../libpdfium.dylib";
    expect(() => validateProductionManifest(unsafe.manifest)).toThrow(
      "safe relative path",
    );

    const enabledV8 = await fixture();
    const args = Buffer.from(
      "pdf_enable_v8=true pdf_enable_xfa=false is_debug=false",
    );
    await writeFile(join(enabledV8.artifacts, "arm/args.gn"), args);
    enabledV8.manifest.artifacts[0].gnArgs = {
      path: "arm/args.gn",
      bytes: args.length,
      sha256: digest(args),
    };
    await writeFile(
      enabledV8.manifestPath,
      `${JSON.stringify(enabledV8.manifest, null, 2)}\n`,
    );
    await expect(
      stageProductionPdfium({
        manifestPath: enabledV8.manifestPath,
        artifactRoot: enabledV8.artifacts,
        target: "aarch64-apple-darwin",
        outputDirectory: join(enabledV8.root, "v8-enabled"),
      }),
    ).rejects.toThrow("GN args");
  });
});
