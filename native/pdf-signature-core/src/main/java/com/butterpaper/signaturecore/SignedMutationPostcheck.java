package com.butterpaper.signaturecore;

import eu.europa.esig.dss.model.InMemoryDocument;
import eu.europa.esig.dss.pades.validation.ByteRange;
import eu.europa.esig.dss.pades.validation.PAdESSignature;
import eu.europa.esig.dss.pades.validation.PDFDocumentValidator;
import eu.europa.esig.dss.spi.signature.AdvancedSignature;
import org.apache.pdfbox.Loader;
import org.apache.pdfbox.cos.COSArray;
import org.apache.pdfbox.cos.COSBase;
import org.apache.pdfbox.cos.COSDictionary;
import org.apache.pdfbox.cos.COSInteger;
import org.apache.pdfbox.cos.COSName;
import org.apache.pdfbox.cos.COSNumber;
import org.apache.pdfbox.cos.COSObject;
import org.apache.pdfbox.cos.COSObjectKey;
import org.apache.pdfbox.cos.COSStream;
import org.apache.pdfbox.pdmodel.PDDocument;
import org.apache.pdfbox.pdmodel.PDPage;
import org.apache.pdfbox.pdmodel.common.PDRectangle;
import org.apache.pdfbox.pdmodel.interactive.annotation.PDAnnotationWidget;
import org.apache.pdfbox.pdmodel.interactive.form.PDSignatureField;
import org.apache.pdfbox.pdmodel.interactive.digitalsignature.PDSignature;

import java.io.IOException;
import java.io.InputStream;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

final class SignedMutationPostcheck {
    record AppearanceExpectation(
        boolean visible,
        Integer pageIndex,
        Float x,
        Float y,
        Float width,
        Float height,
        int pageRotation
    ) {}

    static final class PostcheckException extends Exception {
        private final String code;

        PostcheckException(String code) {
            super(code);
            this.code = code;
        }

        String code() { return code; }
    }

