package com.butterpaper.signaturecore;

import eu.europa.esig.dss.enumerations.CertificationPermission;
import eu.europa.esig.dss.enumerations.PdfLockAction;
import eu.europa.esig.dss.model.InMemoryDocument;
import eu.europa.esig.dss.pades.validation.ByteRange;
import eu.europa.esig.dss.pades.validation.PAdESSignature;
import eu.europa.esig.dss.pades.validation.PDFDocumentValidator;
import eu.europa.esig.dss.pades.validation.PdfRevision;
import eu.europa.esig.dss.pdf.modifications.PdfModificationDetection;
import eu.europa.esig.dss.pdf.modifications.PdfObjectModifications;
import eu.europa.esig.dss.spi.signature.AdvancedSignature;
import org.apache.pdfbox.Loader;
import org.apache.pdfbox.cos.COSArray;
import org.apache.pdfbox.cos.COSBase;
import org.apache.pdfbox.cos.COSDictionary;
import org.apache.pdfbox.cos.COSInteger;
import org.apache.pdfbox.cos.COSName;
import org.apache.pdfbox.cos.COSString;
import org.apache.pdfbox.pdmodel.PDDocument;
import org.apache.pdfbox.pdmodel.interactive.digitalsignature.PDSignature;

import java.io.IOException;
import java.math.BigInteger;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/** Reconstructs signed policy dictionaries from their exact covered revisions. */
final class AuthoritativeSignedPolicies {
    private static final COSName REFERENCE = COSName.getPDFName("Reference");
    private static final COSName TRANSFORM_METHOD = COSName.getPDFName("TransformMethod");
    private static final COSName TRANSFORM_PARAMS = COSName.getPDFName("TransformParams");
    private static final COSName VERSION = COSName.getPDFName("V");
    private static final COSName ACTION = COSName.getPDFName("Action");
    private static final COSName FIELDS = COSName.getPDFName("Fields");
    private static final COSName PERMS = COSName.getPDFName("Perms");
    private static final COSName DOC_MDP = COSName.getPDFName("DocMDP");

    private record Identity(int[] byteRange, byte[] contents) {
        int revisionEnd() { return byteRange[2] + byteRange[3]; }
    }

    private record RawPolicies(Integer docMdp, RawFieldPolicy fieldMdp) {}
    private record RawFieldPolicy(PdfLockAction action, List<String> names) {
        boolean locks(String fieldName) {
            if (action == PdfLockAction.ALL) return true;
            boolean listed = names.contains(fieldName);
            return action == PdfLockAction.INCLUDE ? listed : !listed;
        }
    }

    static void assertAllowsApproval(byte[] source, String targetFieldName)
        throws SigningService.SigningException {
        try (PDDocument current = Loader.loadPDF(source)) {
            List<PDSignature> pdfBox = new ArrayList<>(current.getSignatureDictionaries());
            if (pdfBox.isEmpty()) return;
            List<Identity> identities = validateRevisionChain(source, pdfBox);

            PDFDocumentValidator validator = new PDFDocumentValidator(new InMemoryDocument(source));
            validator.setCertificateVerifier(ValidationService.newOfflineVerifier());
            List<AdvancedSignature> advanced = new ArrayList<>(validator.getSignatures());
            List<PdfRevision> revisions = new ArrayList<>(validator.getRevisions());
            if (advanced.size() != pdfBox.size()) indeterminate();

            PAdESSignature certification = null;
            Identity certificationIdentity = null;
            int certificationPermission = 0;
            for (int index = 0; index < pdfBox.size(); index++) {
                Identity identity = identities.get(index);
                PAdESSignature pades = uniquelyMappedAdvanced(advanced, identity);
                pades.checkSignatureIntegrity();
                if (!pades.getSignatureCryptographicVerification().isSignatureValid()) indeterminate();

                RawPolicies raw = rawPoliciesAtSignedRevision(source, identity);
                CertificationPermission dssDocMdp = pades.getPdfSignatureDictionary().getDocMDP();
                Integer dssPermission = dssDocMdp == null ? null : dssDocMdp.getCode();
                if (!java.util.Objects.equals(raw.docMdp(), dssPermission)) indeterminate();
                assertFieldPolicyMatches(raw.fieldMdp(), pades);
                if (raw.fieldMdp() != null && raw.fieldMdp().locks(targetFieldName)) {
                    throw new SigningService.SigningException("SIGNATURE_FIELD_LOCKED");
                }
                if (raw.docMdp() != null) {
                    if (certification != null) indeterminate();
                    certification = pades;
                    certificationIdentity = identity;
                    certificationPermission = raw.docMdp();
                    assertCatalogDesignates(source, identity.revisionEnd(), identity);
                }
            }

            Identity currentDocMdp = catalogDocMdpIdentity(current);
            if (certification == null) {
                if (currentDocMdp != null) indeterminate();
            } else {
                if (currentDocMdp == null || !sameIdentity(currentDocMdp, certificationIdentity)) indeterminate();
                assertSubsequentRevisionsComply(revisions, certificationIdentity, certificationPermission);
                if (certificationPermission == 1) {
                    throw new SigningService.SigningException("CERTIFICATION_FORBIDS_SIGNATURE");
                }
            }
        } catch (SigningService.SigningException exception) {
            throw exception;
        } catch (IOException | RuntimeException exception) {
            throw new SigningService.SigningException("SOURCE_POLICY_INDETERMINATE");
        }
    }

