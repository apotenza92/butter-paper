package com.butterpaper.signaturecore;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import eu.europa.esig.dss.model.FileDocument;
import eu.europa.esig.dss.model.x509.CertificateToken;
import eu.europa.esig.dss.pades.validation.PDFDocumentValidator;
import eu.europa.esig.dss.spi.signature.AdvancedSignature;
import eu.europa.esig.dss.spi.validation.CommonCertificateVerifier;
import eu.europa.esig.dss.crl.ICRLUtils;
import org.apache.pdfbox.Loader;
import org.apache.pdfbox.cos.COSArray;
import org.apache.pdfbox.cos.COSDictionary;
import org.apache.pdfbox.cos.COSName;
import org.apache.pdfbox.cos.COSStream;
import org.apache.pdfbox.cos.COSString;
import org.apache.pdfbox.pdmodel.PDDocument;
import org.apache.pdfbox.pdmodel.PDPage;
import org.apache.pdfbox.pdmodel.interactive.digitalsignature.PDSignature;
import org.apache.pdfbox.pdmodel.interactive.form.PDAcroForm;
import org.apache.pdfbox.pdmodel.interactive.form.PDSignatureField;
import org.bouncycastle.asn1.x500.X500Name;
import org.bouncycastle.asn1.x509.ExtendedKeyUsage;
import org.bouncycastle.asn1.x509.Extension;
import org.bouncycastle.asn1.x509.KeyPurposeId;
import org.bouncycastle.cert.jcajce.JcaCertStore;
import org.bouncycastle.cert.jcajce.JcaX509CertificateConverter;
import org.bouncycastle.cert.jcajce.JcaX509v3CertificateBuilder;
import org.bouncycastle.cms.CMSProcessableByteArray;
import org.bouncycastle.cms.CMSSignedDataGenerator;
import org.bouncycastle.cms.jcajce.JcaSignerInfoGeneratorBuilder;
import org.bouncycastle.jce.provider.BouncyCastleProvider;
import org.bouncycastle.operator.ContentSigner;
import org.bouncycastle.operator.jcajce.JcaContentSignerBuilder;
import org.bouncycastle.operator.jcajce.JcaDigestCalculatorProviderBuilder;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.math.BigInteger;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.KeyPair;
import java.security.KeyPairGenerator;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.security.Security;
import java.security.cert.X509Certificate;
import java.security.spec.ECGenParameterSpec;
import java.time.Instant;
import java.util.Calendar;
import java.util.Base64;
import java.util.Arrays;
import java.util.Date;
import java.util.HexFormat;
import java.util.List;
import java.util.Map;
import java.util.TimeZone;
import java.util.ServiceLoader;

import static org.junit.jupiter.api.Assertions.*;

class ValidationServiceTest {
    private static final ObjectMapper JSON = new ObjectMapper();
    private static final Instant SIGNING_INSTANT = Instant.parse("2026-08-05T00:00:00Z");

    @TempDir Path temporaryDirectory;

    @Test
    void offlineVerifierHasNoNetworkOrTrustSources() {
        CommonCertificateVerifier verifier = ValidationService.newOfflineVerifier();
        assertNull(verifier.getAIASource());
        assertNull(verifier.getOcspSource());
        assertNull(verifier.getCrlSource());
        assertTrue(verifier.getTrustedCertSources().getCertificates().isEmpty());
        assertFalse(verifier.isCheckRevocationForUntrustedChains());
        assertFalse(verifier.isRevocationFallback());
    }

    @Test
    void runtimeProvidesExactlyOnePinnedCrlParserImplementation() {
        List<ICRLUtils> providers = ServiceLoader.load(ICRLUtils.class).stream()
            .map(ServiceLoader.Provider::get)
            .toList();

        assertEquals(1, providers.size());
        assertEquals(
            "eu.europa.esig.dss.crl.x509.impl.CRLUtilsX509CRLImpl",
            providers.getFirst().getClass().getName()
        );
    }

    @Test
    void duplicateFullyQualifiedSignatureFieldNamesAreDetectedDeterministically() throws Exception {
        try (PDDocument document = new PDDocument()) {
            PDAcroForm form = new PDAcroForm(document);
            PDSignatureField first = new PDSignatureField(form);
            first.setPartialName("Signature1");
            PDSignatureField duplicate = new PDSignatureField(form);
            duplicate.setPartialName("Signature1");
            PDSignatureField distinct = new PDSignatureField(form);
            distinct.setPartialName("Signature2");

            assertEquals(
                java.util.Set.of("Signature1"),
                ValidationService.duplicateSignatureFieldNames(List.of(first, duplicate, distinct))
            );
        }
    }

    @Test
    void validatesUnsignedPdfWithoutChangingTheSource() throws Exception {
        Path input = temporaryDirectory.resolve("unsigned.pdf").toAbsolutePath();
        try (PDDocument document = new PDDocument()) {
            document.addPage(new PDPage());
            document.save(input.toFile());
        }
        byte[] before = Files.readAllBytes(input);

        JsonNode report = report(new ValidationService().validate(input.toString()));

        assertArrayEquals(before, Files.readAllBytes(input));
        assertEquals(sha256(before), report.get("inputSha256").textValue());
        assertEquals("unsigned", report.at("/inventory/presence").textValue());
        assertTrue(report.at("/signatures").isEmpty());
        assertEquals("absent", report.at("/inventory/validationEvidence/structureStatus").textValue());
        assertFalse(report.at("/inventory/validationEvidence/dssPresent").booleanValue());
        assertFalse(report.at("/inventory/validationEvidence/vriPresent").booleanValue());
        assertTrue(report.at("/inventory/validationEvidence/inventoryComplete").booleanValue());
        assertFalse(report.at("/trust/onlineSourcesUsed").booleanValue());
        assertTrue(report.findValues("valid").isEmpty());
    }

