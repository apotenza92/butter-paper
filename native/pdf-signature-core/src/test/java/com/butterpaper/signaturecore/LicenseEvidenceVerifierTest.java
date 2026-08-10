package com.butterpaper.signaturecore;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.util.zip.ZipEntry;
import java.util.zip.ZipOutputStream;

import static org.junit.jupiter.api.Assertions.*;

class LicenseEvidenceVerifierTest {
    @TempDir Path temporaryDirectory;
    private final ObjectMapper json = new ObjectMapper();

    @Test
    void reconcilesEveryComponentAndRetainsJarNotices() throws Exception {
        Fixture fixture = fixture();
        LicenseEvidenceVerifier.verify(
            fixture.sbom, fixture.policy, fixture.libraries, fixture.licences, fixture.sources, fixture.notices, fixture.report
        );

        JsonNode report = json.readTree(fixture.report.toFile());
        assertFalse(Files.readString(fixture.report).contains("\r"));
        assertEquals(1, report.get("componentCount").intValue());
        assertTrue(report.get("allComponentsHaveDeclaredAndHashedEvidence").booleanValue());
        assertEquals(1, report.get("correspondingSourceCount").intValue());
        assertTrue(report.get("allRequiredCorrespondingSourcesPresentAndHashed").booleanValue());
        assertEquals("source/upstream/example-source.tar.gz", report.at("/correspondingSources/0/evidenceFile").textValue());
        assertEquals(1, report.at("/correspondingSources/0/coveredComponents").size());
        assertFalse(report.get("legalApproval").booleanValue());
        assertEquals(1, report.at("/components/0/retainedJarNotices").size());
        assertTrue(Files.isRegularFile(fixture.notices.resolve("example-1.jar/META-INF_NOTICE.txt")));
    }

    @Test
    void failsClosedForMissingMappingOrChangedLicenceText() throws Exception {
        Fixture missing = fixture();
        Files.writeString(missing.policy, """
            {"schemaVersion":1,"licenses":{},"components":[]}
            """);
        assertThrows(IllegalArgumentException.class, () -> LicenseEvidenceVerifier.verify(
            missing.sbom, missing.policy, missing.libraries, missing.licences, missing.sources, missing.notices, missing.report
        ));

        Fixture changed = fixture();
        Files.writeString(changed.licences.resolve("Example.txt"), "changed");
        assertThrows(IllegalArgumentException.class, () -> LicenseEvidenceVerifier.verify(
            changed.sbom, changed.policy, changed.libraries, changed.licences, changed.sources, changed.notices, changed.report
        ));
    }

    @Test
    void failsClosedForMissingOrChangedCorrespondingSource() throws Exception {
        Fixture missing = fixture();
        Files.delete(missing.sources.resolve("example-source.tar.gz"));
        assertThrows(IllegalArgumentException.class, () -> LicenseEvidenceVerifier.verify(
            missing.sbom, missing.policy, missing.libraries, missing.licences, missing.sources, missing.notices, missing.report
        ));

        Fixture changed = fixture();
        Files.writeString(changed.sources.resolve("example-source.tar.gz"), "changed source");
        assertThrows(IllegalArgumentException.class, () -> LicenseEvidenceVerifier.verify(
            changed.sbom, changed.policy, changed.libraries, changed.licences, changed.sources, changed.notices, changed.report
        ));
    }

    @Test
    void failsClosedForMismatchedSbomLicenceOrCollidingJarNotices() throws Exception {
        Fixture mismatched = fixture();
        String policy = Files.readString(mismatched.policy)
            .replace("\"acceptedSbomLicenses\":[\"MIT\"]", "\"acceptedSbomLicenses\":[\"Apache-2.0\"]");
        Files.writeString(mismatched.policy, policy);
        assertThrows(IllegalArgumentException.class, () -> LicenseEvidenceVerifier.verify(
            mismatched.sbom, mismatched.policy, mismatched.libraries, mismatched.licences,
            mismatched.sources, mismatched.notices, mismatched.report
        ));

        Fixture colliding = fixture();
        Path jar = colliding.libraries.resolve("example-1.jar");
        try (ZipOutputStream zip = new ZipOutputStream(Files.newOutputStream(jar))) {
            zip.putNextEntry(new ZipEntry("META-INF/LICENSE/name"));
            zip.write("first".getBytes(java.nio.charset.StandardCharsets.UTF_8));
            zip.closeEntry();
            zip.putNextEntry(new ZipEntry("META-INF/LICENSE_name"));
            zip.write("second".getBytes(java.nio.charset.StandardCharsets.UTF_8));
            zip.closeEntry();
        }
        String changedSbom = Files.readString(colliding.sbom).replaceFirst(
            "[0-9a-f]{64}", sha256(jar)
        );
        Files.writeString(colliding.sbom, changedSbom);
        assertThrows(IllegalArgumentException.class, () -> LicenseEvidenceVerifier.verify(
            colliding.sbom, colliding.policy, colliding.libraries, colliding.licences,
            colliding.sources, colliding.notices, colliding.report
        ));
    }