    static void assertRawPoliciesWellFormedForTest(COSDictionary signature)
        throws SigningService.SigningException {
        parseRawPolicies(signature);
    }

    static void assertAddedSignaturePolicies(
        COSDictionary signature,
        Integer expectedDocMdp,
        SignatureFieldSpec.FieldLock expectedFieldLock
    ) throws SigningService.SigningException {
        RawPolicies observed = parseRawPolicies(signature);
        if (!java.util.Objects.equals(expectedDocMdp, observed.docMdp())) indeterminate();
        RawFieldPolicy expected = expectedFieldLock == null ? null : new RawFieldPolicy(
            switch (expectedFieldLock.action()) {
                case "all" -> PdfLockAction.ALL;
                case "include" -> PdfLockAction.INCLUDE;
                case "exclude" -> PdfLockAction.EXCLUDE;
                default -> throw new IllegalArgumentException("unexpected field lock action");
            },
            expectedFieldLock.fieldNames()
        );
        if (expected == null) {
            if (observed.fieldMdp() != null) indeterminate();
        } else if (observed.fieldMdp() == null
            || expected.action() != observed.fieldMdp().action()
            || !expected.names().equals(observed.fieldMdp().names())) {
            indeterminate();
        }
    }

    private static List<Identity> validateRevisionChain(byte[] source, List<PDSignature> signatures)
        throws SigningService.SigningException {
        List<Identity> identities = signatures.stream().map(AuthoritativeSignedPolicies::identity).sorted(
            Comparator.comparingInt(Identity::revisionEnd)
        ).toList();
        int priorEnd = 0;
        Set<List<Integer>> ranges = new HashSet<>();
        for (Identity identity : identities) {
            int[] range = identity.byteRange();
            if (range.length != 4 || range[0] != 0 || range[1] <= 0 || range[2] <= range[1]
                || range[3] < 0 || identity.revisionEnd() <= priorEnd
                || identity.revisionEnd() > source.length
                || !ranges.add(Arrays.stream(range).boxed().toList())) {
                indeterminate();
            }
            priorEnd = identity.revisionEnd();
        }
        // Signing is never allowed on an unsigned trailing revision. Every byte must be
        // covered by the most recent signature before another signature is appended.
        if (priorEnd != source.length) indeterminate();
        return identities;
    }

    private static RawPolicies rawPoliciesAtSignedRevision(byte[] source, Identity identity)
        throws IOException, SigningService.SigningException {
        try (PDDocument revision = Loader.loadPDF(Arrays.copyOf(source, identity.revisionEnd()))) {
            PDSignature exact = uniquelyMappedPdfBox(revision.getSignatureDictionaries(), identity);
            return parseRawPolicies(exact.getCOSObject());
        }
    }