    Map<String, Object> verifySignature(
        byte[] source,
        byte[] output,
        String expectedFieldName,
        String expectedCertificateSha256,
        boolean certification,
        Integer expectedCertificationPermission,
        AppearanceExpectation appearance,
        SignatureFieldSpec.FieldLock expectedFieldLock
    ) throws PostcheckException {
        try {
            SafePdfMutation.assertAppendOnly(source, output);
        } catch (SafePdfMutation.MutationException exception) {
            throw new PostcheckException(exception.code());
        }
        try (PDDocument before = Loader.loadPDF(source); PDDocument after = Loader.loadPDF(output)) {
            List<PDSignature> previous = before.getSignatureDictionaries();
            List<PDSignature> current = after.getSignatureDictionaries();
            if (current.size() != previous.size() + 1) throw new PostcheckException("SIGNATURE_COUNT_MISMATCH");
            boolean[] matchedCurrent = new boolean[current.size()];
            for (PDSignature left : previous) {
                int match = -1;
                for (int index = 0; index < current.size(); index++) {
                    if (!matchedCurrent[index] && sameSignature(left, current.get(index))) {
                        if (match != -1) throw new PostcheckException("PRIOR_SIGNATURE_AMBIGUOUS");
                        match = index;
                    }
                }
                if (match == -1) throw new PostcheckException("PRIOR_SIGNATURE_CHANGED");
                matchedCurrent[match] = true;
            }
            PDSignature added = null;
            for (int index = 0; index < current.size(); index++) {
                if (!matchedCurrent[index]) {
                    if (added != null) throw new PostcheckException("SIGNATURE_COUNT_MISMATCH");
                    added = current.get(index);
                }
            }
            if (added == null) throw new PostcheckException("SIGNATURE_COUNT_MISMATCH");
            PDSignature addedSignature = added;
            int[] byteRange = added.getByteRange();
            if (byteRange == null || byteRange.length != 4 || byteRange[0] != 0
                || byteRange[1] < 1 || byteRange[2] <= byteRange[1]
                || (long) byteRange[2] + byteRange[3] != output.length) {
                throw new PostcheckException("INVALID_NEW_BYTE_RANGE");
            }
            assertExactContentsGap(output, after, added);
            List<PDSignatureField> namedFields = after.getSignatureFields().stream()
                .filter(candidate -> expectedFieldName.equals(candidate.getFullyQualifiedName()))
                .toList();
            if (namedFields.size() != 1) throw new PostcheckException("SIGNED_FIELD_AMBIGUOUS");
            List<PDSignatureField> sourceFields = before.getSignatureFields().stream()
                .filter(candidate -> expectedFieldName.equals(candidate.getFullyQualifiedName()))
                .toList();
            if (sourceFields.size() > 1
                || (!sourceFields.isEmpty() && sourceFields.getFirst().getSignature() != null)
                || (!previous.isEmpty() && sourceFields.size() != 1)) {
                throw new PostcheckException("SIGNED_FIELD_SOURCE_MISMATCH");
            }
            PDSignatureField field = namedFields.getFirst();
            if (field.getSignature() == null || field.getSignature().getCOSObject() != added.getCOSObject()) {
                throw new PostcheckException("SIGNED_FIELD_BINDING_MISMATCH");
            }
            long fieldsBoundToAdded = after.getSignatureFields().stream()
                .filter(candidate -> candidate.getSignature() != null
                    && candidate.getSignature().getCOSObject() == addedSignature.getCOSObject())
                .count();
            if (fieldsBoundToAdded != 1) throw new PostcheckException("SIGNED_FIELD_BINDING_MISMATCH");
            assertAppearance(after, field, appearance);

            COSDictionary catalog = after.getDocumentCatalog().getCOSObject();
            COSDictionary perms = asDictionary(catalog.getDictionaryObject(COSName.getPDFName("Perms")));
            COSDictionary docMdp = perms == null ? null
                : asDictionary(perms.getDictionaryObject(COSName.getPDFName("DocMDP")));
            Integer observedPermission = docMdpPermission(added.getCOSObject());
            if (certification) {
                if (docMdp == null || docMdp != added.getCOSObject()
                    || !expectedCertificationPermission.equals(observedPermission)) {
                    throw new PostcheckException("CERTIFICATION_MISMATCH");
                }
            } else if (docMdp == added.getCOSObject() || observedPermission != null) {
                throw new PostcheckException("UNEXPECTED_CERTIFICATION");
            }
            try {
                AuthoritativeSignedPolicies.assertAddedSignaturePolicies(
                    added.getCOSObject(),
                    expectedCertificationPermission,
                    expectedFieldLock
                );
            } catch (SigningService.SigningException exception) {
                throw new PostcheckException("SIGNATURE_POLICY_MISMATCH");
            }

            PDFDocumentValidator validator = new PDFDocumentValidator(new InMemoryDocument(output));
            validator.setCertificateVerifier(ValidationService.newOfflineVerifier());
            List<AdvancedSignature> advanced = new ArrayList<>(validator.getSignatures());
            if (advanced.size() != current.size()) throw new PostcheckException("VALIDATION_SIGNATURE_COUNT_MISMATCH");
            PAdESSignature addedAdvanced = null;
            for (AdvancedSignature signature : advanced) {
                signature.checkSignatureIntegrity();
                if (!signature.getSignatureCryptographicVerification().isSignatureValid()) {
                    throw new PostcheckException("OUTPUT_SIGNATURE_INVALID");
                }
                if (signature instanceof PAdESSignature pades
                    && matches(pades, byteRange)) {
                    if (addedAdvanced != null) throw new PostcheckException("ADVANCED_SIGNATURE_AMBIGUOUS");
                    addedAdvanced = pades;
                }
            }
            if (addedAdvanced == null) throw new PostcheckException("ADVANCED_SIGNATURE_BINDING_MISMATCH");
            if (addedAdvanced.getSigningCertificateToken() == null
                || !Pkcs12IdentityService.fingerprint(addedAdvanced.getSigningCertificateToken())
                    .equals(expectedCertificateSha256)) {
                throw new PostcheckException("SIGNING_CERTIFICATE_MISMATCH");
            }

            Map<String, Object> result = new LinkedHashMap<>();
            result.put("appendOnly", true);
            result.put("sourceBytesPreserved", source.length);
            result.put("appendedBytes", output.length - source.length);
            result.put("priorSignatureCount", previous.size());
            result.put("outputSignatureCount", current.size());
            result.put("newSignatureByteRange", List.of(byteRange[0], byteRange[1], byteRange[2], byteRange[3]));
            result.put("wholeRevisionCovered", true);
            result.put("contentsGapExact", true);
            result.put("fieldBindingExact", true);
            result.put("advancedSignatureBindingExact", true);
            result.put("fieldName", expectedFieldName);
            result.put("certificateSha256", expectedCertificateSha256);
            result.put("certificationPermission", observedPermission);
            result.put("cryptographicIntegrity", "intact");
            return result;
        } catch (PostcheckException exception) {
            throw exception;
        } catch (Pkcs12IdentityService.IdentityException exception) {
            throw new PostcheckException(exception.code());
        } catch (IOException | RuntimeException exception) {
            throw new PostcheckException("OUTPUT_VALIDATION_FAILED");
        }
    }

