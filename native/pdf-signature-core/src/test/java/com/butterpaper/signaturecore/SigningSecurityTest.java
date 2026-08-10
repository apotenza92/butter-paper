package com.butterpaper.signaturecore;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.apache.pdfbox.Loader;
import org.apache.pdfbox.cos.COSName;
import org.apache.pdfbox.cos.COSArray;
import org.apache.pdfbox.cos.COSDictionary;
import org.apache.pdfbox.cos.COSString;
import org.apache.pdfbox.cos.COSInteger;
import org.apache.pdfbox.cos.COSStream;
import org.apache.pdfbox.pdmodel.PDResources;
import org.apache.pdfbox.pdmodel.common.PDRectangle;
import org.apache.pdfbox.pdmodel.PDDocument;
import org.apache.pdfbox.pdmodel.interactive.digitalsignature.PDSignature;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Clock;
import java.time.Instant;
import java.time.ZoneOffset;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.zip.CRC32;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class SigningSecurityTest {
    private static final ObjectMapper JSON = new ObjectMapper();
    private static final Clock CLOCK = Clock.fixed(Instant.parse("2026-08-05T00:00:00Z"), ZoneOffset.UTC);
    private static final SignedMutationPostcheck.AppearanceExpectation INVISIBLE =
        new SignedMutationPostcheck.AppearanceExpectation(false, null, null, null, null, null, 0);

    @ParameterizedTest
    @ValueSource(strings = {
        "form-filling-and-signatures",
        "form-filling-signatures-and-annotations"
    })
    void signsAPrecreatedFieldAfterAuthoritativeP2OrP3Certification(String permission) throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        byte[] prepared = prepareInvisibleFields("Certification", "Approval");
        byte[] certified = sign(prepared, identity, "Certification", true, permission);
        byte[] approved = sign(certified, identity, "Approval", false, null);
        try (PDDocument document = Loader.loadPDF(approved)) {
            assertEquals(2, document.getSignatureDictionaries().size());
        }
    }

    @Test
    void rejectsApprovalAfterAuthoritativeP1Certification() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        byte[] prepared = prepareInvisibleFields("Certification", "Approval");
        byte[] certified = sign(prepared, identity, "Certification", true, "no-changes");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(certified);
        SigningService.SigningException exception = assertThrows(
            SigningService.SigningException.class,
            () -> new SigningService(SigningTestFixtures.fixedPrompt(), CLOCK).sign(
                payload(workspace, identity, existingField("Approval"), false, null),
                new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
                false
            )
        );
        assertEquals("CERTIFICATION_FORBIDS_SIGNATURE", exception.code());
        assertEquals(0, Files.size(workspace.output()));
    }

    @Test
    void rejectsRemovedOrSubstitutedCurrentCatalogDocMdp() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        byte[] prepared = prepareInvisibleFields("Certification", "Approval", "Target");
        byte[] certified = sign(prepared, identity, "Certification", true, "form-filling-and-signatures");
        assertSourcePolicyRejected(removeCatalogPerms(certified), identity, "Approval");

        byte[] approved = sign(certified, identity, "Approval", false, null);
        byte[] substituted;
        try (PDDocument document = Loader.loadPDF(approved)) {
            COSDictionary perms = (COSDictionary) document.getDocumentCatalog().getCOSObject()
                .getDictionaryObject(COSName.getPDFName("Perms"));
            PDSignature approval = document.getSignatureDictionaries().stream()
                .max(java.util.Comparator.comparingInt(signature -> signature.getByteRange()[2] + signature.getByteRange()[3]))
                .orElseThrow();
            perms.setItem(COSName.getPDFName("DocMDP"), approval.getCOSObject());
            ByteArrayOutputStream encoded = new ByteArrayOutputStream();
            document.saveIncremental(encoded);
            substituted = encoded.toByteArray();
        }
        assertSourcePolicyRejected(substituted, identity, "Target");
    }

    @Test
    void rawFieldMdpParserRejectsDuplicateConflictingAndMalformedPolicies() throws Exception {
        byte[] include = signedFieldLock("include", List.of("Target"));
        assertMalformedRawPolicy(include, signature -> {
            COSArray references = (COSArray) signature.getDictionaryObject(COSName.getPDFName("Reference"));
            references.add(references.get(0));
        });
        assertMalformedRawPolicy(include, signature -> fieldTransform(signature)
            .setName(COSName.getPDFName("V"), "1.1"));
        assertMalformedRawPolicy(include, signature -> {
            COSArray fields = (COSArray) fieldTransform(signature).getDictionaryObject(COSName.getPDFName("Fields"));
            fields.add(new COSString("Target"));
        });

        byte[] exclude = signedFieldLock("exclude", List.of("Unprotected"));
        assertMalformedRawPolicy(exclude, signature -> fieldTransform(signature)
            .removeItem(COSName.getPDFName("Fields")));

        byte[] all = signedFieldLock("all", List.of());
        assertMalformedRawPolicy(all, signature -> {
            COSArray fields = new COSArray();
            fields.add(new COSString("Target"));
            fieldTransform(signature).setItem(COSName.getPDFName("Fields"), fields);
        });
    }

    @Test
    void exactAddedPolicyCheckRejectsWrongFieldLockAndMalformedDocMdpShell() throws Exception {
        byte[] fieldLocked = signedFieldLock("include", List.of("Target"));
        try (PDDocument document = Loader.loadPDF(fieldLocked)) {
            COSDictionary signature = document.getSignatureDictionaries().getFirst().getCOSObject();
            SigningService.SigningException wrongLock = assertThrows(
                SigningService.SigningException.class,
                () -> AuthoritativeSignedPolicies.assertAddedSignaturePolicies(
                    signature,
                    null,
                    new SignatureFieldSpec.FieldLock("exclude", List.of("Target"))
                )
            );
            assertEquals("SOURCE_POLICY_INDETERMINATE", wrongLock.code());
        }

        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        byte[] prepared = prepareInvisibleFields("Certification");
        byte[] certified = sign(
            prepared, identity, "Certification", true, "form-filling-and-signatures"
        );
        try (PDDocument document = Loader.loadPDF(certified)) {
            COSDictionary signature = document.getSignatureDictionaries().getFirst().getCOSObject();
            docTransform(signature).setName(COSName.getPDFName("V"), "1.1");
            SigningService.SigningException malformed = assertThrows(
                SigningService.SigningException.class,
                () -> AuthoritativeSignedPolicies.assertAddedSignaturePolicies(signature, 2, null)
            );
            assertEquals("SOURCE_POLICY_INDETERMINATE", malformed.code());
        }
    }

    @Test
    void enforcesCanonicalIncludeExcludeAndAllFieldMdpActions() throws Exception {
        assertLockedByCanonicalPolicy("include", List.of("Target"));
        assertLockedByCanonicalPolicy("exclude", List.of("Other"));
        assertLockedByCanonicalPolicy("all", List.of());

        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        byte[] prepared = prepareLockedTarget("exclude", List.of("Target"));
        byte[] locking = sign(prepared, identity, "Locking", false, null);
        byte[] target = sign(locking, identity, "Target", false, null);
        try (PDDocument document = Loader.loadPDF(target)) {
            assertEquals(2, document.getSignatureDictionaries().size());
        }
    }

    @Test
    void signsTwoPrecreatedUnlockedFieldsSequentially() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("EC");
        byte[] prepared = prepareInvisibleFields("First", "Second");
        byte[] first = sign(prepared, identity, "First", false, null);
        byte[] second = sign(first, identity, "Second", false, null);
        try (PDDocument document = Loader.loadPDF(second)) {
            assertEquals(2, document.getSignatureDictionaries().size());
            assertEquals(2, document.getSignatureFields().stream().filter(field -> field.getSignature() != null).count());
        }
    }

    @Test
    void signsAPrecreatedVisibleFieldAndRetainsItsExactGeometryAndAppearance() throws Exception {
        byte[] source = new SignatureFieldService().addField(
            SigningTestFixtures.pdf(),
            SignatureFieldSpec.parse(newField("VisibleExisting", widget(), null))
        );
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(source);
        Map<String, byte[]> frames = new HashMap<>();
        frames.put("pkcs12", identity.pkcs12().clone());
        frames.put("appearance", SigningTestFixtures.png());
        new SigningService(SigningTestFixtures.fixedPrompt(), CLOCK).sign(
            payload(workspace, identity, existingField("VisibleExisting"), true, null),
            frames,
            false
        );
        try (PDDocument document = Loader.loadPDF(Files.readAllBytes(workspace.output()))) {
            var widget = document.getSignatureFields().getFirst().getWidgets().getFirst();
            assertEquals(72f, widget.getRectangle().getLowerLeftX());
            assertEquals(72f, widget.getRectangle().getLowerLeftY());
            assertEquals(180f, widget.getRectangle().getWidth());
            assertEquals(60f, widget.getRectangle().getHeight());
            assertEquals(true, widget.getCOSObject().containsKey(COSName.AP));
        }
    }

    @Test
    void bindsTheExpectedCertificateToTheExactNewAdvancedSignature() throws Exception {
        SigningTestFixtures.IdentityFixture firstIdentity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.IdentityFixture secondIdentity = SigningTestFixtures.identity("EC");
        byte[] prepared = prepareInvisibleFields("First", "Second");
        byte[] first = sign(prepared, firstIdentity, "First", false, null);
        byte[] second = sign(first, secondIdentity, "Second", false, null);
        SignedMutationPostcheck.PostcheckException exception = assertThrows(
            SignedMutationPostcheck.PostcheckException.class,
            () -> new SignedMutationPostcheck().verifySignature(
                first, second, "Second", firstIdentity.fingerprint(), false, null, INVISIBLE, null
            )
        );
        assertEquals("SIGNING_CERTIFICATE_MISMATCH", exception.code());
    }

    @Test
    void independentlyPostvalidationBindsTheSelectedCertificateToTheNewSignature() throws Exception {
        SigningTestFixtures.IdentityFixture firstIdentity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.IdentityFixture secondIdentity = SigningTestFixtures.identity("EC");
        byte[] prepared = prepareInvisibleFields("First", "Second");
        byte[] first = sign(prepared, firstIdentity, "First", false, null);
        byte[] second = sign(first, secondIdentity, "Second", false, null);
        SignedMutationPostcheck check = new SignedMutationPostcheck();

        SignedMutationPostcheck.PostcheckException mismatch = assertThrows(
            SignedMutationPostcheck.PostcheckException.class,
            () -> check.verifyIndependent(first, second, "Second", firstIdentity.fingerprint(), "approval", "invisible", null)
        );
        assertEquals("SIGNING_CERTIFICATE_MISMATCH", mismatch.code());
        check.verifyIndependent(first, second, "Second", secondIdentity.fingerprint(), "approval", "invisible", null);
    }

    @Test
    void independentlyPostvalidationBindsOperationAppearanceAndCertificationPermission() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        byte[] prepared = prepareInvisibleFields("Approval", "Other");
        byte[] signed = sign(prepared, identity, "Approval", false, null);
        SignedMutationPostcheck check = new SignedMutationPostcheck();

        SignedMutationPostcheck.PostcheckException appearanceMismatch = assertThrows(
            SignedMutationPostcheck.PostcheckException.class,
            () -> check.verifyIndependent(
                prepared, signed, "Approval", identity.fingerprint(), "approval", "visible", null
            )
        );
        assertEquals("APPEARANCE_MODE_MISMATCH", appearanceMismatch.code());

        SignedMutationPostcheck.PostcheckException certificationMismatch = assertThrows(
            SignedMutationPostcheck.PostcheckException.class,
            () -> check.verifyIndependent(
                prepared, signed, "Approval", identity.fingerprint(), "certification", "invisible", 2
            )
        );
        assertEquals("CERTIFICATION_MISMATCH", certificationMismatch.code());
    }

    @Test
    void rejectsUnsignedRevisionAfterCurrentLockDictionaryIsRemoved() throws Exception {
        byte[] source = SigningTestFixtures.pdf();
        SignatureFieldService fields = new SignatureFieldService();
        source = fields.addField(source, SignatureFieldSpec.parse(newField(
            "Locking", null, fieldLock("include", List.of("Target"))
        )));
        source = fields.addField(source, SignatureFieldSpec.parse(newField("Target", null, null)));
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        byte[] signed = sign(source, identity, "Locking", false, null);
        byte[] lockRemoved;
        try (PDDocument document = Loader.loadPDF(signed)) {
            document.getSignatureFields().stream()
                .filter(field -> "Locking".equals(field.getFullyQualifiedName()))
                .findFirst().orElseThrow()
                .getCOSObject().removeItem(COSName.getPDFName("Lock"));
            ByteArrayOutputStream encoded = new ByteArrayOutputStream();
            document.saveIncremental(encoded);
            lockRemoved = encoded.toByteArray();
        }
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(lockRemoved);
        SigningService.SigningException exception = assertThrows(
            SigningService.SigningException.class,
            () -> new SigningService(SigningTestFixtures.fixedPrompt(), CLOCK).sign(
                payload(workspace, identity, existingField("Target"), false, null),
                new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
                false
            )
        );
        assertEquals("SOURCE_POLICY_INDETERMINATE", exception.code());
    }

    @Test
    void rejectsVisibleWidgetSemanticMismatchAndOversizedImageDimensions() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        SigningService service = new SigningService(SigningTestFixtures.fixedPrompt(), CLOCK);
        SigningService.SigningException invisibleWidget = assertThrows(
            SigningService.SigningException.class,
            () -> service.sign(
                payload(workspace, identity, newField("Mismatch", widget(), null), false, null),
                new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
                false
            )
        );
        assertEquals("INVALID_SIGNING_REQUEST", invisibleWidget.code());
        SigningService.SigningException visibleWithoutWidget = assertThrows(
            SigningService.SigningException.class,
            () -> service.sign(
                payload(workspace, identity, newField("Mismatch", null, null), true, null),
                new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone(), "appearance", SigningTestFixtures.png())),
                false
            )
        );
        assertEquals("INVALID_SIGNING_REQUEST", visibleWithoutWidget.code());
        SigningService.SigningException bomb = assertThrows(
            SigningService.SigningException.class,
            () -> SigningService.validateAppearance(oversizedPngHeader())
        );
        assertEquals("INVALID_APPEARANCE", bomb.code());
    }

    @Test
    void validatesAppearanceEntirelyInMemoryWithoutCreatingImageIoCacheFiles() throws Exception {
        Path cacheSentinel = Files.createTempDirectory("butter-paper-image-cache-sentinel-");
        String prior = System.getProperty("java.io.tmpdir");
        try {
            System.setProperty("java.io.tmpdir", cacheSentinel.toString());
            SigningService.validateAppearance(SigningTestFixtures.png());
            try (var entries = Files.list(cacheSentinel)) {
                assertEquals(List.of(), entries.toList());
            }
        } finally {
            if (prior == null) System.clearProperty("java.io.tmpdir");
            else System.setProperty("java.io.tmpdir", prior);
            Files.deleteIfExists(cacheSentinel);
        }
    }

    @Test
    void rejectsAnExistingVisibleWidgetOutsideItsCropBoxBeforeUnlockingTheIdentity() throws Exception {
        byte[] source = new SignatureFieldService().addField(
            SigningTestFixtures.pdf(),
            SignatureFieldSpec.parse(newField("OutsideExisting", widget(), null))
        );
        try (PDDocument document = Loader.loadPDF(source)) {
            document.getSignatureFields().getFirst().getWidgets().getFirst()
                .setRectangle(new PDRectangle(1_000, 1_000, 180, 60));
            ByteArrayOutputStream encoded = new ByteArrayOutputStream();
            document.saveIncremental(encoded);
            source = encoded.toByteArray();
        }
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(source);
        Map<String, byte[]> frames = new HashMap<>();
        frames.put("pkcs12", identity.pkcs12().clone());
        frames.put("appearance", SigningTestFixtures.png());
        SigningService.SigningException exception = assertThrows(
            SigningService.SigningException.class,
            () -> new SigningService(SigningTestFixtures.fixedPrompt(), CLOCK).sign(
                payload(workspace, identity, existingField("OutsideExisting"), true, null),
                frames,
                false
            )
        );
        assertEquals("FIELD_OUTSIDE_CROP_BOX", exception.code());
        assertEquals(0, Files.size(workspace.output()));
    }

    @Test
    void postcheckRejectsDuplicateFieldBindingMissingAppearanceAndNonExactContentsGap() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace visibleWorkspace = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        Map<String, byte[]> visibleFrames = new HashMap<>();
        visibleFrames.put("pkcs12", identity.pkcs12().clone());
        visibleFrames.put("appearance", SigningTestFixtures.png());
        new SigningService(SigningTestFixtures.fixedPrompt(), CLOCK).sign(
            payload(visibleWorkspace, identity, newField("Visible", widget(), null), true, null),
            visibleFrames,
            false
        );
        byte[] visible = Files.readAllBytes(visibleWorkspace.output());
        byte[] withoutAppearance = visible.clone();
        int appearanceName = lastIndexOf(withoutAppearance, "/AP".getBytes(StandardCharsets.US_ASCII));
        if (appearanceName < visibleWorkspace.sourceBytes().length) throw new AssertionError("appearance token missing");
        withoutAppearance[appearanceName + 2] = 'Q';
        SignedMutationPostcheck.PostcheckException missingAppearance = assertThrows(
            SignedMutationPostcheck.PostcheckException.class,
            () -> new SignedMutationPostcheck().verifySignature(
                    visibleWorkspace.sourceBytes(), withoutAppearance, "Visible", identity.fingerprint(), false, null,
                    new SignedMutationPostcheck.AppearanceExpectation(true, 0, 72f, 72f, 180f, 60f, 0), null
            )
        );
        assertEquals("VISIBLE_APPEARANCE_MISSING", missingAppearance.code());

        byte[] prepared = prepareInvisibleFields("Bound", "Alias");
        byte[] invisible = sign(prepared, identity, "Bound", false, null);
        SignedMutationPostcheck.PostcheckException binding = assertThrows(
            SignedMutationPostcheck.PostcheckException.class,
            () -> new SignedMutationPostcheck().verifySignature(
                prepared, invisible, "Alias", identity.fingerprint(), false, null, INVISIBLE, null
            )
        );
        assertEquals("SIGNED_FIELD_BINDING_MISMATCH", binding.code());

        try (PDDocument document = Loader.loadPDF(invisible)) {
            PDSignature signature = document.getSignatureDictionaries().getFirst();
            byte[] malformedGap = invisible.clone();
            malformedGap[signature.getByteRange()[1]] = '[';
            SignedMutationPostcheck.PostcheckException gap = assertThrows(
                SignedMutationPostcheck.PostcheckException.class,
                () -> SignedMutationPostcheck.assertExactContentsGap(malformedGap, document, signature)
            );
            assertEquals("INVALID_CONTENTS_GAP", gap.code());

            byte[] unrelated = invisible.clone();
            int contentsName = signature.getByteRange()[1] - "/Contents ".length();
            System.arraycopy("/OtherHex".getBytes(StandardCharsets.US_ASCII), 0, unrelated, contentsName, 9);
            SignedMutationPostcheck.PostcheckException unrelatedGap = assertThrows(
                SignedMutationPostcheck.PostcheckException.class,
                () -> SignedMutationPostcheck.assertExactContentsGap(unrelated, document, signature)
            );
            assertEquals("CONTENTS_GAP_NOT_SIGNATURE_CONTENTS", unrelatedGap.code());

            byte[] extraWhitespace = invisible.clone();
            extraWhitespace[signature.getByteRange()[1]] = ' ';
            SignedMutationPostcheck.PostcheckException whitespace = assertThrows(
                SignedMutationPostcheck.PostcheckException.class,
                () -> SignedMutationPostcheck.assertExactContentsGap(extraWhitespace, document, signature)
            );
            assertEquals("INVALID_CONTENTS_GAP", whitespace.code());
        }
    }

    @Test
    void postcheckRejectsWrongTypeEmptyAndStructurallyBrokenNormalAppearances() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(SigningTestFixtures.pdf());
        Map<String, byte[]> frames = new HashMap<>();
        frames.put("pkcs12", identity.pkcs12().clone());
        frames.put("appearance", SigningTestFixtures.png());
        new SigningService(SigningTestFixtures.fixedPrompt(), CLOCK).sign(
            payload(workspace, identity, newField("Visible", widget(), null), true, null),
            frames,
            false
        );
        byte[] signed = Files.readAllBytes(workspace.output());
        var expected = new SignedMutationPostcheck.AppearanceExpectation(true, 0, 72f, 72f, 180f, 60f, 0);

        try (PDDocument document = Loader.loadPDF(signed)) {
            var field = document.getSignatureFields().getFirst();
            COSDictionary appearance = (COSDictionary) field.getWidgets().getFirst().getCOSObject()
                .getDictionaryObject(COSName.AP);
            appearance.setItem(COSName.N, COSInteger.ZERO);
            assertPostcheckCode("VISIBLE_APPEARANCE_INVALID", document, field, expected);
        }
        try (PDDocument document = Loader.loadPDF(signed)) {
            var field = document.getSignatureFields().getFirst();
            COSDictionary appearance = (COSDictionary) field.getWidgets().getFirst().getCOSObject()
                .getDictionaryObject(COSName.AP);
            COSStream empty = document.getDocument().createCOSStream();
            empty.setName(COSName.TYPE, "XObject");
            empty.setName(COSName.SUBTYPE, "Form");
            empty.setItem(COSName.BBOX, new PDRectangle(0, 0, 180, 60).getCOSArray());
            empty.setItem(COSName.RESOURCES, new PDResources().getCOSObject());
            try (var ignored = empty.createOutputStream(COSName.FLATE_DECODE)) {
                // A compressed stream can have raw bytes while decoding to no appearance commands.
            }
            appearance.setItem(COSName.N, empty);
            assertPostcheckCode("VISIBLE_APPEARANCE_INVALID", document, field, expected);
        }
        try (PDDocument document = Loader.loadPDF(signed)) {
            var field = document.getSignatureFields().getFirst();
            COSDictionary appearance = (COSDictionary) field.getWidgets().getFirst().getCOSObject()
                .getDictionaryObject(COSName.AP);
            COSStream normal = (COSStream) appearance.getDictionaryObject(COSName.N);
            normal.removeItem(COSName.BBOX);
            assertPostcheckCode("VISIBLE_APPEARANCE_INVALID", document, field, expected);
        }
        try (PDDocument document = Loader.loadPDF(signed)) {
            var field = document.getSignatureFields().getFirst();
            var widget = field.getWidgets().getFirst();
            COSDictionary appearance = (COSDictionary) widget.getCOSObject().getDictionaryObject(COSName.AP);
            COSStream normal = (COSStream) appearance.getDictionaryObject(COSName.N);
            COSDictionary states = new COSDictionary();
            states.setItem(COSName.getPDFName("On"), normal);
            appearance.setItem(COSName.N, states);
            widget.getCOSObject().removeItem(COSName.AS);
            assertPostcheckCode("VISIBLE_APPEARANCE_INVALID", document, field, expected);
        }
        try (PDDocument document = Loader.loadPDF(signed)) {
            var field = document.getSignatureFields().getFirst();
            COSDictionary appearance = (COSDictionary) field.getWidgets().getFirst().getCOSObject()
                .getDictionaryObject(COSName.AP);
            COSStream normal = (COSStream) appearance.getDictionaryObject(COSName.N);
            normal.removeItem(COSName.SUBTYPE);
            assertPostcheckCode("VISIBLE_APPEARANCE_INVALID", document, field, expected);
        }
    }

    @Test
    void exactGapBindingRejectsAContentsTokenInAnotherObject() {
        byte[] bytes = ("1 0 obj\n"
            + "<< /Type /Sig /Contents 2 0 R /ByteRange [0 82 86 10] >>\n"
            + "endobj\n"
            + "2 0 obj\n<< /Contents <00> >>\nendobj\n").getBytes(StandardCharsets.US_ASCII);
        int start = indexOf(bytes, "<00>".getBytes(StandardCharsets.US_ASCII));
        SignedMutationPostcheck.PostcheckException exception = assertThrows(
            SignedMutationPostcheck.PostcheckException.class,
            () -> SignedMutationPostcheck.assertDirectSignatureObject(
                bytes, 0, 1, 0, start, start + 4, new int[]{0, 82, 86, 10}
            )
        );
        assertEquals("CONTENTS_GAP_NOT_SIGNATURE_CONTENTS", exception.code());
    }

    private static void assertPostcheckCode(
        String expectedCode,
        PDDocument document,
        org.apache.pdfbox.pdmodel.interactive.form.PDSignatureField field,
        SignedMutationPostcheck.AppearanceExpectation expected
    ) {
        SignedMutationPostcheck.PostcheckException exception = assertThrows(
            SignedMutationPostcheck.PostcheckException.class,
            () -> SignedMutationPostcheck.assertAppearance(document, field, expected)
        );
        assertEquals(expectedCode, exception.code());
    }

    private static void assertSourcePolicyRejected(
        byte[] source,
        SigningTestFixtures.IdentityFixture identity,
        String target
    ) throws Exception {
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(source);
        SigningService.SigningException exception = assertThrows(
            SigningService.SigningException.class,
            () -> new SigningService(SigningTestFixtures.fixedPrompt(), CLOCK).sign(
                payload(workspace, identity, existingField(target), false, null),
                new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
                false
            )
        );
        assertEquals("SOURCE_POLICY_INDETERMINATE", exception.code());
        assertEquals(0, Files.size(workspace.output()));
    }

    private static byte[] signedFieldLock(String action, List<String> names) throws Exception {
        byte[] source = SigningTestFixtures.pdf();
        SignatureFieldService fields = new SignatureFieldService();
        source = fields.addField(source, SignatureFieldSpec.parse(newField(
            "Locking", null, fieldLock(action, names)
        )));
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        return sign(source, identity, "Locking", false, null);
    }

    private static byte[] prepareLockedTarget(String action, List<String> names) throws Exception {
        byte[] source = SigningTestFixtures.pdf();
        SignatureFieldService fields = new SignatureFieldService();
        source = fields.addField(source, SignatureFieldSpec.parse(newField(
            "Locking", null, fieldLock(action, names)
        )));
        return fields.addField(source, SignatureFieldSpec.parse(newField("Target", null, null)));
    }

    private static void assertLockedByCanonicalPolicy(String action, List<String> names) throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        byte[] signed = sign(prepareLockedTarget(action, names), identity, "Locking", false, null);
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(signed);
        SigningService.SigningException exception = assertThrows(
            SigningService.SigningException.class,
            () -> new SigningService(SigningTestFixtures.fixedPrompt(), CLOCK).sign(
                payload(workspace, identity, existingField("Target"), false, null),
                new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
                false
            )
        );
        assertEquals("SIGNATURE_FIELD_LOCKED", exception.code());
    }

    @FunctionalInterface
    private interface PolicyMutation { void mutate(COSDictionary signature); }

    private static void assertMalformedRawPolicy(byte[] signed, PolicyMutation mutation) throws Exception {
        try (PDDocument document = Loader.loadPDF(signed)) {
            COSDictionary signature = document.getSignatureDictionaries().getFirst().getCOSObject();
            mutation.mutate(signature);
            SigningService.SigningException exception = assertThrows(
                SigningService.SigningException.class,
                () -> AuthoritativeSignedPolicies.assertRawPoliciesWellFormedForTest(signature)
            );
            assertEquals("SOURCE_POLICY_INDETERMINATE", exception.code());
        }
    }

    private static COSDictionary fieldTransform(COSDictionary signature) {
        COSArray references = (COSArray) signature.getDictionaryObject(COSName.getPDFName("Reference"));
        for (int index = 0; index < references.size(); index++) {
            COSDictionary reference = (COSDictionary) references.getObject(index);
            if ("FieldMDP".equals(reference.getNameAsString(COSName.getPDFName("TransformMethod")))) {
                return (COSDictionary) reference.getDictionaryObject(COSName.getPDFName("TransformParams"));
            }
        }
        throw new AssertionError("FieldMDP transform missing");
    }

    private static COSDictionary docTransform(COSDictionary signature) {
        COSArray references = (COSArray) signature.getDictionaryObject(COSName.getPDFName("Reference"));
        for (int index = 0; index < references.size(); index++) {
            COSDictionary reference = (COSDictionary) references.getObject(index);
            if ("DocMDP".equals(reference.getNameAsString(COSName.getPDFName("TransformMethod")))) {
                return (COSDictionary) reference.getDictionaryObject(COSName.getPDFName("TransformParams"));
            }
        }
        throw new AssertionError("DocMDP transform missing");
    }

    private static byte[] sign(
        byte[] source,
        SigningTestFixtures.IdentityFixture identity,
        String fieldName,
        boolean certification,
        String permission
    ) throws Exception {
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(source);
        new SigningService(SigningTestFixtures.fixedPrompt(), CLOCK).sign(
            payload(workspace, identity, existingField(fieldName), false, permission),
            new HashMap<>(Map.of("pkcs12", identity.pkcs12().clone())),
            certification
        );
        return Files.readAllBytes(workspace.output());
    }

    private static byte[] prepareInvisibleFields(String... names) throws Exception {
        byte[] source = SigningTestFixtures.pdf();
        SignatureFieldService service = new SignatureFieldService();
        for (String name : names) {
            source = service.addField(source, SignatureFieldSpec.parse(newField(name, null, null)));
        }
        return source;
    }

    private static byte[] removeCatalogPerms(byte[] source) throws Exception {
        try (PDDocument document = Loader.loadPDF(source)) {
            document.getDocumentCatalog().getCOSObject().removeItem(COSName.getPDFName("Perms"));
            ByteArrayOutputStream encoded = new ByteArrayOutputStream();
            document.saveIncremental(encoded);
            return encoded.toByteArray();
        }
    }

    private static ObjectNode payload(
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
        payload.put("appearance", visible ? "visible" : "invisible");
        if (permission != null) payload.put("certificationPermission", permission);
        return payload;
    }

    private static ObjectNode existingField(String name) {
        ObjectNode field = JSON.createObjectNode();
        field.put("kind", "existing");
        field.put("name", name);
        return field;
    }

    private static ObjectNode newField(String name, ObjectNode widget, ObjectNode lock) {
        ObjectNode field = JSON.createObjectNode();
        field.put("kind", "new");
        field.put("name", name);
        if (widget == null) field.putNull("widget"); else field.set("widget", widget);
        if (lock != null) field.set("lock", lock);
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

    private static byte[] oversizedPngHeader() throws Exception {
        ByteArrayOutputStream encoded = new ByteArrayOutputStream();
        DataOutputStream output = new DataOutputStream(encoded);
        output.write(new byte[]{(byte) 0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a});
        ByteArrayOutputStream ihdr = new ByteArrayOutputStream();
        DataOutputStream data = new DataOutputStream(ihdr);
        data.writeInt(100_000);
        data.writeInt(100_000);
        data.write(new byte[]{8, 6, 0, 0, 0});
        writePngChunk(output, "IHDR", ihdr.toByteArray());
        writePngChunk(output, "IEND", new byte[0]);
        return encoded.toByteArray();
    }

    private static void writePngChunk(DataOutputStream output, String name, byte[] data) throws Exception {
        byte[] type = name.getBytes(StandardCharsets.US_ASCII);
        output.writeInt(data.length);
        output.write(type);
        output.write(data);
        CRC32 crc = new CRC32();
        crc.update(type);
        crc.update(data);
        output.writeInt((int) crc.getValue());
    }

    private static int lastIndexOf(byte[] value, byte[] needle) {
        outer: for (int start = value.length - needle.length; start >= 0; start--) {
            for (int index = 0; index < needle.length; index++) {
                if (value[start + index] != needle[index]) continue outer;
            }
            return start;
        }
        return -1;
    }

    private static int indexOf(byte[] value, byte[] needle) {
        outer: for (int start = 0; start <= value.length - needle.length; start++) {
            for (int index = 0; index < needle.length; index++) {
                if (value[start + index] != needle[index]) continue outer;
            }
            return start;
        }
        return -1;
    }
}