    private static RawPolicies parseRawPolicies(COSDictionary signature)
        throws SigningService.SigningException {
        COSBase rawReferences = signature.getDictionaryObject(REFERENCE);
        if (rawReferences == null) return new RawPolicies(null, null);
        if (!(rawReferences instanceof COSArray)) indeterminate();
        COSArray references = (COSArray) rawReferences;
        if (references.size() == 0 || references.size() > 8) indeterminate();
        Integer docMdp = null;
        RawFieldPolicy fieldMdp = null;
        for (int index = 0; index < references.size(); index++) {
            if (!(references.getObject(index) instanceof COSDictionary)) indeterminate();
            COSDictionary reference = (COSDictionary) references.getObject(index);
            if (!"SigRef".equals(reference.getNameAsString(COSName.TYPE))) indeterminate();
            String method = reference.getNameAsString(TRANSFORM_METHOD);
            if (!(reference.getDictionaryObject(TRANSFORM_PARAMS) instanceof COSDictionary)) indeterminate();
            COSDictionary parameters = (COSDictionary) reference.getDictionaryObject(TRANSFORM_PARAMS);
            if (!"TransformParams".equals(parameters.getNameAsString(COSName.TYPE))
                || !"1.2".equals(parameters.getNameAsString(VERSION))) indeterminate();
            if ("DocMDP".equals(method)) {
                if (docMdp != null || !(parameters.getDictionaryObject(COSName.P) instanceof COSInteger)) indeterminate();
                COSInteger permission = (COSInteger) parameters.getDictionaryObject(COSName.P);
                if (permission.intValue() < 1 || permission.intValue() > 3) indeterminate();
                docMdp = permission.intValue();
            } else if ("FieldMDP".equals(method)) {
                if (fieldMdp != null) indeterminate();
                fieldMdp = parseFieldPolicy(parameters);
            } else {
                indeterminate();
            }
        }
        return new RawPolicies(docMdp, fieldMdp);
    }

    private static RawFieldPolicy parseFieldPolicy(COSDictionary parameters)
        throws SigningService.SigningException {
        String actionName = parameters.getNameAsString(ACTION);
        PdfLockAction action = switch (actionName == null ? "" : actionName) {
            case "All" -> PdfLockAction.ALL;
            case "Include" -> PdfLockAction.INCLUDE;
            case "Exclude" -> PdfLockAction.EXCLUDE;
            default -> null;
        };
        if (action == null) indeterminate();
        COSBase rawFields = parameters.getDictionaryObject(FIELDS);
        if (action == PdfLockAction.ALL) {
            if (rawFields != null) indeterminate();
            return new RawFieldPolicy(action, List.of());
        }
        if (!(rawFields instanceof COSArray)) indeterminate();
        COSArray fields = (COSArray) rawFields;
        if (fields.size() == 0 || fields.size() > 256) indeterminate();
        List<String> names = new ArrayList<>();
        for (int index = 0; index < fields.size(); index++) {
            COSBase value = fields.getObject(index);
            if (!(value instanceof COSString)) indeterminate();
            COSString string = (COSString) value;
            String name = string.getString();
            if (name.isEmpty() || name.length() > 512 || name.codePoints().anyMatch(Character::isISOControl)
                || names.contains(name)) {
                indeterminate();
            }
            names.add(name);
        }
        return new RawFieldPolicy(action, List.copyOf(names));
    }

    private static void assertFieldPolicyMatches(RawFieldPolicy raw, PAdESSignature pades)
        throws SigningService.SigningException {
        var dss = pades.getPdfSignatureDictionary().getFieldMDP();
        if (raw == null) {
            if (dss != null) indeterminate();
            return;
        }
        if (dss == null || raw.action() != dss.getAction() || dss.getFields() == null
            || !raw.names().equals(dss.getFields())) {
            indeterminate();
        }
    }

    private static void assertCatalogDesignates(byte[] source, int revisionEnd, Identity expected)
        throws IOException, SigningService.SigningException {
        try (PDDocument revision = Loader.loadPDF(Arrays.copyOf(source, revisionEnd))) {
            Identity designated = catalogDocMdpIdentity(revision);
            if (designated == null || !sameIdentity(designated, expected)) indeterminate();
        }
    }