    void verifyIndependent(
        byte[] source,
        byte[] output,
        String expectedFieldName,
        String expectedCertificateSha256,
        String expectedOperation,
        String expectedAppearance,
        Integer expectedCertificationPermission
    )
        throws PostcheckException {
        if (expectedCertificateSha256 == null || !expectedCertificateSha256.matches("[a-f0-9]{64}")) {
            throw new PostcheckException("INVALID_POSTVALIDATION_REQUEST");
        }
        boolean certification = switch (expectedOperation == null ? "" : expectedOperation) {
            case "approval" -> false;
            case "certification" -> true;
            default -> throw new PostcheckException("INVALID_POSTVALIDATION_REQUEST");
        };
        boolean visible = switch (expectedAppearance == null ? "" : expectedAppearance) {
            case "visible" -> true;
            case "invisible" -> false;
            default -> throw new PostcheckException("INVALID_POSTVALIDATION_REQUEST");
        };
        if (certification != (expectedCertificationPermission != null)) {
            throw new PostcheckException("INVALID_POSTVALIDATION_REQUEST");
        }
        if (expectedCertificationPermission != null
            && (expectedCertificationPermission < 1 || expectedCertificationPermission > 3)) {
            throw new PostcheckException("INVALID_POSTVALIDATION_REQUEST");
        }
        try {
            SafePdfMutation.assertAppendOnly(source, output);
        } catch (SafePdfMutation.MutationException exception) {
            throw new PostcheckException(exception.code());
        }
        try (PDDocument before = Loader.loadPDF(source); PDDocument after = Loader.loadPDF(output)) {
            List<PDSignature> previous = before.getSignatureDictionaries();
            List<PDSignature> current = after.getSignatureDictionaries();
            if (current.size() != previous.size() + 1) {
                throw new PostcheckException("SIGNATURE_COUNT_MISMATCH");
            }
            boolean[] matchedCurrent = new boolean[current.size()];
            for (PDSignature left : previous) {
                int match = -1;
                for (int index = 0; index < current.size(); index++) {
                    if (!matchedCurrent[index] && sameSignature(left, current.get(index))) {
                        if (match != -1) throw new PostcheckException("PRIOR_SIGNATURE_AMBIGUOUS");
                        match = index;
                    }
                }
                if (match == -1) throw new PostcheckException("PRIOR_SIGNATURE_CHANGED");
                matchedCurrent[match] = true;
            }
            PDSignature added = null;
            for (int index = 0; index < current.size(); index++) {
                if (!matchedCurrent[index]) {
                    if (added != null) throw new PostcheckException("SIGNATURE_COUNT_MISMATCH");
                    added = current.get(index);
                }
            }
            if (added == null) throw new PostcheckException("SIGNATURE_COUNT_MISMATCH");
            PDSignature addedSignature = added;
            int[] byteRange = added.getByteRange();
            if (byteRange == null || byteRange.length != 4 || byteRange[0] != 0
                || byteRange[1] < 1 || byteRange[2] <= byteRange[1]
                || (long) byteRange[2] + byteRange[3] != output.length) {
                throw new PostcheckException("INVALID_NEW_BYTE_RANGE");
            }
            assertExactContentsGap(output, after, added);

            List<PDSignatureField> namedFields = after.getSignatureFields().stream()
                .filter(candidate -> expectedFieldName.equals(candidate.getFullyQualifiedName()))
                .toList();
            if (namedFields.size() != 1) throw new PostcheckException("SIGNED_FIELD_AMBIGUOUS");
            List<PDSignatureField> sourceFields = before.getSignatureFields().stream()
                .filter(candidate -> expectedFieldName.equals(candidate.getFullyQualifiedName()))
                .toList();
            if (sourceFields.size() > 1
                || (!sourceFields.isEmpty() && sourceFields.getFirst().getSignature() != null)
                || (!previous.isEmpty() && sourceFields.size() != 1)) {
                throw new PostcheckException("SIGNED_FIELD_SOURCE_MISMATCH");
            }
            PDSignatureField field = namedFields.getFirst();
            if (field.getSignature() == null || field.getSignature().getCOSObject() != added.getCOSObject()) {
                throw new PostcheckException("SIGNED_FIELD_BINDING_MISMATCH");
            }
            long fieldsBoundToAdded = after.getSignatureFields().stream()
                .filter(candidate -> candidate.getSignature() != null
                    && candidate.getSignature().getCOSObject() == addedSignature.getCOSObject())
                .count();
            if (fieldsBoundToAdded != 1) throw new PostcheckException("SIGNED_FIELD_BINDING_MISMATCH");
            AppearanceExpectation observedAppearance = inferAppearance(after, field);
            if (observedAppearance.visible() != visible) {
                throw new PostcheckException("APPEARANCE_MODE_MISMATCH");
            }
            assertAppearance(after, field, observedAppearance);

            COSDictionary beforeDocMdp = catalogDocMdp(before);
            COSDictionary afterDocMdp = catalogDocMdp(after);
            if (beforeDocMdp == null) {
                if (afterDocMdp != null && afterDocMdp != added.getCOSObject()) {
                    throw new PostcheckException("CERTIFICATION_MISMATCH");
                }
            } else if (afterDocMdp == null
                || !sameSignature(new PDSignature(beforeDocMdp), new PDSignature(afterDocMdp))) {
                throw new PostcheckException("PRIOR_SIGNATURE_CHANGED");
            }
            Integer observedPermission = docMdpPermission(added.getCOSObject());
            Integer expectedPermission = afterDocMdp == added.getCOSObject() ? observedPermission : null;
            if ((afterDocMdp == added.getCOSObject()) != (observedPermission != null)) {
                throw new PostcheckException("CERTIFICATION_MISMATCH");
            }
            if (certification) {
                if (afterDocMdp != added.getCOSObject()
                    || !expectedCertificationPermission.equals(observedPermission)) {
                    throw new PostcheckException("CERTIFICATION_MISMATCH");
                }
            } else if (afterDocMdp == added.getCOSObject() || observedPermission != null) {
                throw new PostcheckException("UNEXPECTED_CERTIFICATION");
            }
            SignatureFieldSpec.FieldLock fieldLock;
            try {
                fieldLock = SignatureFieldService.fieldLock(field);
                AuthoritativeSignedPolicies.assertAddedSignaturePolicies(
                    added.getCOSObject(), expectedPermission, fieldLock
                );
                if (!previous.isEmpty()) {
                    AuthoritativeSignedPolicies.assertAllowsApproval(source, expectedFieldName);
                }
            } catch (SignatureFieldService.FieldException | SigningService.SigningException exception) {
                throw new PostcheckException("SIGNATURE_POLICY_MISMATCH");
            }

            PDFDocumentValidator validator = new PDFDocumentValidator(new InMemoryDocument(output));
            validator.setCertificateVerifier(ValidationService.newOfflineVerifier());
            List<AdvancedSignature> advanced = new ArrayList<>(validator.getSignatures());
            if (advanced.size() != current.size()) {
                throw new PostcheckException("VALIDATION_SIGNATURE_COUNT_MISMATCH");
            }
            PAdESSignature addedAdvanced = null;
            for (AdvancedSignature signature : advanced) {
                signature.checkSignatureIntegrity();
                if (!signature.getSignatureCryptographicVerification().isSignatureValid()) {
                    throw new PostcheckException("OUTPUT_SIGNATURE_INVALID");
                }
                if (signature instanceof PAdESSignature pades && matches(pades, byteRange)) {
                    if (addedAdvanced != null) {
                        throw new PostcheckException("ADVANCED_SIGNATURE_AMBIGUOUS");
                    }
                    addedAdvanced = pades;
                }
            }
            if (addedAdvanced == null || addedAdvanced.getSigningCertificateToken() == null) {
                throw new PostcheckException("ADVANCED_SIGNATURE_BINDING_MISMATCH");
            }
            try {
                if (!Pkcs12IdentityService.fingerprint(addedAdvanced.getSigningCertificateToken())
                    .equals(expectedCertificateSha256)) {
                    throw new PostcheckException("SIGNING_CERTIFICATE_MISMATCH");
                }
            } catch (Pkcs12IdentityService.IdentityException exception) {
                throw new PostcheckException(exception.code());
            }
        } catch (PostcheckException exception) {
            throw exception;
        } catch (IOException | RuntimeException exception) {
            throw new PostcheckException("OUTPUT_VALIDATION_FAILED");
        }
    }