    @Test
    void fixedReferenceClockIsReportedAndDrivesCurrentCertificateStatus() throws Exception {
        Path signed = createSignedPdf(temporaryDirectory.resolve("fixed-clock.pdf").toAbsolutePath());
        Instant reference = Instant.parse("2024-08-05T00:00:00Z");
        ValidationService.ValidationClock clock = ValidationService.ValidationClock.fixedReference(
            reference,
            Instant.parse("2026-08-06T00:00:00Z")
        );

        JsonNode report = report(new ValidationService().validate(
            signed.toString(),
            ExactTrustPolicy.empty(),
            clock
        ));

        assertEquals(reference.toString(), report.get("validationTime").textValue());
        assertEquals(
            ValidationService.CALLER_SUPPLIED_FIXED_REFERENCE,
            report.get("validationTimeProvenance").textValue()
        );
        assertEquals("not-yet-valid", report.at("/signatures/0/certificateStatus").textValue());
        assertEquals("offline", report.get("validationMode").textValue());
        assertFalse(report.at("/trust/onlineSourcesUsed").booleanValue());
    }

    @Test
    void fixedReferenceClockRejectsUnsafeBounds() {
        Instant observed = Instant.parse("2026-08-06T00:00:00Z");
        assertThrows(IllegalArgumentException.class, () -> ValidationService.ValidationClock.fixedReference(
            Instant.parse("1899-12-31T23:59:59Z"),
            observed
        ));
        assertThrows(IllegalArgumentException.class, () -> ValidationService.ValidationClock.fixedReference(
            observed.plusNanos(1),
            observed
        ));
    }

    @Test
    void validatesLocallySignedPdfAsCryptographicallyIntactButUntrustedOffline() throws Exception {
        Path signed = createSignedPdf(temporaryDirectory.resolve("self-signed.pdf").toAbsolutePath());
        byte[] before = Files.readAllBytes(signed);

        try (PDDocument pdf = Loader.loadPDF(signed.toFile())) {
            PDFDocumentValidator validator = new PDFDocumentValidator(new FileDocument(signed.toFile()));
            validator.setCertificateVerifier(ValidationService.newOfflineVerifier());
            AdvancedSignature advanced = validator.getSignatures().getFirst();
            byte[] extracted = ValidationService.cmsSignatureValue(
                pdf.getSignatureDictionaries().getFirst().getContents()
            );
            assertArrayEquals(
                advanced.getSignatureValue(),
                extracted,
                "PDFBox /Contents and DSS signature-value identity must agree"
            );
        }

        JsonNode report = report(new ValidationService().validate(signed.toString()));

        assertArrayEquals(before, Files.readAllBytes(signed));
        assertEquals(sha256(before), report.get("inputSha256").textValue());
        assertEquals("signed", report.at("/inventory/presence").textValue());
        assertEquals(1, report.at("/signatures").size());
        assertEquals("intact", report.at("/signatures/0/integrity").textValue());
        assertEquals("untrusted", report.at("/signatures/0/identityTrust").textValue());
        assertEquals("offline", report.at("/signatures/0/certificateStatus").textValue());
        assertEquals("claimed-only", report.at("/signatures/0/signingTime").textValue());
        assertEquals("none", report.at("/signatures/0/evidenceFreshness/source").textValue());
        assertTrue(report.at("/signatures/0/timestamps/0/tsaClaim").isNull());
        assertEquals("whole-relevant-revision", report.at("/signatures/0/coverage").textValue());
        assertEquals(false, report.at("/trust/onlineSourcesUsed").booleanValue());
        assertTrue(report.findValues("valid").isEmpty());
    }

    @Test
    void ordinaryEcdsaSigningCertificateIsNotReportedAsFreshnessEvidence() throws Exception {
        Path unsigned = temporaryDirectory.resolve("ecdsa-unsigned.pdf").toAbsolutePath();
        try (PDDocument document = new PDDocument()) {
            document.addPage(new PDPage());
            document.save(unsigned.toFile());
        }
        Path signed = temporaryDirectory.resolve("ecdsa-signed.pdf").toAbsolutePath();
        appendSignatureWithAlgorithm(
            unsigned,
            signed,
            "butter-paper-phase-1-ecdsa-fixture",
            "Butter Paper ECDSA Test",
            BigInteger.valueOf(20260809),
            SIGNING_INSTANT,
            "EC",
            "SHA256withECDSA"
        );

        JsonNode report = report(new ValidationService().validate(signed.toString()));

        assertEquals("intact", report.at("/signatures/0/integrity").textValue());
        assertEquals("none", report.at("/signatures/0/evidenceFreshness/source").textValue());
        assertTrue(report.at("/signatures/0/evidenceFreshness/producedAt").isNull());
        assertTrue(report.at("/signatures/0/evidenceFreshness/nextUpdateAt").isNull());
    }