    private static Identity catalogDocMdpIdentity(PDDocument document)
        throws SigningService.SigningException {
        COSBase rawPerms = document.getDocumentCatalog().getCOSObject().getDictionaryObject(PERMS);
        if (rawPerms == null) return null;
        if (!(rawPerms instanceof COSDictionary)) indeterminate();
        COSDictionary permissions = (COSDictionary) rawPerms;
        COSBase rawDocMdp = permissions.getDictionaryObject(DOC_MDP);
        if (rawDocMdp == null) return null;
        if (!(rawDocMdp instanceof COSDictionary)) indeterminate();
        COSDictionary dictionary = (COSDictionary) rawDocMdp;
        Identity designated = identity(new PDSignature(dictionary));
        // A catalog designation is authoritative only if it resolves uniquely to a
        // parsed signature dictionary in this exact revision.
        PDSignature mapped = uniquelyMappedPdfBox(document.getSignatureDictionaries(), designated);
        if (dictionary != mapped.getCOSObject()) indeterminate();
        return designated;
    }

    private static void assertSubsequentRevisionsComply(
        List<PdfRevision> revisions,
        Identity certification,
        int permission
    ) throws SigningService.SigningException {
        PdfRevision matched = null;
        for (PdfRevision revision : revisions) {
            if (revision.getPdfSigDictInfo() != null && matches(revision.getPdfSigDictInfo().getByteRange(), certification.byteRange())) {
                if (matched != null) indeterminate();
                matched = revision;
            }
        }
        if (matched == null) indeterminate();
        PdfModificationDetection detection = matched.getModificationDetection();
        if (detection == null || !detection.areModificationsDetected()) return;
        if (!detection.getPageDifferences().isEmpty() || !detection.getVisualDifferences().isEmpty()
            || !detection.getAnnotationOverlaps().isEmpty()) {
            indeterminate();
        }
        PdfObjectModifications objects = detection.getObjectModifications();
        if (objects == null || !objects.getUndefinedChanges().isEmpty()) indeterminate();
        if (permission == 1 && (!objects.getFormFillInAndSignatureCreationChanges().isEmpty()
            || !objects.getAnnotCreationChanges().isEmpty())) {
            throw new SigningService.SigningException("CERTIFICATION_FORBIDS_SIGNATURE");
        }
        if (permission == 2 && !objects.getAnnotCreationChanges().isEmpty()) indeterminate();
    }

    private static PAdESSignature uniquelyMappedAdvanced(List<AdvancedSignature> signatures, Identity expected)
        throws SigningService.SigningException {
        PAdESSignature match = null;
        for (AdvancedSignature signature : signatures) {
            if (signature instanceof PAdESSignature pades
                && matches(pades.getPdfSignatureDictionary().getByteRange(), expected.byteRange())) {
                if (match != null) indeterminate();
                match = pades;
            }
        }
        if (match == null || !MessageDigest.isEqual(trimPadding(match.getPdfSignatureDictionary().getContents()), trimPadding(expected.contents()))) {
            indeterminate();
        }
        return match;
    }

    private static PDSignature uniquelyMappedPdfBox(List<PDSignature> signatures, Identity expected)
        throws SigningService.SigningException {
        PDSignature match = null;
        for (PDSignature signature : signatures) {
            if (Arrays.equals(signature.getByteRange(), expected.byteRange())
                && MessageDigest.isEqual(trimPadding(signature.getContents()), trimPadding(expected.contents()))) {
                if (match != null) indeterminate();
                match = signature;
            }
        }
        if (match == null) indeterminate();
        return match;
    }

    private static Identity identity(PDSignature signature) {
        int[] range = signature.getByteRange();
        return new Identity(range == null ? new int[0] : range.clone(), signature.getContents().clone());
    }

    private static boolean sameIdentity(Identity left, Identity right) {
        return Arrays.equals(left.byteRange(), right.byteRange())
            && MessageDigest.isEqual(trimPadding(left.contents()), trimPadding(right.contents()));
    }

    private static boolean matches(ByteRange range, int[] expected) {
        return range != null && range.toBigIntegerList().equals(List.of(
            BigInteger.valueOf(expected[0]), BigInteger.valueOf(expected[1]),
            BigInteger.valueOf(expected[2]), BigInteger.valueOf(expected[3])
        ));
    }

    private static byte[] trimPadding(byte[] value) {
        int end = value.length;
        while (end > 0 && value[end - 1] == 0) end--;
        return Arrays.copyOf(value, end);
    }

    private static void indeterminate() throws SigningService.SigningException {
        throw new SigningService.SigningException("SOURCE_POLICY_INDETERMINATE");
    }
}