    private static AppearanceExpectation inferAppearance(PDDocument document, PDSignatureField field)
        throws PostcheckException {
        List<PDAnnotationWidget> effective = field.getWidgets().stream()
            .filter(widget -> widget.getRectangle() != null
                && widget.getRectangle().getWidth() > 0
                && widget.getRectangle().getHeight() > 0)
            .toList();
        if (effective.isEmpty()) {
            return new AppearanceExpectation(false, null, null, null, null, null, 0);
        }
        if (effective.size() != 1 || effective.getFirst().getPage() == null) {
            throw new PostcheckException("VISIBLE_WIDGET_MISMATCH");
        }
        PDAnnotationWidget widget = effective.getFirst();
        PDPage page = widget.getPage();
        int pageIndex = 0;
        boolean found = false;
        for (PDPage candidate : document.getPages()) {
            if (candidate == page || candidate.getCOSObject() == page.getCOSObject()) {
                found = true;
                break;
            }
            pageIndex++;
        }
        if (!found) throw new PostcheckException("VISIBLE_WIDGET_MISMATCH");
        PDRectangle rectangle = widget.getRectangle();
        try {
            SignatureFieldService.assertGeometry(page, rectangle, null);
        } catch (SignatureFieldService.FieldException exception) {
            throw new PostcheckException(exception.code());
        }
        return new AppearanceExpectation(
            true,
            pageIndex,
            rectangle.getLowerLeftX(),
            rectangle.getLowerLeftY(),
            rectangle.getWidth(),
            rectangle.getHeight(),
            ((page.getRotation() % 360) + 360) % 360
        );
    }