    @Test
    void certificateInventoryIsOrderedByFingerprintIndependentlyOfDssCollectionOrder() throws Exception {
        CertificateToken first = certificateToken(
            "butter-paper-certificate-order-first",
            BigInteger.valueOf(20260812)
        );
        CertificateToken second = certificateToken(
            "butter-paper-certificate-order-second",
            BigInteger.valueOf(20260813)
        );

        List<Map<String, Object>> forward = ValidationService.certificateInventory(
            List.of(first, second),
            first
        );
        List<Map<String, Object>> reverse = ValidationService.certificateInventory(
            List.of(second, first),
            first
        );

        assertEquals(forward, reverse);
        assertEquals(
            forward.stream().map(item -> (String) item.get("sha256Fingerprint")).sorted().toList(),
            forward.stream().map(item -> (String) item.get("sha256Fingerprint")).toList()
        );
        assertEquals(2, forward.size());
        assertEquals(1, forward.stream()
            .filter(item -> Boolean.TRUE.equals(item.get("signingCertificate")))
            .count());
        assertEquals(
            sha256(first.getEncoded()),
            forward.stream()
                .filter(item -> Boolean.TRUE.equals(item.get("signingCertificate")))
                .findFirst()
                .orElseThrow()
                .get("sha256Fingerprint")
        );
    }

    @Test
    void exactCertificatePolicyTrustsOnlyTheMatchingSigningCertificate() throws Exception {
        Path signed = createSignedPdf(temporaryDirectory.resolve("exact-trust.pdf").toAbsolutePath());
        PDFDocumentValidator validator = new PDFDocumentValidator(new FileDocument(signed.toFile()));
        validator.setCertificateVerifier(ValidationService.newOfflineVerifier());
        byte[] signingCertificate = validator.getSignatures().getFirst().getSigningCertificateToken().getEncoded();
        String fingerprint = sha256(signingCertificate);
        String canonical = "{\"policyId\":\"butter-paper-local-explicit-certificates\",\"policyVersion\":1,"
            + "\"enabledExactCertificateFingerprints\":[\"" + fingerprint + "\"]}";
        String configurationSha256 = sha256(canonical.getBytes(StandardCharsets.UTF_8));
        ExactTrustPolicy policy = ExactTrustPolicy.parse(JSON.valueToTree(Map.of(
            "policyId", "butter-paper-local-explicit-certificates",
            "policyVersion", 1,
            "configurationSha256", configurationSha256,
            "exactCertificateAnchors", List.of(Map.of(
                "sha256Fingerprint", fingerprint,
                "derBase64", Base64.getEncoder().encodeToString(signingCertificate)
            ))
        )));

        JsonNode report = report(new ValidationService().validate(signed.toString(), policy));

        assertEquals("explicitly-trusted", report.at("/signatures/0/identityTrust").textValue());
        assertFalse(report.at("/signatures/0/issues").toString().contains("TRUST_PATH_UNAVAILABLE"));
        assertEquals(configurationSha256, report.at("/trust/configurationSha256").textValue());
        assertEquals(fingerprint, report.at("/trust/configuredExactCertificateFingerprints/0").textValue());
        assertEquals(ExactTrustPolicy.POLICY_DISPLAY_NAME, report.at("/trust/policyName").textValue());
        assertTrue(report.at("/limitations/0").textValue().contains("external certificate-path or legal trust"));
    }

    @Test
    void reportsMalformedByteRangeWithoutChangingTheSource() throws Exception {
        Path signed = createSignedPdf(temporaryDirectory.resolve("malformed-byte-range.pdf").toAbsolutePath());
        byte[] malformed = Files.readAllBytes(signed);
        byte[] marker = "/ByteRange [0".getBytes(StandardCharsets.US_ASCII);
        int markerIndex = indexOf(malformed, marker);
        assertTrue(markerIndex >= 0, "signed fixture must contain a ByteRange");
        malformed[markerIndex + marker.length - 1] = '1';
        Files.write(signed, malformed);
        byte[] before = Files.readAllBytes(signed);

        JsonNode report = report(new ValidationService().validate(signed.toString()));

        assertArrayEquals(before, Files.readAllBytes(signed));
        assertEquals("failed", report.at("/signatures/0/integrity").textValue());
        assertDependentAxesIndeterminate(report.at("/signatures/0"));
        assertEquals("malformed", report.at("/signatures/0/coverage").textValue());
        assertEquals("INVALID_BYTE_RANGE", report.at("/signatures/0/issues/0/code").textValue());
        assertEquals("indeterminate", report.at("/inventory/presence").textValue());
        assertFalse(report.at("/inventory/revisionInventoryComplete").booleanValue());
        assertFalse(report.at("/inventory/modificationPolicyComplete").booleanValue());
        assertTrue(report.at("/inventory/currentRevision").isNull());
        assertEquals("INVALID_BYTE_RANGE", report.at("/issues/0/code").textValue());
    }