    @Test
    void productionCliPolicyPinsTheExactReviewedDssSourceIdentity() throws Exception {
        Path policy = temporaryDirectory.resolve("production-policy.json");
        Files.writeString(policy, """
            {"correspondingSources":[{"archiveRoot":"dss-6.4/","bytes":137227450,"file":"dss-6.4-source.tar.gz","packagePath":"source/upstream/dss-6.4-source.tar.gz","requiredCoordinatePrefix":"eu.europa.ec.joinup.sd-dss:","resolvedCommit":"26a2e3338d8d4fe6c6281c2b53d13546fa64c9bf","sha256":"5f2421d6bf1c6073aa1e3c1ed4b44d2f058c6d751a4d89dbf326082860b224a4","sourceUrl":"https://github.com/esig/dss/archive/refs/tags/6.4.tar.gz","version":"6.4"}]}
            """);
        assertDoesNotThrow(() -> LicenseEvidenceVerifier.verifyProductionSourcePolicy(policy));

        Files.writeString(policy, Files.readString(policy).replace("137227450", "137227451"));
        assertThrows(IllegalArgumentException.class, () -> LicenseEvidenceVerifier.verifyProductionSourcePolicy(policy));
    }

    private Fixture fixture() throws Exception {
        Path libraries = Files.createDirectories(temporaryDirectory.resolve("lib-" + System.nanoTime()));
        Path licences = Files.createDirectories(temporaryDirectory.resolve("licenses-" + System.nanoTime()));
        Path sources = Files.createDirectories(temporaryDirectory.resolve("sources-" + System.nanoTime()));
        Path notices = temporaryDirectory.resolve("notices-" + System.nanoTime());
        Path report = temporaryDirectory.resolve("report-" + System.nanoTime() + ".json");
        Path jar = libraries.resolve("example-1.jar");
        try (ZipOutputStream zip = new ZipOutputStream(Files.newOutputStream(jar))) {
            zip.putNextEntry(new ZipEntry("META-INF/NOTICE.txt"));
            zip.write("upstream notice".getBytes(java.nio.charset.StandardCharsets.UTF_8));
            zip.closeEntry();
        }
        Path licence = licences.resolve("Example.txt");
        Files.writeString(licence, "example licence");
        Path source = sources.resolve("example-source.tar.gz");
        Files.writeString(source, "complete source");
        String jarHash = sha256(jar);
        String licenceHash = sha256(licence);
        String sourceHash = sha256(source);
        Path sbom = temporaryDirectory.resolve("sbom-" + System.nanoTime() + ".json");
        Files.writeString(sbom, """
            {"components":[{"group":"eu.europa.ec.joinup.sd-dss","name":"dss-example","version":"1","licenses":[{"license":{"id":"MIT"}}],"hashes":[{"alg":"SHA-256","content":"%s"}]}]}
            """.formatted(jarHash));
        Path policy = temporaryDirectory.resolve("policy-" + System.nanoTime() + ".json");
        Files.writeString(policy, """
            {"schemaVersion":1,"licenses":{"Example":{"acceptedSbomLicenses":["MIT"],"file":"Example.txt","sourceUrl":"https://example.invalid/LICENSE","sha256":"%s"}},"correspondingSources":[{"archiveRoot":"dss-1/","bytes":%d,"file":"example-source.tar.gz","packagePath":"source/upstream/example-source.tar.gz","requiredCoordinatePrefix":"eu.europa.ec.joinup.sd-dss:","resolvedCommit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","sha256":"%s","sourceUrl":"https://github.com/esig/dss/archive/refs/tags/1.tar.gz","version":"1"}],"components":[{"coordinate":"eu.europa.ec.joinup.sd-dss:dss-example:1","jar":"example-1.jar","license":"Example"}]}
            """.formatted(licenceHash, Files.size(source), sourceHash));
        return new Fixture(sbom, policy, libraries, licences, sources, notices, report);
    }

    private static String sha256(Path file) throws Exception {
        MessageDigest digest = MessageDigest.getInstance("SHA-256");
        try (InputStream input = Files.newInputStream(file)) {
            byte[] buffer = new byte[8_192];
            int read;
            while ((read = input.read(buffer)) != -1) digest.update(buffer, 0, read);
        }
        return HexFormat.of().formatHex(digest.digest());
    }

    private record Fixture(
        Path sbom,
        Path policy,
        Path libraries,
        Path licences,
        Path sources,
        Path notices,
        Path report
    ) {}
}