    private static COSDictionary catalogDocMdp(PDDocument document) throws PostcheckException {
        COSBase rawPerms = document.getDocumentCatalog().getCOSObject()
            .getDictionaryObject(COSName.getPDFName("Perms"));
        if (rawPerms == null) return null;
        if (!(rawPerms instanceof COSDictionary permissions)) {
            throw new PostcheckException("CERTIFICATION_MISMATCH");
        }
        COSBase rawDocMdp = permissions.getDictionaryObject(COSName.getPDFName("DocMDP"));
        if (rawDocMdp == null) return null;
        if (!(rawDocMdp instanceof COSDictionary dictionary)) {
            throw new PostcheckException("CERTIFICATION_MISMATCH");
        }
        long matches = document.getSignatureDictionaries().stream()
            .filter(signature -> signature.getCOSObject() == dictionary)
            .count();
        if (matches != 1) throw new PostcheckException("CERTIFICATION_MISMATCH");
        return dictionary;
    }

    static void assertExactContentsGap(byte[] output, PDDocument document, PDSignature signature)
        throws PostcheckException {
        int[] range = signature.getByteRange();
        if (range == null || range.length != 4 || range[1] < 0 || range[2] > output.length
            || range[0] != 0 || range[1] > range[2] || range[3] < 0
            || (long) range[2] + range[3] != output.length || range[2] <= range[1] + 2) {
            throw new PostcheckException("INVALID_CONTENTS_GAP");
        }
        int start = range[1];
        int end = range[2];
        if (output[start] != '<' || output[end - 1] != '>' || ((end - start - 2) & 1) != 0) {
            throw new PostcheckException("INVALID_CONTENTS_GAP");
        }
        COSObjectKey objectKey = null;
        for (COSObjectKey key : document.getDocument().getXrefTable().keySet()) {
            COSObject candidate = document.getDocument().getObjectFromPool(key);
            if (candidate.getObject() == signature.getCOSObject()) {
                if (objectKey != null) throw new PostcheckException("CONTENTS_GAP_NOT_SIGNATURE_CONTENTS");
                objectKey = key;
            }
        }
        Long rawOffset = objectKey == null ? null : document.getDocument().getXrefTable().get(objectKey);
        if (rawOffset == null || rawOffset < 0 || rawOffset > Integer.MAX_VALUE) {
            throw new PostcheckException("CONTENTS_GAP_NOT_SIGNATURE_CONTENTS");
        }
        assertDirectSignatureObject(
            output,
            rawOffset.intValue(),
            objectKey.getNumber(),
            objectKey.getGeneration(),
            start,
            end,
            range
        );
        byte[] decoded = new byte[(end - start - 2) / 2];
        try {
            for (int index = 0; index < decoded.length; index++) {
                int high = Character.digit((char) output[start + 1 + index * 2], 16);
                int low = Character.digit((char) output[start + 2 + index * 2], 16);
                if (high < 0 || low < 0) throw new PostcheckException("INVALID_CONTENTS_GAP");
                decoded[index] = (byte) ((high << 4) | low);
            }
            if (!MessageDigest.isEqual(decoded, signature.getContents())) {
                throw new PostcheckException("CONTENTS_GAP_MISMATCH");
            }
        } finally {
            Arrays.fill(decoded, (byte) 0);
        }
    }

    static void assertDirectSignatureObject(
        byte[] output,
        int objectOffset,
        long objectNumber,
        int generation,
        int contentsStart,
        int contentsEnd,
        int[] expectedRange
    ) throws PostcheckException {
        new PdfObjectCursor(output, objectOffset).assertSignatureDictionary(
            objectNumber, generation, contentsStart, contentsEnd, expectedRange
        );
    }

    private static final class PdfObjectCursor {
        private final byte[] bytes;
        private int cursor;

        PdfObjectCursor(byte[] bytes, int offset) throws PostcheckException {
            this.bytes = bytes;
            if (offset < 0 || offset >= bytes.length) fail();
            this.cursor = offset;
        }

        void assertSignatureDictionary(
            long objectNumber,
            int generation,
            int contentsStart,
            int contentsEnd,
            int[] expectedRange
        ) throws PostcheckException {
            skipSpaceAndComments();
            if (readUnsignedLong() != objectNumber) fail();
            skipSpaceAndComments();
            if (readUnsignedLong() != generation) fail();
            skipSpaceAndComments();
            expectWord("obj");
            skipSpaceAndComments();
            expect("<<");
            boolean foundContents = false;
            boolean foundByteRange = false;
            while (true) {
                skipSpaceAndComments();
                if (matches(">>")) {
                    cursor += 2;
                    break;
                }
                String key = readName();
                skipSpaceAndComments();
                if ("Contents".equals(key)) {
                    if (foundContents || cursor != contentsStart) fail();
                    foundContents = true;
                    skipExactHexString(contentsEnd);
                } else if ("ByteRange".equals(key)) {
                    if (foundByteRange) fail();
                    foundByteRange = true;
                    if (!Arrays.equals(readByteRange(), expectedRange)) fail();
                } else {
                    skipValue(0);
                }
            }
            if (!foundContents || !foundByteRange) fail();
            skipSpaceAndComments();
            expectWord("endobj");
        }