    @Test
    void numbersMultipleSignedRevisionsInChronologicalOrder() throws Exception {
        Path first = createSignedPdf(temporaryDirectory.resolve("first-revision.pdf").toAbsolutePath());
        Path second = temporaryDirectory.resolve("second-revision.pdf").toAbsolutePath();
        appendSignature(
            first,
            second,
            "butter-paper-phase-1-second-signature",
            "Butter Paper Phase 1 Second Test",
            BigInteger.valueOf(20260806),
            SIGNING_INSTANT.plusSeconds(60)
        );

        JsonNode report = report(new ValidationService().validate(second.toString()));

        assertEquals(2, report.at("/inventory/currentRevision").intValue());
        assertEquals(2, report.at("/inventory/totalRevisions").intValue());
        assertTrue(report.at("/inventory/revisionInventoryComplete").booleanValue());
        assertEquals(2, report.at("/signatures").size());
        JsonNode firstRevision = null;
        JsonNode secondRevision = null;
        for (JsonNode signature : report.at("/signatures")) {
            if (signature.get("signedRevision").intValue() == 1) firstRevision = signature;
            if (signature.get("signedRevision").intValue() == 2) secondRevision = signature;
        }
        assertNotNull(firstRevision);
        assertNotNull(secondRevision);
        assertEquals("permitted", firstRevision.get("modificationStatus").textValue());
        assertFalse(firstRevision.get("issues").toString().contains("VALIDATION_OFFLINE_INCOMPLETE"));
        assertEquals("none", secondRevision.get("modificationStatus").textValue());
        assertEquals("intact", firstRevision.get("integrity").textValue());
        assertEquals("intact", secondRevision.get("integrity").textValue());
    }

    @Test
    void changedCoveredByteFailsClosedAcrossDependentAxes() throws Exception {
        Path signed = createSignedPdf(temporaryDirectory.resolve("changed-covered-byte.pdf").toAbsolutePath());
        byte[] changed = Files.readAllBytes(signed);
        byte[] marker = "Butter Paper Phase 1 Test".getBytes(StandardCharsets.US_ASCII);
        int markerIndex = indexOf(changed, marker);
        assertTrue(markerIndex >= 0, "signed fixture must expose the signer claim in covered PDF bytes");
        changed[markerIndex] = 'M';
        Files.write(signed, changed);

        JsonNode report = report(new ValidationService().validate(signed.toString()));
        JsonNode signature = report.at("/signatures/0");

        assertEquals("failed", signature.get("integrity").textValue());
        assertEquals("prohibited", signature.get("modificationStatus").textValue());
        assertDependentAxesIndeterminate(signature);
        assertTrue(signature.get("issues").toString().contains("CRYPTOGRAPHIC_FAILURE"));
        assertTrue(signature.get("issues").toString().contains("MODIFICATION_PROHIBITED"));
    }

    @Test
    void serverAuthenticationOnlyCertificateIsNotAcceptedForDocumentSigning() throws Exception {
        Path unsigned = temporaryDirectory.resolve("wrong-eku-unsigned.pdf").toAbsolutePath();
        try (PDDocument document = new PDDocument()) {
            document.addPage(new PDPage());
            document.save(unsigned.toFile());
        }
        Path signed = temporaryDirectory.resolve("wrong-eku-signed.pdf").toAbsolutePath();
        appendSignatureWithAlgorithm(
            unsigned,
            signed,
            "butter-paper-phase-1-wrong-eku",
            "Butter Paper Wrong EKU Test",
            BigInteger.valueOf(20260810),
            SIGNING_INSTANT,
            false,
            "RSA",
            "SHA256withRSA",
            KeyPurposeId.id_kp_serverAuth
        );

        JsonNode signature = report(new ValidationService().validate(signed.toString()))
            .at("/signatures/0");

        assertEquals("intact", signature.get("integrity").textValue());
        assertEquals("indeterminate", signature.get("certificateStatus").textValue());
        assertTrue(signature.get("issues").toString().contains("SIGNING_EKU_UNSUITABLE"));
    }

    @Test
    void appendedValidationEvidenceDictionaryIsAPermittedLaterRevision() throws Exception {
        Path signed = createSignedPdf(temporaryDirectory.resolve("dss-base.pdf").toAbsolutePath());
        Path withDss = temporaryDirectory.resolve("dss-update.pdf").toAbsolutePath();
        try (PDDocument document = Loader.loadPDF(signed.toFile())) {
            COSDictionary dss = new COSDictionary();
            dss.setItem(COSName.getPDFName("VRI"), new COSDictionary());
            document.getDocumentCatalog().getCOSObject().setItem(COSName.getPDFName("DSS"), dss);
            try (java.io.OutputStream stream = Files.newOutputStream(withDss)) {
                document.saveIncremental(stream);
            }
        }

        JsonNode report = report(new ValidationService().validate(withDss.toString()));

        assertEquals(1, report.at("/signatures").size());
        assertEquals("intact", report.at("/signatures/0/integrity").textValue());
        assertEquals("permitted", report.at("/signatures/0/modificationStatus").textValue());
        assertTrue(report.at("/inventory/validationEvidence/dssPresent").booleanValue());
        assertTrue(report.at("/inventory/validationEvidence/vriPresent").booleanValue());
        assertEquals("well-formed", report.at("/inventory/validationEvidence/structureStatus").textValue());
        assertEquals(0, report.at("/inventory/validationEvidence/vriEntryCount").intValue());
        assertTrue(report.at("/inventory/validationEvidence/inventoryComplete").booleanValue());
    }

