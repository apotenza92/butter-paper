package com.butterpaper.signaturecore;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.apache.pdfbox.Loader;
import org.apache.pdfbox.pdmodel.PDDocument;
import org.apache.pdfbox.pdmodel.interactive.digitalsignature.PDSignature;
import org.apache.pdfbox.pdmodel.interactive.form.PDSignatureField;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

import java.io.ByteArrayOutputStream;
import java.nio.file.Files;
import java.security.KeyStore;
import java.time.Clock;
import java.time.Instant;
import java.time.ZoneOffset;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SigningServiceTest {
    private static final ObjectMapper JSON = new ObjectMapper();
    private static final Clock FIXED_CLOCK = Clock.fixed(Instant.parse("2026-08-05T00:00:00Z"), ZoneOffset.UTC);

    @Test
    void rejectsUnqualifiedPdfSerializationModesBeforeSigning() throws Exception {
        try (PDDocument xrefStream = new PDDocument()) {
            xrefStream.getDocument().setIsXRefStream(true);
            SigningService.SigningException exception = assertThrows(
                SigningService.SigningException.class,
                () -> SigningService.assertSupportedSourceSerialization(xrefStream)
            );
            assertEquals("SOURCE_SERIALIZATION_UNSUPPORTED", exception.code());
        }

        try (PDDocument hybrid = new PDDocument()) {
            hybrid.getDocument().setHasHybridXRef();
            SigningService.SigningException exception = assertThrows(
                SigningService.SigningException.class,
                () -> SigningService.assertSupportedSourceSerialization(hybrid)
            );
            assertEquals("SOURCE_SERIALIZATION_UNSUPPORTED", exception.code());
        }

    }

    @ParameterizedTest
    @ValueSource(strings = {"RSA", "EC"})
    void signsInvisiblePadesBaselineBIncrementally(String algorithm) throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity(algorithm);
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        SigningService service = new SigningService(SigningTestFixtures.fixedPrompt(), FIXED_CLOCK);
        byte[] pkcs12 = identity.pkcs12().clone();
        Map<String, Object> result = service.sign(
            signingPayload(workspace, identity, newField("ApprovalSignature", null), false, null),
            new HashMap<>(Map.of("pkcs12", pkcs12)),
            false
        );

        assertEquals("approval", result.get("kind"));
        assertEquals("PAdES-B-B", result.get("profile"));
        assertEquals("SHA-256", result.get("digestAlgorithm"));
        assertEquals(true, result.get("appendOnly"));
        assertTrue(allZero(pkcs12));
        byte[] output = Files.readAllBytes(workspace.output());
        SafePdfMutation.assertAppendOnly(workspace.sourceBytes(), output);
        try (PDDocument document = Loader.loadPDF(output)) {
            assertEquals(1, document.getSignatureDictionaries().size());
            assertEquals("ApprovalSignature", document.getSignatureFields().getFirst().getFullyQualifiedName());
        }
    }

    @ParameterizedTest
    @ValueSource(strings = {"SHA-256", "SHA-384", "SHA-512"})
    void returnsCanonicalHyphenatedDigestNames(String digestName) throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        ObjectNode payload = signingPayload(
            workspace, identity, newField("DigestSignature", null), false, null
        );
        payload.put("digestAlgorithm", digestName);
        Map<String, Object> result = new SigningService(SigningTestFixtures.fixedPrompt(), FIXED_CLOCK).sign(
            payload,
            new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
            false
        );
        assertEquals(digestName, result.get("digestAlgorithm"));
    }

    @Test
    void signsVisibleRasterWithoutReturningItsBytes() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        byte[] appearance = SigningTestFixtures.png();
        Map<String, byte[]> frames = new HashMap<>();
        frames.put("pkcs12", identity.pkcs12().clone());
        frames.put("appearance", appearance.clone());
        Map<String, Object> result = new SigningService(SigningTestFixtures.fixedPrompt(), FIXED_CLOCK).sign(
            signingPayload(workspace, identity, newField("VisibleSignature", widget()), true, null),
            frames,
            false
        );
        assertEquals("VisibleSignature", result.get("fieldName"));
        assertFalse(result.toString().contains(java.util.Base64.getEncoder().encodeToString(appearance)));
        try (PDDocument document = Loader.loadPDF(Files.readAllBytes(workspace.output()))) {
            PDSignatureField field = document.getSignatureFields().getFirst();
            assertFalse(field.getWidgets().isEmpty());
            assertTrue(field.getWidgets().getFirst().getCOSObject().containsKey(org.apache.pdfbox.cos.COSName.AP));
        }
    }

    @Test
    void preservesUnicodeReasonLocationAndContactExactly() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        ObjectNode payload = signingPayload(
            workspace, identity, newField("UnicodeMetadata", null), false, null
        );
        String reason = "Reviewed – Δοκιμή – 测试 – اختبار";
        String location = "München / 東京 / ملبورن";
        String contact = "签署人+δοκιμή@example.invalid";
        payload.put("reason", reason);
        payload.put("location", location);
        payload.put("contact", contact);
        new SigningService(SigningTestFixtures.fixedPrompt(), FIXED_CLOCK).sign(
            payload,
            new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
            false
        );
        assertSignatureMetadata(workspace.output(), reason, location, contact);
    }

    @Test
    void treatsOmittedAndEmptySignerMetadataAsAbsent() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace omitted = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        ObjectNode omittedPayload = signingPayload(
            omitted, identity, newField("OmittedMetadata", null), false, null
        );
        omittedPayload.remove(List.of("reason", "location", "contact"));
        new SigningService(SigningTestFixtures.fixedPrompt(), FIXED_CLOCK).sign(
            omittedPayload,
            new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
            false
        );
        assertSignatureMetadata(omitted.output(), null, null, null);

        SigningTestFixtures.Workspace empty = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        ObjectNode emptyPayload = signingPayload(
            empty, identity, newField("EmptyMetadata", null), false, null
        );
        emptyPayload.put("reason", "");
        emptyPayload.put("location", "");
        emptyPayload.put("contact", "");
        new SigningService(SigningTestFixtures.fixedPrompt(), FIXED_CLOCK).sign(
            emptyPayload,
            new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
            false
        );
        assertSignatureMetadata(empty.output(), null, null, null);
    }

    @Test
    void acceptsExactMaximumSignerMetadataLengths() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        ObjectNode payload = signingPayload(
            workspace, identity, newField("MaximumMetadata", null), false, null
        );
        String reason = "R".repeat(1_024);
        String location = "L".repeat(512);
        String contact = "C".repeat(512);
        payload.put("reason", reason);
        payload.put("location", location);
        payload.put("contact", contact);
        new SigningService(SigningTestFixtures.fixedPrompt(), FIXED_CLOCK).sign(
            payload,
            new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
            false
        );
        assertSignatureMetadata(workspace.output(), reason, location, contact);
    }

    @ParameterizedTest
    @ValueSource(strings = {"reason", "location", "contact"})
    void rejectsOversizedSignerMetadataBeforePromptOrOutputWrite(String property) throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        byte[] sourceBefore = Files.readAllBytes(workspace.source());
        ObjectNode payload = signingPayload(
            workspace, identity, newField("OversizedMetadata", null), false, null
        );
        payload.put(property, "X".repeat("reason".equals(property) ? 1_025 : 513));
        AtomicInteger promptCalls = new AtomicInteger();
        byte[] pkcs12 = identity.pkcs12().clone();
        SigningService.SigningException exception = assertThrows(
            SigningService.SigningException.class,
            () -> new SigningService(() -> {
                promptCalls.incrementAndGet();
                return SigningTestFixtures.PASSWORD.clone();
            }, FIXED_CLOCK).sign(
                payload,
                new HashMap<>(Map.of("pkcs12", pkcs12)),
                false
            )
        );
        assertEquals("INVALID_SIGNING_REQUEST", exception.code());
        assertEquals(0, promptCalls.get());
        assertEquals(0, Files.size(workspace.output()));
        assertArrayEquals(sourceBefore, Files.readAllBytes(workspace.source()));
        assertTrue(allZero(pkcs12));
    }

    @Test
    void rejectsCertificateOnlyPkcs12WithoutChangingSourceOrOutput() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        byte[] sourceBefore = Files.readAllBytes(workspace.source());
        byte[] certificateOnly = certificateOnlyPkcs12(identity);
        SigningService.SigningException exception = assertThrows(
            SigningService.SigningException.class,
            () -> new SigningService(SigningTestFixtures.fixedPrompt(), FIXED_CLOCK).sign(
                signingPayload(workspace, identity, newField("NoPrivateKey", null), false, null),
                new HashMap<>(Map.of("pkcs12", certificateOnly)),
                false
            )
        );
        assertEquals("PRIVATE_KEY_MISSING", exception.code());
        assertTrue(allZero(certificateOnly));
        assertArrayEquals(sourceBefore, Files.readAllBytes(workspace.source()));
        assertEquals(0, Files.size(workspace.output()));
    }

    @Test
    void truncatesOutputWhenReopenedPostvalidationFails() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        byte[] sourceBefore = Files.readAllBytes(workspace.source());
        byte[] pkcs12 = identity.pkcs12().clone();
        AtomicInteger calls = new AtomicInteger();
        SignedMutationPostcheck delegate = new SignedMutationPostcheck();
        SigningService.SignaturePostcheck failingSecondCheck = (
            source, output, fieldName, certificateSha256, certification, permission, appearance, lock
        ) -> {
            if (calls.incrementAndGet() == 2) {
                throw new SignedMutationPostcheck.PostcheckException("OUTPUT_VALIDATION_FAILED");
            }
            return delegate.verifySignature(
                source, output, fieldName, certificateSha256, certification, permission, appearance, lock
            );
        };
        SigningService.SigningException exception = assertThrows(
            SigningService.SigningException.class,
            () -> new SigningService(
                SigningTestFixtures.fixedPrompt(), FIXED_CLOCK, new SafePdfMutation(), failingSecondCheck
            ).sign(
                signingPayload(workspace, identity, newField("FailedPostvalidation", null), false, null),
                new HashMap<>(Map.of("pkcs12", pkcs12)),
                false
            )
        );
        assertEquals("OUTPUT_VALIDATION_FAILED", exception.code());
        assertEquals(2, calls.get());
        assertTrue(allZero(pkcs12));
        assertArrayEquals(sourceBefore, Files.readAllBytes(workspace.source()));
        assertEquals(0, Files.size(workspace.output()));
    }

    @Test
    void providerCancellationDuringSigningClearsContainerAndPreservesFiles() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        byte[] sourceBefore = Files.readAllBytes(workspace.source());
        byte[] pkcs12 = identity.pkcs12().clone();
        SigningService.SigningException exception = assertThrows(
            SigningService.SigningException.class,
            () -> new SigningService(() -> {
                throw new Pkcs12PasswordPrompt.PromptException("PROVIDER_CANCELLED");
            }, FIXED_CLOCK).sign(
                signingPayload(workspace, identity, newField("Cancelled", null), false, null),
                new HashMap<>(Map.of("pkcs12", pkcs12)),
                false
            )
        );
        assertEquals("PROVIDER_CANCELLED", exception.code());
        assertTrue(allZero(pkcs12));
        assertArrayEquals(sourceBefore, Files.readAllBytes(workspace.source()));
        assertEquals(0, Files.size(workspace.output()));
    }

    @ParameterizedTest
    @ValueSource(strings = {
        "no-changes",
        "form-filling-and-signatures",
        "form-filling-signatures-and-annotations"
    })
    void certifiesAtEachDocMdpPermission(String permission) throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        Map<String, Object> result = new SigningService(SigningTestFixtures.fixedPrompt(), FIXED_CLOCK).sign(
            signingPayload(workspace, identity, newField("CertificationSignature", null), false, permission),
            new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
            true
        );
        assertEquals("certification", result.get("kind"));
        @SuppressWarnings("unchecked")
        Map<String, Object> postcheck = (Map<String, Object>) result.get("postcheck");
        assertEquals(List.of("no-changes", "form-filling-and-signatures", "form-filling-signatures-and-annotations").indexOf(permission) + 1,
            postcheck.get("certificationPermission"));
    }

    @Test
    void signsAnExistingFieldCreatedIncrementallyWithFieldLock() throws Exception {
        byte[] original = SigningTestFixtures.pdf();
        SignatureFieldSpec field = SignatureFieldSpec.parse(newField(
            "PreparedSignature",
            null,
            fieldLock("include", List.of("PreparedSignature"))
        ));
        byte[] prepared = new SignatureFieldService().addField(original, field);
        SafePdfMutation.assertAppendOnly(original, prepared);
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("EC");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(prepared);
        Map<String, Object> result = new SigningService(SigningTestFixtures.fixedPrompt(), FIXED_CLOCK).sign(
            signingPayload(workspace, identity, existingField("PreparedSignature"), false, null),
            new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
            false
        );
        assertEquals("PreparedSignature", result.get("fieldName"));
        try (PDDocument document = Loader.loadPDF(Files.readAllBytes(workspace.output()))) {
            PDSignatureField signed = document.getSignatureFields().getFirst();
            assertTrue(signed.getCOSObject().containsKey(org.apache.pdfbox.cos.COSName.getPDFName("Lock")));
            assertTrue(signed.getSignature().getCOSObject().toString().contains("FieldMDP"));
        }
    }

    @Test
    void reportsWrongPasswordAndClearsSuppliedContainerBuffer() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        byte[] pkcs12 = identity.pkcs12().clone();
        SigningService service = new SigningService(() -> "wrong password".toCharArray(), FIXED_CLOCK);
        SigningService.SigningException exception = assertThrows(
            SigningService.SigningException.class,
            () -> service.inspectPkcs12(pkcs12)
        );
        assertEquals("PKCS12_UNLOCK_FAILED", exception.code());
        assertTrue(allZero(pkcs12));
        assertFalse(exception.getMessage().contains("wrong password"));
    }

    @Test
    void reportsProviderCancellationWithoutOpeningTheContainer() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        byte[] pkcs12 = identity.pkcs12().clone();
        SigningService service = new SigningService(() -> {
            throw new Pkcs12PasswordPrompt.PromptException("PROVIDER_CANCELLED");
        }, FIXED_CLOCK);
        SigningService.SigningException exception = assertThrows(
            SigningService.SigningException.class,
            () -> service.inspectPkcs12(pkcs12)
        );
        assertEquals("PROVIDER_CANCELLED", exception.code());
        assertTrue(allZero(pkcs12));
    }

    @Test
    void refusesCertificationAfterAnApprovalSignature() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace first = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        SigningService service = new SigningService(SigningTestFixtures.fixedPrompt(), FIXED_CLOCK);
        service.sign(
            signingPayload(first, identity, newField("Approval", null), false, null),
            new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
            false
        );
        SigningTestFixtures.Workspace second = SigningTestFixtures.workspace(Files.readAllBytes(first.output()));
        SigningService.SigningException exception = assertThrows(
            SigningService.SigningException.class,
            () -> service.sign(
                signingPayload(second, identity, newField("Certification", null), false, "no-changes"),
                new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
                true
            )
        );
        assertEquals("CERTIFICATION_REQUIRES_UNSIGNED_SOURCE", exception.code());
        assertEquals(0, Files.size(second.output()));
    }

    @Test
    void refusesAddingANewFieldAfterARevisionHasBeenSigned() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        SigningService service = new SigningService(SigningTestFixtures.fixedPrompt(), FIXED_CLOCK);
        service.sign(
            signingPayload(workspace, identity, newField("Approval", null), false, null),
            new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
            false
        );
        SignatureFieldService.FieldException exception = assertThrows(
            SignatureFieldService.FieldException.class,
            () -> new SignatureFieldService().addField(
                Files.readAllBytes(workspace.output()),
                SignatureFieldSpec.parse(newField("LaterField", null))
            )
        );
        assertEquals("SIGNED_SOURCE_FIELD_CREATION_BLOCKED", exception.code());
    }

    @Test
    void refusesAnExistingFieldLockedByAnEarlierSignature() throws Exception {
        byte[] source = SigningTestFixtures.pdf();
        SignatureFieldService fieldService = new SignatureFieldService();
        source = fieldService.addField(source, SignatureFieldSpec.parse(newField(
            "LockingSignature", null, fieldLock("include", List.of("TargetSignature"))
        )));
        source = fieldService.addField(source, SignatureFieldSpec.parse(newField("TargetSignature", null)));
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningService service = new SigningService(SigningTestFixtures.fixedPrompt(), FIXED_CLOCK);
        SigningTestFixtures.Workspace first = SigningTestFixtures.workspace(source);
        service.sign(
            signingPayload(first, identity, existingField("LockingSignature"), false, null),
            new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
            false
        );
        SigningTestFixtures.Workspace second = SigningTestFixtures.workspace(Files.readAllBytes(first.output()));
        SigningService.SigningException exception = assertThrows(
            SigningService.SigningException.class,
            () -> service.sign(
                signingPayload(second, identity, existingField("TargetSignature"), false, null),
                new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
                false
            )
        );
        assertEquals("SIGNATURE_FIELD_LOCKED", exception.code());
        assertEquals(0, Files.size(second.output()));
    }

    private static ObjectNode signingPayload(
        SigningTestFixtures.Workspace workspace,
        SigningTestFixtures.IdentityFixture identity,
        ObjectNode field,
        boolean visible,
        String permission
    ) {
        ObjectNode payload = JSON.createObjectNode();
        payload.put("inputPath", workspace.source().toString());
        payload.put("outputPath", workspace.output().toString());
        payload.put("expectedInputSha256", workspace.sourceSha256());
        payload.put("certificateSha256", identity.fingerprint());
        payload.put("digestAlgorithm", "SHA-256");
        payload.put("profile", "PAdES-B-B");
        payload.set("field", field);
        payload.put("reason", "Reviewed /ByteRange /Contents endobj obj – Π 测试");
        payload.put("location", "Melbourne");
        payload.put("contact", "test-only@example.invalid");
        payload.put("appearance", visible ? "visible" : "invisible");
        if (permission != null) payload.put("certificationPermission", permission);
        return payload;
    }

    private static void assertSignatureMetadata(
        java.nio.file.Path output,
        String reason,
        String location,
        String contact
    ) throws Exception {
        try (PDDocument document = Loader.loadPDF(output.toFile())) {
            PDSignature signature = document.getSignatureDictionaries().getFirst();
            assertEquals(reason, signature.getReason());
            assertEquals(location, signature.getLocation());
            assertEquals(contact, signature.getContactInfo());
        }
    }

    private static byte[] certificateOnlyPkcs12(SigningTestFixtures.IdentityFixture identity)
        throws Exception {
        KeyStore store = KeyStore.getInstance("PKCS12");
        store.load(null, SigningTestFixtures.PASSWORD);
        store.setCertificateEntry("certificate-only", identity.certificate());
        ByteArrayOutputStream encoded = new ByteArrayOutputStream();
        store.store(encoded, SigningTestFixtures.PASSWORD);
        return encoded.toByteArray();
    }

    private static ObjectNode newField(String name, ObjectNode widget) {
        ObjectNode field = JSON.createObjectNode();
        field.put("kind", "new");
        field.put("name", name);
        if (widget == null) field.putNull("widget"); else field.set("widget", widget);
        return field;
    }

    private static ObjectNode newField(String name, ObjectNode widget, ObjectNode lock) {
        ObjectNode field = newField(name, widget);
        field.set("lock", lock);
        return field;
    }

    private static ObjectNode existingField(String name) {
        ObjectNode field = JSON.createObjectNode();
        field.put("kind", "existing");
        field.put("name", name);
        return field;
    }

    private static ObjectNode widget() {
        ObjectNode widget = JSON.createObjectNode();
        widget.put("pageIndex", 0);
        widget.put("x", 72);
        widget.put("y", 72);
        widget.put("width", 180);
        widget.put("height", 60);
        widget.put("pageRotation", 0);
        widget.put("coordinateSpace", "unrotated-pdf-default-user-space");
        return widget;
    }

    private static ObjectNode fieldLock(String action, List<String> names) {
        ObjectNode lock = JSON.createObjectNode();
        lock.put("action", action);
        var values = lock.putArray("fieldNames");
        names.forEach(values::add);
        return lock;
    }

    private static boolean allZero(byte[] bytes) {
        for (byte value : bytes) if (value != 0) return false;
        return true;
    }
}