        private int[] readByteRange() throws PostcheckException {
            expect("[");
            int[] values = new int[4];
            for (int index = 0; index < values.length; index++) {
                skipSpaceAndComments();
                long value = readUnsignedLong();
                if (value > Integer.MAX_VALUE) fail();
                values[index] = (int) value;
            }
            skipSpaceAndComments();
            expect("]");
            return values;
        }

        private void skipExactHexString(int expectedEnd) throws PostcheckException {
            if (cursor >= bytes.length || bytes[cursor++] != '<') fail();
            int digits = 0;
            while (cursor < bytes.length && bytes[cursor] != '>') {
                if (Character.digit((char) bytes[cursor++], 16) < 0) fail();
                digits++;
            }
            if (cursor >= bytes.length || bytes[cursor++] != '>' || (digits & 1) != 0 || cursor != expectedEnd) fail();
        }

        private void skipValue(int depth) throws PostcheckException {
            if (depth > 64) fail();
            skipSpaceAndComments();
            if (matches("<<")) {
                cursor += 2;
                while (true) {
                    skipSpaceAndComments();
                    if (matches(">>")) { cursor += 2; return; }
                    readName();
                    skipValue(depth + 1);
                }
            }
            if (cursor >= bytes.length) fail();
            if (bytes[cursor] == '[') {
                cursor++;
                while (true) {
                    skipSpaceAndComments();
                    if (cursor >= bytes.length) fail();
                    if (bytes[cursor] == ']') { cursor++; return; }
                    skipValue(depth + 1);
                }
            }
            if (bytes[cursor] == '(') { skipLiteralString(); return; }
            if (bytes[cursor] == '<') { skipGenericHexString(); return; }
            if (bytes[cursor] == '/') { readName(); return; }
            int firstStart = cursor;
            skipToken();
            if (firstStart == cursor || !isNumberToken(firstStart)) return;
            int afterFirst = cursor;
            skipSpaceAndComments();
            int secondStart = cursor;
            skipToken();
            int secondEnd = cursor;
            if (secondStart == secondEnd || !isUnsignedToken(secondStart, secondEnd)) {
                cursor = afterFirst;
                return;
            }
            skipSpaceAndComments();
            int markerStart = cursor;
            skipToken();
            int markerEnd = cursor;
            if (markerEnd - markerStart != 1 || bytes[markerStart] != 'R') cursor = afterFirst;
        }

        private void skipToken() {
            while (cursor < bytes.length && !isDelimiter(bytes[cursor])) cursor++;
        }

        private boolean isNumberToken(int start) {
            int end = cursor;
            if (start >= end) return false;
            int index = start;
            if (bytes[index] == '+' || bytes[index] == '-') index++;
            boolean digit = false;
            boolean dot = false;
            for (; index < end; index++) {
                if (bytes[index] >= '0' && bytes[index] <= '9') { digit = true; continue; }
                if (bytes[index] == '.' && !dot) { dot = true; continue; }
                return false;
            }
            return digit;
        }

        private boolean isUnsignedToken(int start, int end) {
            if (start >= end) return false;
            for (int index = start; index < end; index++) {
                if (bytes[index] < '0' || bytes[index] > '9') return false;
            }
            return true;
        }

        private void skipLiteralString() throws PostcheckException {
            int nesting = 0;
            do {
                if (cursor >= bytes.length) fail();
                byte value = bytes[cursor++];
                if (value == '\\') {
                    if (cursor < bytes.length) {
                        if (bytes[cursor] == '\r') { cursor++; if (cursor < bytes.length && bytes[cursor] == '\n') cursor++; }
                        else cursor++;
                    }
                } else if (value == '(') nesting++;
                else if (value == ')') nesting--;
            } while (nesting > 0);
            if (nesting != 0) fail();
        }

        private void skipGenericHexString() throws PostcheckException {
            cursor++;
            while (cursor < bytes.length && bytes[cursor] != '>') {
                if (!isWhitespace(bytes[cursor]) && Character.digit((char) bytes[cursor], 16) < 0) fail();
                cursor++;
            }
            if (cursor >= bytes.length) fail();
            cursor++;
        }

        private String readName() throws PostcheckException {
            if (cursor >= bytes.length || bytes[cursor++] != '/') fail();
            int start = cursor;
            while (cursor < bytes.length && !isDelimiter(bytes[cursor])) cursor++;
            if (cursor == start) fail();
            return new String(bytes, start, cursor - start, java.nio.charset.StandardCharsets.US_ASCII);
        }

        private long readUnsignedLong() throws PostcheckException {
            if (cursor >= bytes.length || bytes[cursor] < '0' || bytes[cursor] > '9') fail();
            long value = 0;
            while (cursor < bytes.length && bytes[cursor] >= '0' && bytes[cursor] <= '9') {
                int digit = bytes[cursor++] - '0';
                if (value > (Long.MAX_VALUE - digit) / 10) fail();
                value = value * 10 + digit;
            }
            return value;
        }