    @Test
    void inventoriesDssAndVriReferencesWithoutReturningEvidenceBodiesOrValidityClaims() throws Exception {
        Path signed = createSignedPdf(temporaryDirectory.resolve("dss-inventory-base.pdf").toAbsolutePath());
        Path withDss = temporaryDirectory.resolve("dss-inventory.pdf").toAbsolutePath();
        String vriKey = "A".repeat(40);
        try (PDDocument document = Loader.loadPDF(signed.toFile())) {
            COSStream certificate = evidenceStream(document, "certificate-body");
            COSStream ocsp = evidenceStream(document, "ocsp-body");
            COSStream crl = evidenceStream(document, "crl-body");
            COSDictionary dss = new COSDictionary();
            dss.setItem(COSName.getPDFName("Certs"), array(certificate));
            dss.setItem(COSName.getPDFName("OCSPs"), array(ocsp));
            dss.setItem(COSName.getPDFName("CRLs"), array(crl));
            COSDictionary vriEntry = new COSDictionary();
            vriEntry.setItem(COSName.getPDFName("Cert"), array(certificate));
            vriEntry.setItem(COSName.getPDFName("OCSP"), array(ocsp));
            vriEntry.setItem(COSName.getPDFName("CRL"), array(crl));
            COSDictionary vri = new COSDictionary();
            vri.setItem(COSName.getPDFName(vriKey), vriEntry);
            dss.setItem(COSName.getPDFName("VRI"), vri);
            document.getDocumentCatalog().getCOSObject().setItem(COSName.getPDFName("DSS"), dss);
            try (java.io.OutputStream stream = Files.newOutputStream(withDss)) {
                document.saveIncremental(stream);
            }
        }

        JsonNode report = report(new ValidationService().validate(withDss.toString()));
        JsonNode evidence = report.at("/inventory/validationEvidence");

        assertEquals("well-formed", evidence.get("structureStatus").textValue());
        assertTrue(evidence.get("inventoryComplete").booleanValue());
        assertFalse(evidence.get("limitExceeded").booleanValue());
        assertEquals(1, evidence.at("/certificates/referenceCount").intValue());
        assertEquals(1, evidence.at("/certificates/embeddedObjectCount").intValue());
        assertEquals(1, evidence.at("/ocspResponses/embeddedObjectCount").intValue());
        assertEquals(1, evidence.at("/crls/embeddedObjectCount").intValue());
        assertEquals(1, evidence.get("vriEntryCount").intValue());
        assertEquals(vriKey, evidence.at("/vriEntries/0/keyReference").textValue());
        assertEquals(sha256(vriKey.getBytes(StandardCharsets.UTF_8)),
            evidence.at("/vriEntries/0/keyReferenceSha256").textValue());
        assertEquals("sha1-hex", evidence.at("/vriEntries/0/keyReferenceFormat").textValue());
        assertEquals(1, evidence.at("/vriEntries/0/certificates/referenceCount").intValue());
        assertEquals(1, evidence.at("/vriEntries/0/ocspResponses/referenceCount").intValue());
        assertEquals(1, evidence.at("/vriEntries/0/crls/referenceCount").intValue());
        assertFalse(report.toString().contains("certificate-body"));
        assertFalse(report.toString().contains("ocsp-body"));
        assertFalse(report.toString().contains("crl-body"));
        assertTrue(report.findValues("valid").isEmpty());
    }

    @Test
    void classifiesMalformedDssAndVriStructuresWithoutInventingCounts() throws Exception {
        Path input = temporaryDirectory.resolve("malformed-dss.pdf").toAbsolutePath();
        try (PDDocument document = new PDDocument()) {
            document.addPage(new PDPage());
            COSDictionary dss = new COSDictionary();
            COSArray certificates = new COSArray();
            certificates.add(new COSString("not-an-embedded-stream"));
            dss.setItem(COSName.getPDFName("Certs"), certificates);
            dss.setItem(COSName.getPDFName("OCSPs"), new COSString("not-an-array"));
            COSDictionary vri = new COSDictionary();
            vri.setItem(COSName.getPDFName("NOT-A-SHA1-REFERENCE"), new COSString("not-a-dictionary"));
            dss.setItem(COSName.getPDFName("VRI"), vri);
            document.getDocumentCatalog().getCOSObject().setItem(COSName.getPDFName("DSS"), dss);
            document.save(input.toFile());
        }

        JsonNode evidence = report(new ValidationService().validate(input.toString()))
            .at("/inventory/validationEvidence");

        assertEquals("malformed", evidence.get("structureStatus").textValue());
        assertTrue(evidence.get("inventoryComplete").booleanValue());
        assertEquals(1, evidence.at("/certificates/referenceCount").intValue());
        assertEquals(0, evidence.at("/certificates/embeddedObjectCount").intValue());
        assertEquals(1, evidence.at("/certificates/malformedEntryCount").intValue());
        assertTrue(evidence.at("/ocspResponses/referenceCount").isNull());
        assertEquals(1, evidence.at("/ocspResponses/malformedEntryCount").intValue());
        assertEquals("other", evidence.at("/vriEntries/0/keyReferenceFormat").textValue());
        assertEquals("malformed", evidence.at("/vriEntries/0/structureStatus").textValue());
        assertTrue(evidence.at("/vriEntries/0/certificates").isNull());

        try (PDDocument document = new PDDocument()) {
            document.getDocumentCatalog().getCOSObject().setItem(
                COSName.getPDFName("DSS"),
                new COSString("not-a-dictionary")
            );
            JsonNode malformedRoot = report(ValidationService.validationEvidenceInventory(document));
            assertEquals("malformed", malformedRoot.get("structureStatus").textValue());
            assertTrue(malformedRoot.get("dssPresent").booleanValue());
            assertTrue(malformedRoot.get("vriPresent").isNull());
            assertTrue(malformedRoot.get("certificates").isNull());
            assertTrue(malformedRoot.get("vriEntryCount").isNull());
        }
    }