        private void skipSpaceAndComments() {
            while (cursor < bytes.length) {
                if (isWhitespace(bytes[cursor])) { cursor++; continue; }
                if (bytes[cursor] != '%') return;
                while (cursor < bytes.length && bytes[cursor] != '\r' && bytes[cursor] != '\n') cursor++;
            }
        }

        private void expect(String value) throws PostcheckException {
            if (!matches(value)) fail();
            cursor += value.length();
        }

        private void expectWord(String value) throws PostcheckException {
            expect(value);
            if (cursor < bytes.length && !isDelimiter(bytes[cursor])) fail();
        }

        private boolean matches(String value) {
            if (cursor + value.length() > bytes.length) return false;
            for (int index = 0; index < value.length(); index++) {
                if (bytes[cursor + index] != (byte) value.charAt(index)) return false;
            }
            return true;
        }

        private static boolean isDelimiter(byte value) {
            return isWhitespace(value) || switch (value) {
                case '(', ')', '<', '>', '[', ']', '{', '}', '/', '%' -> true;
                default -> false;
            };
        }

        private static boolean isWhitespace(byte value) {
            return switch (value) {
                case 0, 9, 10, 12, 13, 32 -> true;
                default -> false;
            };
        }

        private static void fail() throws PostcheckException {
            throw new PostcheckException("CONTENTS_GAP_NOT_SIGNATURE_CONTENTS");
        }
    }

    private static boolean sameSignature(PDSignature left, PDSignature right) {
        return Arrays.equals(left.getByteRange(), right.getByteRange())
            && MessageDigest.isEqual(left.getContents(), right.getContents());
    }

    private static boolean matches(PAdESSignature signature, int[] expectedRange) {
        ByteRange range = signature.getPdfSignatureDictionary().getByteRange();
        return range != null
            && range.toBigIntegerList().equals(List.of(
                java.math.BigInteger.valueOf(expectedRange[0]),
                java.math.BigInteger.valueOf(expectedRange[1]),
                java.math.BigInteger.valueOf(expectedRange[2]),
                java.math.BigInteger.valueOf(expectedRange[3])
            ));
    }

    static void assertAppearance(
        PDDocument document,
        PDSignatureField field,
        AppearanceExpectation expected
    ) throws PostcheckException {
        List<PDAnnotationWidget> effective = field.getWidgets().stream()
            .filter(widget -> widget.getRectangle() != null
                && widget.getRectangle().getWidth() > 0
                && widget.getRectangle().getHeight() > 0)
            .toList();
        if (!expected.visible()) {
            if (!effective.isEmpty()) {
                throw new PostcheckException("UNEXPECTED_VISIBLE_APPEARANCE");
            }
            return;
        }
        if (effective.size() != 1) throw new PostcheckException("VISIBLE_WIDGET_MISMATCH");
        PDAnnotationWidget widget = effective.getFirst();
        PDPage page = widget.getPage();
        if (page == null) throw new PostcheckException("VISIBLE_WIDGET_MISMATCH");
        int pageIndex = 0;
        boolean found = false;
        for (PDPage candidate : document.getPages()) {
            if (candidate == page || candidate.getCOSObject() == page.getCOSObject()) {
                found = true;
                break;
            }
            pageIndex++;
        }
        PDRectangle rectangle = widget.getRectangle();
        int rotation = ((page.getRotation() % 360) + 360) % 360;
        if (!found || pageIndex != expected.pageIndex()
            || Float.compare(rectangle.getLowerLeftX(), expected.x()) != 0
            || Float.compare(rectangle.getLowerLeftY(), expected.y()) != 0
            || Float.compare(rectangle.getWidth(), expected.width()) != 0
            || Float.compare(rectangle.getHeight(), expected.height()) != 0
            || rotation != expected.pageRotation()) {
            throw new PostcheckException("VISIBLE_WIDGET_MISMATCH");
        }
        COSDictionary appearance = asDictionary(widget.getCOSObject().getDictionaryObject(COSName.AP));
        if (appearance == null) {
            throw new PostcheckException("VISIBLE_APPEARANCE_MISSING");
        }
        COSBase normal = appearance.getDictionaryObject(COSName.N);
        if (normal instanceof COSStream stream) {
            assertAppearanceStream(stream);
        } else if (normal instanceof COSDictionary states) {
            if (states.size() == 0 || states.size() > 256) {
                throw new PostcheckException("VISIBLE_APPEARANCE_INVALID");
            }
            for (COSName state : states.keySet()) {
                COSBase value = states.getDictionaryObject(state);
                if (!(value instanceof COSStream stream)) {
                    throw new PostcheckException("VISIBLE_APPEARANCE_INVALID");
                }
                assertAppearanceStream(stream);
            }
            COSBase selected = widget.getCOSObject().getDictionaryObject(COSName.AS);
            if (!(selected instanceof COSName state)
                || !(states.getDictionaryObject(state) instanceof COSStream)) {
                throw new PostcheckException("VISIBLE_APPEARANCE_INVALID");
            }
        } else if (normal == null) {
            throw new PostcheckException("VISIBLE_APPEARANCE_MISSING");
        } else {
            throw new PostcheckException("VISIBLE_APPEARANCE_INVALID");
        }
    }