    @Test
    void boundsVriEntryInventoryDeterministically() throws Exception {
        try (PDDocument document = new PDDocument()) {
            COSDictionary dss = new COSDictionary();
            COSDictionary vri = new COSDictionary();
            for (int index = 0; index <= Protocol.MAX_CONTAINER_ENTRIES; index++) {
                vri.setItem(COSName.getPDFName(String.format("%040X", index)), new COSDictionary());
            }
            dss.setItem(COSName.getPDFName("VRI"), vri);
            document.getDocumentCatalog().getCOSObject().setItem(COSName.getPDFName("DSS"), dss);

            JsonNode evidence = report(ValidationService.validationEvidenceInventory(document));
            assertEquals("indeterminate", evidence.get("structureStatus").textValue());
            assertFalse(evidence.get("inventoryComplete").booleanValue());
            assertTrue(evidence.get("limitExceeded").booleanValue());
            assertEquals(Protocol.MAX_CONTAINER_ENTRIES + 1, evidence.get("vriEntryCount").intValue());
            assertTrue(evidence.get("vriEntries").isEmpty());
        }
    }

    @Test
    void corruptFinalSignatureContainerDoesNotLeakDependentConclusions() throws Exception {
        Path first = createSignedPdf(temporaryDirectory.resolve("corrupt-final-first.pdf").toAbsolutePath());
        Path second = temporaryDirectory.resolve("corrupt-final-second.pdf").toAbsolutePath();
        appendSignature(
            first,
            second,
            "butter-paper-phase-1-corrupt-final",
            "Butter Paper Corrupt Final Test",
            BigInteger.valueOf(20260811),
            SIGNING_INSTANT.plusSeconds(180)
        );
        byte[] complete = Files.readAllBytes(second);
        assertTrue(complete.length > 256);
        byte[] corrupted = Arrays.copyOf(complete, complete.length - 96);
        Files.write(second, corrupted);

        JsonNode report = report(new ValidationService().validate(second.toString()));
        assertEquals(2, report.at("/signatures").size());
        assertFalse(report.at("/inventory/revisionInventoryComplete").booleanValue());
        assertEquals("unable-to-classify", report.at("/signatures/0/modificationStatus").textValue());
        JsonNode finalSignature = report.at("/signatures/1");
        assertNotEquals("intact", finalSignature.get("integrity").textValue());
        assertDependentAxesIndeterminate(finalSignature);
    }

    @Test
    void catalogDesignatedCertificationWithoutTransformRemainsCertifiedButUnknown() throws Exception {
        Path unsigned = temporaryDirectory.resolve("certification-unsigned.pdf").toAbsolutePath();
        Path certified = temporaryDirectory.resolve("certification-unknown.pdf").toAbsolutePath();
        try (PDDocument document = new PDDocument()) {
            document.addPage(new PDPage());
            document.save(unsigned.toFile());
        }
        appendSignature(
            unsigned,
            certified,
            "butter-paper-phase-1-unknown-certification",
            "Butter Paper Unknown Certification Test",
            BigInteger.valueOf(20260807),
            SIGNING_INSTANT.plusSeconds(120),
            true
        );

        JsonNode report = report(new ValidationService().validate(certified.toString()));

        assertEquals("certified", report.at("/inventory/presence").textValue());
        assertEquals("unknown", report.at("/inventory/certificationPermission").textValue());
        assertFalse(report.at("/inventory/modificationPolicyComplete").booleanValue());
        assertEquals("certification", report.at("/signatures/0/kind").textValue());
        assertEquals("intact", report.at("/signatures/0/integrity").textValue());
    }

    @Test
    void verifiedTimestampKindsDriveTheIndependentSigningTimeAxis() {
        Date claimed = Date.from(SIGNING_INSTANT);
        assertEquals("claimed-only", ValidationService.signingTimeStatus(List.of(), claimed));
        assertEquals("missing", ValidationService.signingTimeStatus(List.of(), null));
        assertEquals("timestamp-verified", ValidationService.signingTimeStatus(List.of(
            Map.of("kind", "signature-timestamp", "verified", true)
        ), claimed));
        assertEquals("document-timestamp-verified", ValidationService.signingTimeStatus(List.of(
            Map.of("kind", "signature-timestamp", "verified", true),
            Map.of("kind", "document-timestamp", "verified", true)
        ), claimed));
    }

    @Test
    void rejectsCorruptPdfWithStableErrorAndNoSourceMutation() throws Exception {
        Path input = temporaryDirectory.resolve("corrupt.pdf").toAbsolutePath();
        byte[] before = "%PDF-1.7\nnot a PDF body\n%%EOF\n".getBytes(StandardCharsets.US_ASCII);
        Files.write(input, before);

        ValidationService.ValidationException exception = assertThrows(
            ValidationService.ValidationException.class,
            () -> new ValidationService().validate(input.toString())
        );

        assertEquals("MALFORMED_PDF", exception.code());
        assertArrayEquals(before, Files.readAllBytes(input));
    }

    static Path createSignedPdf(Path output) throws Exception {
        Path unsigned = output.resolveSibling(output.getFileName() + ".unsigned");
        try (PDDocument document = new PDDocument()) {
            document.addPage(new PDPage());
            document.save(unsigned.toFile());
        }

        appendSignature(
            unsigned,
            output,
            "butter-paper-phase-1-signature-fixture",
            "Butter Paper Phase 1 Test",
            BigInteger.valueOf(20260805),
            SIGNING_INSTANT
        );
        Files.delete(unsigned);
        return output;
    }

    private static CertificateToken certificateToken(
        String deterministicSeed,
        BigInteger serialNumber
    ) throws Exception {
        KeyPairGenerator generator = KeyPairGenerator.getInstance("RSA");
        SecureRandom deterministicRandom = SecureRandom.getInstance("SHA1PRNG");
        deterministicRandom.setSeed(deterministicSeed.getBytes(StandardCharsets.US_ASCII));
        generator.initialize(2048, deterministicRandom);
        KeyPair keyPair = generator.generateKeyPair();

        if (Security.getProvider(BouncyCastleProvider.PROVIDER_NAME) == null) {
            Security.addProvider(new BouncyCastleProvider());
        }
        X500Name subject = new X500Name("CN=Butter Paper Certificate Order Test,O=Butter Paper Tests,C=AU");
        ContentSigner certificateSigner = new JcaContentSignerBuilder("SHA256withRSA")
            .setProvider(BouncyCastleProvider.PROVIDER_NAME)
            .build(keyPair.getPrivate());
        X509Certificate certificate = new JcaX509CertificateConverter()
            .setProvider(BouncyCastleProvider.PROVIDER_NAME)
            .getCertificate(new JcaX509v3CertificateBuilder(
                subject,
                serialNumber,
                Date.from(Instant.parse("2025-08-05T00:00:00Z")),
                Date.from(Instant.parse("2036-08-05T00:00:00Z")),
                subject,
                keyPair.getPublic()
            ).build(certificateSigner));
        return new CertificateToken(certificate);
    }

    static void appendSignature(
        Path input,
        Path output,
        String deterministicSeed,
        String signerName,
        BigInteger serialNumber,
        Instant signingInstant
    ) throws Exception {
        appendSignature(input, output, deterministicSeed, signerName, serialNumber, signingInstant, false);
    }

    static void appendSignature(
        Path input,
        Path output,
        String deterministicSeed,
        String signerName,
        BigInteger serialNumber,
        Instant signingInstant,
        boolean designateCertification
    ) throws Exception {
        appendSignatureWithAlgorithm(
            input,
            output,
            deterministicSeed,
            signerName,
            serialNumber,
            signingInstant,
            designateCertification,
            "RSA",
            "SHA256withRSA"
        );
    }

    private static void appendSignatureWithAlgorithm(
        Path input,
        Path output,
        String deterministicSeed,
        String signerName,
        BigInteger serialNumber,
        Instant signingInstant,
        String keyAlgorithm,
        String signatureAlgorithm
    ) throws Exception {
        appendSignatureWithAlgorithm(
            input,
            output,
            deterministicSeed,
            signerName,
            serialNumber,
            signingInstant,
            false,
            keyAlgorithm,
            signatureAlgorithm,
            null
        );
    }

    private static void appendSignatureWithAlgorithm(
        Path input,
        Path output,
        String deterministicSeed,
        String signerName,
        BigInteger serialNumber,
        Instant signingInstant,
        boolean designateCertification,
        String keyAlgorithm,
        String signatureAlgorithm
    ) throws Exception {
        appendSignatureWithAlgorithm(
            input,
            output,
            deterministicSeed,
            signerName,
            serialNumber,
            signingInstant,
            designateCertification,
            keyAlgorithm,
            signatureAlgorithm,
            null
        );
    }