    private static void assertAppearanceStream(COSStream stream) throws PostcheckException {
        COSArray box = asArray(stream.getDictionaryObject(COSName.BBOX));
        COSDictionary resources = asDictionary(stream.getDictionaryObject(COSName.RESOURCES));
        if (!"XObject".equals(stream.getNameAsString(COSName.TYPE))
            || !"Form".equals(stream.getNameAsString(COSName.SUBTYPE))
            || box == null || box.size() != 4 || resources == null || resources.size() > 4_096) {
            throw new PostcheckException("VISIBLE_APPEARANCE_INVALID");
        }
        double[] coordinates = new double[4];
        for (int index = 0; index < coordinates.length; index++) {
            COSBase value = box.getObject(index);
            if (!(value instanceof COSNumber number)) {
                throw new PostcheckException("VISIBLE_APPEARANCE_INVALID");
            }
            coordinates[index] = number.floatValue();
            if (!Double.isFinite(coordinates[index]) || Math.abs(coordinates[index]) > 1_000_000d) {
                throw new PostcheckException("VISIBLE_APPEARANCE_INVALID");
            }
        }
        if (coordinates[2] <= coordinates[0] || coordinates[3] <= coordinates[1]) {
            throw new PostcheckException("VISIBLE_APPEARANCE_INVALID");
        }
        for (COSName key : resources.keySet()) {
            if (resources.getDictionaryObject(key) == null) {
                throw new PostcheckException("VISIBLE_APPEARANCE_INVALID");
            }
        }
        try (InputStream input = stream.createInputStream()) {
            long length = 0;
            byte[] buffer = new byte[8_192];
            int read;
            while ((read = input.read(buffer)) != -1) {
                length += read;
                if (length > 16L * 1024L * 1024L) {
                    throw new PostcheckException("VISIBLE_APPEARANCE_INVALID");
                }
            }
            if (length == 0) throw new PostcheckException("VISIBLE_APPEARANCE_INVALID");
        } catch (IOException exception) {
            throw new PostcheckException("VISIBLE_APPEARANCE_INVALID");
        }
    }

    Map<String, Object> inspect(byte[] source, byte[] output) throws PostcheckException {
        try {
            SafePdfMutation.assertAppendOnly(source, output);
            try (PDDocument before = Loader.loadPDF(source); PDDocument after = Loader.loadPDF(output)) {
                Map<String, Object> result = new LinkedHashMap<>();
                result.put("appendOnly", true);
                result.put("sourceBytesPreserved", source.length);
                result.put("appendedBytes", output.length - source.length);
                result.put("priorSignatureCount", before.getSignatureDictionaries().size());
                result.put("outputSignatureCount", after.getSignatureDictionaries().size());
                result.put("structurallyReadable", true);
                return result;
            }
        } catch (SafePdfMutation.MutationException exception) {
            throw new PostcheckException(exception.code());
        } catch (IOException | RuntimeException exception) {
            throw new PostcheckException("OUTPUT_VALIDATION_FAILED");
        }
    }

    private static Integer docMdpPermission(COSDictionary signature) throws PostcheckException {
        COSArray references = asArray(signature.getDictionaryObject(COSName.getPDFName("Reference")));
        if (references == null) return null;
        Integer observed = null;
        for (int index = 0; index < references.size(); index++) {
            COSDictionary reference = asDictionary(references.getObject(index));
            if (reference == null) throw new PostcheckException("MALFORMED_SIGNATURE_TRANSFORM");
            String method = reference.getNameAsString(COSName.getPDFName("TransformMethod"));
            if (!"DocMDP".equals(method)) continue;
            if (observed != null) throw new PostcheckException("MALFORMED_SIGNATURE_TRANSFORM");
            COSDictionary parameters = asDictionary(
                reference.getDictionaryObject(COSName.getPDFName("TransformParams"))
            );
            Object permission = parameters == null ? null : parameters.getDictionaryObject(COSName.getPDFName("P"));
            if (!(permission instanceof COSInteger integer)
                || integer.intValue() < 1 || integer.intValue() > 3) {
                throw new PostcheckException("MALFORMED_SIGNATURE_TRANSFORM");
            }
            observed = integer.intValue();
        }
        return observed;
    }

    private static COSDictionary asDictionary(Object value) {
        return value instanceof COSDictionary dictionary ? dictionary : null;
    }

    private static COSArray asArray(Object value) {
        return value instanceof COSArray array ? array : null;
    }
}