    private static void appendSignatureWithAlgorithm(
        Path input,
        Path output,
        String deterministicSeed,
        String signerName,
        BigInteger serialNumber,
        Instant signingInstant,
        boolean designateCertification,
        String keyAlgorithm,
        String signatureAlgorithm,
        KeyPurposeId extendedKeyUsage
    ) throws Exception {
        KeyPairGenerator generator = KeyPairGenerator.getInstance(keyAlgorithm);
        SecureRandom deterministicRandom = SecureRandom.getInstance("SHA1PRNG");
        deterministicRandom.setSeed(deterministicSeed.getBytes(StandardCharsets.US_ASCII));
        if ("EC".equals(keyAlgorithm)) {
            generator.initialize(new ECGenParameterSpec("secp256r1"), deterministicRandom);
        } else {
            generator.initialize(2048, deterministicRandom);
        }
        KeyPair keyPair = generator.generateKeyPair();

        if (Security.getProvider(BouncyCastleProvider.PROVIDER_NAME) == null) {
            Security.addProvider(new BouncyCastleProvider());
        }
        X500Name subject = new X500Name("CN=Butter Paper Phase 1 Test,O=Butter Paper Tests,C=AU");
        ContentSigner certificateSigner = new JcaContentSignerBuilder(signatureAlgorithm)
            .setProvider(BouncyCastleProvider.PROVIDER_NAME)
            .build(keyPair.getPrivate());
        JcaX509v3CertificateBuilder certificateBuilder = new JcaX509v3CertificateBuilder(
            subject,
            serialNumber,
            Date.from(Instant.parse("2025-08-05T00:00:00Z")),
            Date.from(Instant.parse("2036-08-05T00:00:00Z")),
            subject,
            keyPair.getPublic()
        );
        if (extendedKeyUsage != null) {
            certificateBuilder.addExtension(
                Extension.extendedKeyUsage,
                false,
                new ExtendedKeyUsage(extendedKeyUsage)
            );
        }
        X509Certificate certificate = new JcaX509CertificateConverter()
            .setProvider(BouncyCastleProvider.PROVIDER_NAME)
            .getCertificate(certificateBuilder.build(certificateSigner));

        try (PDDocument document = Loader.loadPDF(input.toFile())) {
            PDSignature signature = new PDSignature();
            signature.setFilter(PDSignature.FILTER_ADOBE_PPKLITE);
            signature.setSubFilter(PDSignature.SUBFILTER_ETSI_CADES_DETACHED);
            signature.setName(signerName);
            Calendar signingTime = Calendar.getInstance(TimeZone.getTimeZone("UTC"));
            signingTime.setTime(Date.from(signingInstant));
            signature.setSignDate(signingTime);
            if (designateCertification) {
                COSDictionary permissions = new COSDictionary();
                permissions.setItem(COSName.getPDFName("DocMDP"), signature);
                document.getDocumentCatalog().getCOSObject().setItem(
                    COSName.getPDFName("Perms"),
                    permissions
                );
            }
            document.addSignature(signature, content -> cmsSignature(
                content, keyPair, certificate, signatureAlgorithm
            ));
            try (java.io.OutputStream stream = Files.newOutputStream(output)) {
                document.saveIncremental(stream);
            }
        }
        try (PDDocument ignored = Loader.loadPDF(output.toFile())) {
            assertFalse(ignored.getSignatureDictionaries().isEmpty());
        }
    }

    private static byte[] cmsSignature(InputStream content, KeyPair keyPair, X509Certificate certificate) {
        return cmsSignature(content, keyPair, certificate, "SHA256withRSA");
    }

    private static byte[] cmsSignature(
        InputStream content,
        KeyPair keyPair,
        X509Certificate certificate,
        String signatureAlgorithm
    ) {
        try {
            byte[] signedContent = content.readAllBytes();
            CMSSignedDataGenerator generator = new CMSSignedDataGenerator();
            ContentSigner signer = new JcaContentSignerBuilder(signatureAlgorithm)
                .setProvider(BouncyCastleProvider.PROVIDER_NAME)
                .build(keyPair.getPrivate());
            JcaSignerInfoGeneratorBuilder signerInfo = new JcaSignerInfoGeneratorBuilder(
                new JcaDigestCalculatorProviderBuilder()
                    .setProvider(BouncyCastleProvider.PROVIDER_NAME)
                    .build()
            );
            signerInfo.setDirectSignature(true);
            generator.addSignerInfoGenerator(signerInfo.build(signer, certificate));
            generator.addCertificates(new JcaCertStore(List.of(certificate)));
            return generator.generate(new CMSProcessableByteArray(signedContent), false).getEncoded();
        } catch (Exception exception) {
            throw new IllegalStateException("Could not create deterministic test signature", exception);
        }
    }

    private static COSStream evidenceStream(PDDocument document, String contents) throws Exception {
        COSStream stream = document.getDocument().createCOSStream();
        try (java.io.OutputStream output = stream.createOutputStream()) {
            output.write(contents.getBytes(StandardCharsets.US_ASCII));
        }
        return stream;
    }

    private static COSArray array(COSStream stream) {
        COSArray result = new COSArray();
        result.add(stream);
        return result;
    }

    private static JsonNode report(Map<String, Object> report) {
        return JSON.valueToTree(report);
    }

    private static String sha256(byte[] bytes) throws Exception {
        return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));
    }

    private static int indexOf(byte[] haystack, byte[] needle) {
        outer: for (int index = 0; index <= haystack.length - needle.length; index++) {
            for (int offset = 0; offset < needle.length; offset++) {
                if (haystack[index + offset] != needle[offset]) continue outer;
            }
            return index;
        }
        return -1;
    }

    private static void assertDependentAxesIndeterminate(JsonNode signature) {
        assertEquals("indeterminate", signature.get("identityTrust").textValue());
        assertEquals("indeterminate", signature.get("certificateStatus").textValue());
        assertEquals("indeterminate", signature.get("signingTime").textValue());
        assertEquals("indeterminate", signature.at("/evidenceFreshness/source").textValue());
        assertTrue(signature.at("/evidenceFreshness/producedAt").isNull());
        assertTrue(signature.at("/evidenceFreshness/nextUpdateAt").isNull());
    }
}
