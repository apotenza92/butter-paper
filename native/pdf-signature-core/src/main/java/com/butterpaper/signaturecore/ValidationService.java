package com.butterpaper.signaturecore;
import eu.europa.esig.dss.model.InMemoryDocument;
import eu.europa.esig.dss.model.signature.SignatureCryptographicVerification;
import eu.europa.esig.dss.model.x509.CertificateToken;
import eu.europa.esig.dss.model.identifier.EncapsulatedRevocationTokenIdentifier;
import eu.europa.esig.dss.enumerations.CertificateStatus;
import eu.europa.esig.dss.enumerations.RevocationOrigin;
import eu.europa.esig.dss.pades.validation.PDFDocumentValidator;
import eu.europa.esig.dss.pades.validation.PAdESSignature;
import eu.europa.esig.dss.pades.validation.PdfRevision;
import eu.europa.esig.dss.pdf.modifications.PdfModificationDetection;
import eu.europa.esig.dss.pdf.modifications.PdfObjectModifications;
import eu.europa.esig.dss.spi.signature.AdvancedSignature;
import eu.europa.esig.dss.spi.validation.CommonCertificateVerifier;
import eu.europa.esig.dss.spi.x509.revocation.RevocationToken;
import eu.europa.esig.dss.spi.x509.tsp.TimestampToken;
import org.apache.pdfbox.Loader;
import org.apache.pdfbox.cos.COSArray;
import org.apache.pdfbox.cos.COSBase;
import org.apache.pdfbox.cos.COSDictionary;
import org.apache.pdfbox.cos.COSName;
import org.apache.pdfbox.cos.COSObject;
import org.apache.pdfbox.cos.COSStream;
import org.apache.pdfbox.cos.COSString;
import org.apache.pdfbox.pdmodel.PDDocument;
import org.apache.pdfbox.pdmodel.PDPage;
import org.apache.pdfbox.pdmodel.common.PDRectangle;
import org.apache.pdfbox.pdmodel.interactive.annotation.PDAnnotation;
import org.apache.pdfbox.pdmodel.interactive.annotation.PDAnnotationWidget;
import org.apache.pdfbox.pdmodel.interactive.digitalsignature.PDSignature;
import org.apache.pdfbox.pdmodel.interactive.form.PDSignatureField;
import org.bouncycastle.cms.CMSSignedData;
import org.bouncycastle.cms.SignerInformation;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.channels.SeekableByteChannel;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.OpenOption;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.nio.file.attribute.BasicFileAttributes;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.security.cert.CertificateExpiredException;
import java.security.cert.CertificateNotYetValidException;
import java.security.cert.CertificateParsingException;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Calendar;
import java.util.Collection;
import java.util.Date;
import java.util.HexFormat;
import java.util.IdentityHashMap;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.Comparator;
/**
 * Read-only PDF signature inventory and offline cryptographic validation.
 *
 * <p>This service intentionally does not run DSS's aggregate policy result. It
 * reports cryptographic integrity independently from trust, revocation, time,
 * and modification status. The verifier has no AIA, OCSP, CRL, or trust-store
 * source, so this code cannot silently turn an offline request into network
 * validation.</p>
 */
final class ValidationService {
    static final String OBSERVED_SYSTEM_UTC = "observed-system-utc";
    static final String CALLER_SUPPLIED_FIXED_REFERENCE = "caller-supplied-fixed-reference";
    static final Instant EARLIEST_FIXED_REFERENCE = Instant.parse("1900-01-01T00:00:00Z");
    record ValidationClock(Instant instant, String provenance) {
        ValidationClock {
            if (instant == null || (!OBSERVED_SYSTEM_UTC.equals(provenance)
                && !CALLER_SUPPLIED_FIXED_REFERENCE.equals(provenance))) {
                throw new IllegalArgumentException("invalid validation clock");
            }
        }
        static ValidationClock observedSystemUtc() {
            return observedSystemUtc(Instant.now());
        }
        static ValidationClock observedSystemUtc(Instant observed) {
            return new ValidationClock(observed, OBSERVED_SYSTEM_UTC);
        }
        static ValidationClock fixedReference(Instant fixed, Instant observedSystemUtc) {
            if (fixed == null || observedSystemUtc == null
                || fixed.isBefore(EARLIEST_FIXED_REFERENCE)
                || fixed.isAfter(observedSystemUtc)) {
                throw new IllegalArgumentException("fixed validation clock is outside the safe reference window");
            }
            return new ValidationClock(fixed, CALLER_SUPPLIED_FIXED_REFERENCE);
        }
    }
    static final class ValidationException extends Exception {
        private final String code;
        ValidationException(String code) {
            super(code);
            this.code = code;
        }
        String code() { return code; }
    }
    Map<String, Object> validate(String inputPath) throws ValidationException {
        return validate(inputPath, ExactTrustPolicy.empty(), ValidationClock.observedSystemUtc());
    }
    Map<String, Object> validate(String inputPath, ExactTrustPolicy trustPolicy) throws ValidationException {
        return validate(inputPath, trustPolicy, ValidationClock.observedSystemUtc());
    }
    Map<String, Object> validate(
        String inputPath,
        ExactTrustPolicy trustPolicy,
        ValidationClock validationClock
    ) throws ValidationException {
        if (validationClock == null) throw new ValidationException("INVALID_VALIDATION_CLOCK");
        InputSnapshot input = readInput(inputPath);
        try (PDDocument pdf = Loader.loadPDF(input.bytes())) {
            List<PDSignature> dictionaries = pdf.getSignatureDictionaries();
            List<PDSignatureField> signatureFields = pdf.getSignatureFields();
            Set<String> duplicateFieldNames = duplicateSignatureFieldNames(signatureFields);
            boolean invalidSignatureStructure = dictionaries.stream()
                .anyMatch(dictionary -> !validByteRange(dictionary.getByteRange(), input.bytes().length));
            DssAnalysis dss = analyzeWithDss(input.bytes());
            RevisionAnalysis revisions = invalidSignatureStructure
                ? RevisionAnalysis.incomplete(dictionaries.size())
                : analyzeRevisions(dss.validator(), dictionaries);
            List<AdvancedSignature> matchedSignatures = matchAdvancedSignatures(dictionaries, dss.signatures());
            List<Map<String, Object>> fields = inventoryFields(pdf, signatureFields);
            Map<String, Object> validationEvidence = validationEvidenceInventory(pdf);
            List<Map<String, Object>> signatures = new ArrayList<>();
            String certificationPermission = "not-certified";
            boolean modificationPolicyComplete = true;
            boolean hasCertification = false;
            for (int index = 0; index < dictionaries.size(); index++) {
                PDSignature dictionary = dictionaries.get(index);
                AdvancedSignature advanced = matchedSignatures.get(index);
                TransformAnalysis transforms = analyzeTransforms(dictionary.getCOSObject());
                boolean designatedCertification = isCatalogCertification(pdf, dictionary);
                if (designatedCertification) {
                    hasCertification = true;
                    certificationPermission = transforms.certificationPermission() == null
                        ? "unknown" : transforms.certificationPermission();
                }
                modificationPolicyComplete &= transforms.complete()
                    && (!designatedCertification || transforms.certificationPermission() != null)
                    && (transforms.certificationPermission() == null || designatedCertification);
                signatures.add(signatureSummary(
                    input.bytes().length,
                    index,
                    dictionary,
                    fieldNamesFor(dictionary, signatureFields),
                    advanced,
                    revisions.signedRevision(index),
                    revisions.complete(),
                    transforms,
                    designatedCertification,
                    trustPolicy,
                    validationClock.instant()
                ));
            }
            List<Map<String, Object>> topIssues = new ArrayList<>();
            if (dss.failure()) {
                topIssues.add(issue(
                    "UNSUPPORTED_SIGNATURE",
                    "warning",
                    "One or more signature containers could not be parsed by the offline validation engine.",
                    null
                ));
            }
            if (!duplicateFieldNames.isEmpty()) {
                topIssues.add(issue(
                    "MALFORMED_SIGNATURE",
                    "error",
                    "Duplicate fully qualified signature field names make the signature inventory ambiguous.",
                    null
                ));
                modificationPolicyComplete = false;
            }
            if (invalidSignatureStructure) {
                topIssues.add(issue(
                    "INVALID_BYTE_RANGE",
                    "error",
                    "A malformed signature byte range makes the PDF revision inventory unreliable.",
                    null
                ));
                modificationPolicyComplete = false;
            }
            String presence;
            if (dictionaries.isEmpty() && signatureFields.stream().noneMatch(field -> field.getSignature() != null)) {
                presence = "unsigned";
                certificationPermission = "not-certified";
            } else if (dictionaries.isEmpty() || invalidSignatureStructure || !duplicateFieldNames.isEmpty()) {
                presence = "indeterminate";
                modificationPolicyComplete = false;
            } else if (hasCertification) {
                presence = "certified";
            } else {
                presence = "signed";
            }
            Map<String, Object> inventory = orderedMap();
            inventory.put("presence", presence);
            inventory.put("certificationPermission", certificationPermission);
            inventory.put("currentRevision", revisions.complete() ? revisions.totalRevisions() : null);
            inventory.put("totalRevisions", revisions.complete() ? revisions.totalRevisions() : null);
            inventory.put("revisionInventoryComplete", revisions.complete());
            inventory.put("fields", fields);
            inventory.put("modificationPolicyComplete", modificationPolicyComplete && !dss.failure());
            inventory.put("validationEvidence", validationEvidence);
            Map<String, Object> trust = orderedMap();
            trust.put("policyName", ExactTrustPolicy.POLICY_DISPLAY_NAME);
            trust.put("policyId", trustPolicy.policyId());
            trust.put("policyVersion", trustPolicy.policyVersion());
            trust.put("configurationSha256", trustPolicy.configurationSha256());
            trust.put("configuredExactCertificateFingerprints", trustPolicy.configuredFingerprints());
            trust.put("onlineSourcesUsed", false);
            trust.put("limitations", List.of(
                "Only enabled exact-certificate decisions are applied; they are never CA or descendant trust anchors.",
                "AIA, OCSP, and CRL network retrieval were disabled.",
                "Cryptographically bound embedded revocation evidence is reported independently of trust."
            ));
            Map<String, Object> report = orderedMap();
            report.put("schemaVersion", 1);
            report.put("inputSha256", input.sha256());
            report.put("validationMode", "offline");
            report.put("validationTime", validationClock.instant().toString());
            report.put("validationTimeProvenance", validationClock.provenance());
            report.put("engineVersion", Protocol.ENGINE_VERSION);
            report.put("inventory", inventory);
            report.put("signatures", signatures);
            report.put("trust", trust);
            report.put("limitations", List.of(
                "Offline validation does not establish external certificate-path or legal trust; enabled exact-certificate local decisions are reported separately.",
                "No aggregate validity conclusion is produced.",
                "Later modifications are classified only when the signed revision boundary is unambiguous.",
                "DSS and VRI inventory is structural only; presence, counts, embedded-object observations, and key references do not establish validity or cryptographic binding."
            ));
            report.put("issues", topIssues);
            return report;
        } catch (IOException | RuntimeException exception) {
            throw new ValidationException("MALFORMED_PDF");
        }
    }
    static CommonCertificateVerifier newOfflineVerifier() {
        CommonCertificateVerifier verifier = new CommonCertificateVerifier(true);
        verifier.setAIASource(null);
        verifier.setOcspSource(null);
        verifier.setCrlSource(null);
        verifier.setCheckRevocationForUntrustedChains(false);
        verifier.setRevocationFallback(false);
        return verifier;
    }
    private static DssAnalysis analyzeWithDss(byte[] bytes) {
        try {
            PDFDocumentValidator validator = new PDFDocumentValidator(new InMemoryDocument(bytes));
            validator.setCertificateVerifier(newOfflineVerifier());
            List<AdvancedSignature> signatures = new ArrayList<>(validator.getSignatures());
            for (AdvancedSignature signature : signatures) {
                try {
                    signature.checkSignatureIntegrity();
                } catch (Throwable exception) {
                    throwIfFatal(exception);
                    // The per-signature result remains indeterminate and is reported below.
                }
            }
            try {
                // Diagnostic construction performs DSS/PDFBox's structural
                // modification analysis without producing an aggregate policy conclusion.
                validator.getDiagnosticData();
            } catch (Throwable exception) {
                throwIfFatal(exception);
                // Per-signature modification status remains unable-to-classify.
            }
            return new DssAnalysis(validator, signatures, false);
        } catch (Throwable exception) {
            throwIfFatal(exception);
            return new DssAnalysis(null, List.of(), true);
        }
    }
    private static RevisionAnalysis analyzeRevisions(
        PDFDocumentValidator validator,
        List<PDSignature> dictionaries
    ) {
        if (validator == null) return RevisionAnalysis.incomplete(dictionaries.size());
        try {
            List<PdfRevision> revisions = validator.getRevisions();
            int total = Math.max(1, revisions.size());
            List<Integer> signedRevisions = new ArrayList<>();
            for (PDSignature dictionary : dictionaries) {
                int revision = 0;
                int[] byteRange = dictionary.getByteRange();
                long revisionEnd = coveredRevisionEnd(byteRange);
                for (int index = 0; index < revisions.size(); index++) {
                    if (revisions.get(index).getPdfSigDictInfo() != null
                        && revisions.get(index).getPdfSigDictInfo().getByteRange() != null
                        && revisions.get(index).getPdfSigDictInfo().getByteRange().getLength() == revisionEnd) {
                        // DSS exposes PDF revisions newest-first. The public
                        // contract numbers them chronologically from one.
                        revision = revisions.size() - index;
                        break;
                    }
                }
                signedRevisions.add(revision == 0 ? null : revision);
            }
            return new RevisionAnalysis(total, true, signedRevisions);
        } catch (Throwable exception) {
            throwIfFatal(exception);
            return RevisionAnalysis.incomplete(dictionaries.size());
        }
    }
    private static List<AdvancedSignature> matchAdvancedSignatures(
        List<PDSignature> dictionaries,
        List<AdvancedSignature> candidates
    ) {
        List<AdvancedSignature> result = new ArrayList<>();
        Set<AdvancedSignature> used = java.util.Collections.newSetFromMap(new IdentityHashMap<>());
        for (PDSignature dictionary : dictionaries) {
            byte[] cmsSignatureValue = cmsSignatureValue(dictionary.getContents());
            AdvancedSignature match = null;
            if (cmsSignatureValue != null) {
                for (AdvancedSignature candidate : candidates) {
                    if (!used.contains(candidate)
                        && MessageDigest.isEqual(cmsSignatureValue, candidate.getSignatureValue())) {
                        match = candidate;
                        used.add(candidate);
                        break;
                    }
                }
            }
            result.add(match);
        }
        return result;
    }
    static byte[] cmsSignatureValue(byte[] contents) {
        if (contents == null || contents.length == 0) return null;
        byte[] value = parseCmsSignatureValue(contents);
        if (value != null) return value;
        int meaningfulLength = contents.length;
        while (meaningfulLength > 0 && contents[meaningfulLength - 1] == 0) meaningfulLength--;
        return parseCmsSignatureValue(Arrays.copyOf(contents, meaningfulLength));
    }
    private static byte[] parseCmsSignatureValue(byte[] contents) {
        try {
            CMSSignedData cms = new CMSSignedData(contents);
            java.util.Collection<SignerInformation> signers = cms.getSignerInfos().getSigners();
            return signers.size() == 1 ? signers.iterator().next().getSignature() : null;
        } catch (RuntimeException | org.bouncycastle.cms.CMSException exception) {
            return null;
        }
    }
    private static List<Map<String, Object>> inventoryFields(
        PDDocument pdf,
        List<PDSignatureField> fields
    ) throws IOException {
        List<Map<String, Object>> result = new ArrayList<>();
        for (int index = 0; index < fields.size(); index++) {
            PDSignatureField field = fields.get(index);
            List<Map<String, Object>> widgets = new ArrayList<>();
            for (PDAnnotationWidget widget : field.getWidgets()) {
                PDRectangle rectangle = widget.getRectangle();
                if (rectangle == null) continue;
                int pageIndex = pageIndex(pdf, widget);
                if (pageIndex < 0) continue;
                Map<String, Object> item = orderedMap();
                item.put("pageIndex", pageIndex);
                item.put("rect", List.of(
                    rectangle.getLowerLeftX(),
                    rectangle.getLowerLeftY(),
                    rectangle.getWidth(),
                    rectangle.getHeight()
                ));
                widgets.add(item);
            }
            Map<String, Object> item = orderedMap();
            item.put("id", "field-" + (index + 1));
            item.put("name", safeClaim(field.getFullyQualifiedName()));
            item.put("signed", field.getSignature() != null);
            item.put("widgets", widgets);
            result.add(item);
        }
        return result;
    }
    static Set<String> duplicateSignatureFieldNames(List<PDSignatureField> fields) {
        Set<String> seen = new HashSet<>();
        Set<String> duplicates = new HashSet<>();
        for (PDSignatureField field : fields) {
            String name = safeClaim(field.getFullyQualifiedName());
            if (!seen.add(name)) duplicates.add(name);
        }
        return duplicates;
    }
    private static int pageIndex(PDDocument pdf, PDAnnotationWidget widget) throws IOException {
        PDPage widgetPage = widget.getPage();
        for (int pageIndex = 0; pageIndex < pdf.getNumberOfPages(); pageIndex++) {
            PDPage page = pdf.getPage(pageIndex);
            if (widgetPage != null && page.getCOSObject() == widgetPage.getCOSObject()) return pageIndex;
            for (PDAnnotation annotation : page.getAnnotations()) {
                if (annotation.getCOSObject() == widget.getCOSObject()) return pageIndex;
            }
        }
        return -1;
    }
    private static List<String> fieldNamesFor(
        PDSignature dictionary,
        List<PDSignatureField> fields
    ) {
        List<String> names = new ArrayList<>();
        COSDictionary target = dictionary.getCOSObject();
        for (PDSignatureField field : fields) {
            PDSignature value = field.getSignature();
            if (value != null && value.getCOSObject() == target) {
                names.add(safeClaim(field.getFullyQualifiedName()));
            }
        }
        return names;
    }
    private static Map<String, Object> signatureSummary(
        long fileLength,
        int index,
        PDSignature dictionary,
        List<String> fieldNames,
        AdvancedSignature advanced,
        Integer signedRevision,
        boolean revisionInventoryComplete,
        TransformAnalysis transforms,
        boolean designatedCertification,
        ExactTrustPolicy trustPolicy,
        Instant validationTime
    ) {
        String signatureId = "signature-" + (index + 1);
        int[] byteRange = dictionary.getByteRange();
        boolean structurallyValid = validByteRange(byteRange, fileLength);
        long coveredRevisionEnd = structurallyValid ? coveredRevisionEnd(byteRange) : -1;
        List<Map<String, Object>> issues = new ArrayList<>();
        CertificateToken signingCertificate = advanced == null ? null : advanced.getSigningCertificateToken();
        String integrity = "indeterminate";
        if (!structurallyValid) {
            integrity = "failed";
            issues.add(issue(
                "INVALID_BYTE_RANGE",
                "error",
                "The signature byte range is malformed or outside the input file.",
                signatureId
            ));
        } else if (advanced == null) {
            issues.add(issue(
                "UNSUPPORTED_SIGNATURE",
                "warning",
                "The signature container could not be parsed by the offline validation engine.",
                signatureId
            ));
        } else {
            SignatureCryptographicVerification verification = advanced.getSignatureCryptographicVerification();
            if (verification != null
                && verification.isSignatureIntact()
                && verification.isReferenceDataFound()
                && verification.isReferenceDataIntact()) {
                integrity = "intact";
            } else if (verification != null
                && signingCertificate != null
                && verification.isReferenceDataFound()) {
                integrity = "failed";
                issues.add(issue(
                    "CRYPTOGRAPHIC_FAILURE",
                    "error",
                    "The signed bytes or cryptographic signature did not verify.",
                    signatureId
                ));
            } else {
                issues.add(issue(
                    "UNSUPPORTED_SIGNATURE",
                    "warning",
                    "Cryptographic integrity could not be established from the embedded signature material.",
                    signatureId
                ));
            }
        }
        Date claimedTime = advanced != null && advanced.getSigningTime() != null
            ? advanced.getSigningTime()
            : calendarDate(dictionary.getSignDate());
        String kind = signatureKind(dictionary, designatedCertification);
        RevocationEvidence revocation = revocationEvidence(advanced, signingCertificate);
        boolean dependableSignature = structurallyValid && "intact".equals(integrity);
        boolean unsuitableSigningEku = dependableSignature
            && explicitlyUnsuitableSigningEku(signingCertificate, kind);
        String certificateStatus = !dependableSignature || unsuitableSigningEku
            ? "indeterminate"
            : revocation.status() == null
                ? certificateStatus(signingCertificate, claimedTime, validationTime)
                : revocation.status();
        boolean explicitlyTrusted = trustPolicy.explicitlyTrusts(signingCertificate);
        if (unsuitableSigningEku) {
            issues.add(issue(
                "SIGNING_EKU_UNSUITABLE",
                "warning",
                "The signing certificate has an explicit extended-key-usage set that does not authorize a supported document-signing purpose.",
                signatureId
            ));
        }
        if (signingCertificate != null) {
            if (!explicitlyTrusted) {
                issues.add(issue(
                    "TRUST_PATH_UNAVAILABLE",
                    "warning",
                    "The exact signing certificate has no enabled local trust decision.",
                    signatureId
                ));
            }
            if (revocation.status() == null) {
                issues.add(issue(
                    "REVOCATION_STATUS_UNKNOWN",
                    "warning",
                    "No cryptographically bound embedded revocation status was available offline.",
                    signatureId
                ));
            }
        }
        List<Map<String, Object>> timestamps = timestampInventory(advanced, claimedTime);
        if (timestamps.stream().anyMatch(timestamp -> !"claimed-time".equals(timestamp.get("kind"))
            && !Boolean.TRUE.equals(timestamp.get("verified")))) {
            issues.add(issue(
                "TIMESTAMP_INVALID",
                "error",
                "An embedded timestamp did not pass both token-signature and message-imprint verification.",
                signatureId
            ));
        }
        List<Map<String, Object>> certificates = certificateInventory(advanced, signingCertificate);
        Map<String, Object> byteRangeResult = orderedMap();
        byteRangeResult.put("segments", byteRangeSegments(byteRange));
        byteRangeResult.put("coveredRevisionEnd", structurallyValid ? coveredRevisionEnd : null);
        byteRangeResult.put("structurallyValid", structurallyValid);
        Map<String, Object> freshness = orderedMap();
        freshness.put("source", dependableSignature ? revocation.embedded() ? "embedded" : "none" : "indeterminate");
        freshness.put("producedAt", dependableSignature ? iso(revocation.producedAt()) : null);
        freshness.put("nextUpdateAt", dependableSignature ? iso(revocation.nextUpdateAt()) : null);
        Map<String, Object> qualification = orderedMap();
        qualification.put("padesProfile", padesProfile(advanced));
        qualification.put("claimedCompliant", false);
        qualification.put("limitations", List.of(
            "Profile material was inventoried without an online trust or revocation decision."
        ));
        Map<String, Object> result = orderedMap();
        result.put("id", signatureId);
        result.put("fieldNames", fieldNames);
        result.put("kind", kind);
        result.put("signedRevision", signedRevision);
        result.put("byteRange", byteRangeResult);
        result.put("transforms", transforms.transforms());
        result.put("certificates", certificates);
        result.put("timestamps", timestamps);
        result.put("signerClaim", safeNullableClaim(dictionary.getName()));
        result.put("claimedSigningTime", iso(claimedTime));
        result.put("integrity", integrity);
        result.put("identityTrust", !dependableSignature ? "indeterminate"
            : signingCertificate == null ? "unknown"
                : explicitlyTrusted ? "explicitly-trusted" : "untrusted");
        result.put("certificateStatus", certificateStatus);
        result.put("signingTime", dependableSignature
            ? signingTimeStatus(timestamps, claimedTime) : "indeterminate");
        String modificationStatus;
        if (!structurallyValid) {
            modificationStatus = "unable-to-classify";
        } else if ("failed".equals(integrity)) {
            modificationStatus = "prohibited";
            issues.add(issue(
                "MODIFICATION_PROHIBITED",
                "error",
                "The bytes covered by the signature no longer pass cryptographic verification.",
                signatureId
            ));
        } else if (coveredRevisionEnd == fileLength) {
            modificationStatus = "none";
        } else if (!revisionInventoryComplete) {
            modificationStatus = "unable-to-classify";
            issues.add(issue(
                "VALIDATION_OFFLINE_INCOMPLETE",
                "warning",
                "Later revisions exist, but the complete PDF revision inventory could not be established.",
                signatureId
            ));
        } else if (designatedCertification
            && "no-changes".equals(transforms.certificationPermission())) {
            modificationStatus = "prohibited";
            issues.add(issue(
                "MODIFICATION_PROHIBITED",
                "error",
                "The document contains a later revision after a no-changes certification.",
                signatureId
            ));
        } else if (permittedLaterChanges(advanced)) {
            modificationStatus = "permitted";
        } else if (prohibitedLaterChanges(advanced)) {
            modificationStatus = "prohibited";
            issues.add(issue(
                "MODIFICATION_PROHIBITED",
                "error",
                "The later PDF revision contains changes outside supported signature, form, or validation-evidence updates.",
                signatureId
            ));
        } else {
            modificationStatus = "unable-to-classify";
            issues.add(issue(
                "VALIDATION_OFFLINE_INCOMPLETE",
                "warning",
                "Later revisions exist, but their changes could not be classified conservatively.",
                signatureId
            ));
        }
        result.put("modificationStatus", modificationStatus);
        result.put("coverage", !structurallyValid ? "malformed"
            : signedRevision != null ? "whole-relevant-revision" : "partial-or-ambiguous");
        result.put("evidenceFreshness", freshness);
        result.put("qualification", qualification);
        result.put("fieldLock", transforms.fieldLock());
        result.put("issues", issues);
        return result;
    }
    private static boolean explicitlyUnsuitableSigningEku(
        CertificateToken certificate,
        String signatureKind
    ) {
        if (certificate == null) return false;
        try {
            List<String> usages = certificate.getCertificate().getExtendedKeyUsage();
            if (usages == null || usages.isEmpty()) return false;
            Set<String> supported = "document-timestamp".equals(signatureKind)
                ? Set.of(
                    "2.5.29.37.0",         // anyExtendedKeyUsage
                    "1.3.6.1.5.5.7.3.8"   // timeStamping
                )
                : Set.of(
                    "2.5.29.37.0",         // anyExtendedKeyUsage
                    "1.3.6.1.5.5.7.3.4",  // emailProtection
                    "1.2.840.113583.1.1.5" // Adobe authentic documents trust
                );
            return usages.stream().noneMatch(supported::contains);
        } catch (CertificateParsingException exception) {
            return true;
        }
    }
    private static boolean permittedLaterChanges(AdvancedSignature signature) {
        PdfModificationDetection detection = modificationDetection(signature);
        if (detection == null) return false;
        PdfObjectModifications objects = detection.getObjectModifications();
        return detection.getAnnotationOverlaps().isEmpty()
            && detection.getPageDifferences().isEmpty()
            && detection.getVisualDifferences().isEmpty()
            && (objects == null || objects.getUndefinedChanges().isEmpty());
    }
    private static boolean prohibitedLaterChanges(AdvancedSignature signature) {
        PdfModificationDetection detection = modificationDetection(signature);
        return detection != null && !permittedLaterChanges(signature);
    }
    private static PdfModificationDetection modificationDetection(AdvancedSignature signature) {
        if (!(signature instanceof PAdESSignature pades) || pades.getPdfRevision() == null) return null;
        return pades.getPdfRevision().getModificationDetection();
    }
    private static TransformAnalysis analyzeTransforms(COSDictionary signature) {
        COSArray references = asArray(signature.getDictionaryObject(COSName.getPDFName("Reference")));
        if (references == null) return new TransformAnalysis(List.of(), null, null, true);
        List<Map<String, Object>> transforms = new ArrayList<>();
        String certificationPermission = null;
        Map<String, Object> effectiveFieldLock = null;
        boolean complete = true;
        for (COSBase entry : references) {
            COSDictionary reference = asDictionary(entry);
            String methodName = reference == null ? null
                : reference.getNameAsString(COSName.getPDFName("TransformMethod"));
            String method = switch (methodName == null ? "" : methodName) {
                case "DocMDP" -> "DocMDP";
                case "FieldMDP" -> "FieldMDP";
                case "UR3" -> "UR3";
                default -> "unknown";
            };
            COSDictionary parameters = reference == null ? null
                : asDictionary(reference.getDictionaryObject(COSName.getPDFName("TransformParams")));
            boolean parsed = parameters != null && !"unknown".equals(method);
            String permission = "unknown";
            Map<String, Object> fieldLock = null;
            if ("DocMDP".equals(method) && parameters != null) {
                permission = certificationPermission(parameters.getInt(COSName.P, -1));
                certificationPermission = permission;
                parsed = !"unknown".equals(permission);
            } else if ("FieldMDP".equals(method) && parameters != null) {
                permission = "not-certified";
                fieldLock = parseFieldLock(parameters);
                effectiveFieldLock = fieldLock;
                parsed = fieldLock != null && !"unknown".equals(fieldLock.get("action"));
            }
            Map<String, Object> item = orderedMap();
            item.put("method", method);
            item.put("certificationPermission", permission);
            item.put("fieldLock", fieldLock);
            item.put("parsed", parsed);
            transforms.add(item);
            complete &= parsed;
        }
        return new TransformAnalysis(transforms, certificationPermission, effectiveFieldLock, complete);
    }
    private static Map<String, Object> parseFieldLock(COSDictionary parameters) {
        String rawAction = parameters.getNameAsString(COSName.getPDFName("Action"));
        String action = switch (rawAction == null ? "" : rawAction) {
            case "All" -> "all";
            case "Include" -> "include";
            case "Exclude" -> "exclude";
            default -> "unknown";
        };
        List<String> names = new ArrayList<>();
        COSArray fields = asArray(parameters.getDictionaryObject(COSName.FIELDS));
        if (fields != null) {
            for (COSBase entry : fields) {
                COSBase value = dereference(entry);
                if (value instanceof COSString string) names.add(safeClaim(string.getString()));
            }
        }
        Map<String, Object> lock = orderedMap();
        lock.put("action", action);
        lock.put("fieldNames", names);
        return lock;
    }
    /**
     * Produces a bounded structural inventory of the document security store.
     * This deliberately does not decode certificates or revocation bodies and
     * does not make a validity or binding claim from their presence.
     */
    static Map<String, Object> validationEvidenceInventory(PDDocument pdf) {
        COSDictionary catalog = pdf.getDocumentCatalog().getCOSObject();
        COSName dssName = COSName.getPDFName("DSS");
        COSName vriName = COSName.getPDFName("VRI");
        boolean dssPresent = catalog.containsKey(dssName);
        COSDictionary dss = dssPresent ? asDictionary(catalog.getItem(dssName)) : null;
        EvidenceInventoryState state = new EvidenceInventoryState();
        Map<String, Object> result = orderedMap();
        result.put("dssPresent", dssPresent);
        if (!dssPresent) {
            result.put("vriPresent", false);
            result.put("structureStatus", "absent");
            result.put("inventoryComplete", true);
            result.put("limitExceeded", false);
            result.put("certificates", evidenceCollection(false, 0, 0, 0, true));
            result.put("ocspResponses", evidenceCollection(false, 0, 0, 0, true));
            result.put("crls", evidenceCollection(false, 0, 0, 0, true));
            result.put("vriEntryCount", 0);
            result.put("vriEntries", List.of());
            return result;
        }
        if (dss == null) {
            state.malformed = true;
            result.put("vriPresent", null);
            result.put("structureStatus", "malformed");
            result.put("inventoryComplete", true);
            result.put("limitExceeded", false);
            result.put("certificates", null);
            result.put("ocspResponses", null);
            result.put("crls", null);
            result.put("vriEntryCount", null);
            result.put("vriEntries", List.of());
            return result;
        }
        result.put("certificates", inspectEvidenceCollection(dss, "Certs", state));
        result.put("ocspResponses", inspectEvidenceCollection(dss, "OCSPs", state));
        result.put("crls", inspectEvidenceCollection(dss, "CRLs", state));
        boolean vriPresent = dss.containsKey(vriName);
        result.put("vriPresent", vriPresent);
        List<Map<String, Object>> entries = new ArrayList<>();
        Integer vriEntryCount = 0;
        if (vriPresent) {
            COSDictionary vri = asDictionary(dss.getItem(vriName));
            if (vri == null) {
                state.malformed = true;
                vriEntryCount = null;
            } else {
                vriEntryCount = vri.size();
                if (vri.size() > Protocol.MAX_CONTAINER_ENTRIES) {
                    state.limitExceeded = true;
                } else {
                    List<COSName> keys = new ArrayList<>(vri.keySet());
                    keys.sort(Comparator.comparing(COSName::getName));
                    for (COSName key : keys) {
                        entries.add(inspectVriEntry(key, vri.getItem(key), state));
                    }
                }
            }
        }
        result.put("vriEntryCount", vriEntryCount);
        result.put("vriEntries", entries);
        result.put("structureStatus", state.malformed
            ? "malformed" : state.limitExceeded ? "indeterminate" : "well-formed");
        result.put("inventoryComplete", !state.limitExceeded);
        result.put("limitExceeded", state.limitExceeded);
        return result;
    }
    private static Map<String, Object> inspectVriEntry(
        COSName key,
        COSBase rawEntry,
        EvidenceInventoryState state
    ) {
        String keyReference = key.getName();
        boolean canonicalSha1Reference = keyReference.matches("[0-9A-F]{40}");
        COSDictionary entry = asDictionary(rawEntry);
        Map<String, Object> result = orderedMap();
        result.put("keyReference", keyReference.length() <= Protocol.MAX_REQUEST_ID_LENGTH
            ? keyReference : null);
        result.put("keyReferenceSha256", sha256(keyReference.getBytes(StandardCharsets.UTF_8)));
        result.put("keyReferenceFormat", canonicalSha1Reference ? "sha1-hex" : "other");
        if (entry == null) {
            state.malformed = true;
            result.put("structureStatus", "malformed");
            result.put("certificates", null);
            result.put("ocspResponses", null);
            result.put("crls", null);
        } else {
            Map<String, Object> certificates = inspectEvidenceCollection(entry, "Cert", state);
            Map<String, Object> ocspResponses = inspectEvidenceCollection(entry, "OCSP", state);
            Map<String, Object> crls = inspectEvidenceCollection(entry, "CRL", state);
            boolean malformed = !canonicalSha1Reference
                || collectionMalformed(certificates)
                || collectionMalformed(ocspResponses)
                || collectionMalformed(crls);
            boolean complete = collectionComplete(certificates)
                && collectionComplete(ocspResponses)
                && collectionComplete(crls);
            if (malformed) state.malformed = true;
            result.put("structureStatus", malformed
                ? "malformed" : complete ? "well-formed" : "indeterminate");
            result.put("certificates", certificates);
            result.put("ocspResponses", ocspResponses);
            result.put("crls", crls);
        }
        if (!canonicalSha1Reference) state.malformed = true;
        return result;
    }
    private static boolean collectionMalformed(Map<String, Object> collection) {
        Object malformedEntryCount = collection.get("malformedEntryCount");
        return malformedEntryCount instanceof Integer count && count > 0;
    }
    private static boolean collectionComplete(Map<String, Object> collection) {
        return Boolean.TRUE.equals(collection.get("inspectionComplete"));
    }
    private static Map<String, Object> inspectEvidenceCollection(
        COSDictionary parent,
        String key,
        EvidenceInventoryState state
    ) {
        COSName name = COSName.getPDFName(key);
        if (!parent.containsKey(name)) return evidenceCollection(false, 0, 0, 0, true);
        COSArray values = asArray(parent.getItem(name));
        if (values == null) {
            state.malformed = true;
            return evidenceCollection(true, null, null, 1, true);
        }
        int referenceCount = values.size();
        if (referenceCount > state.remainingReferences) {
            state.limitExceeded = true;
            return evidenceCollection(true, referenceCount, null, null, false);
        }
        state.remainingReferences -= referenceCount;
        int embeddedObjectCount = 0;
        int malformedEntryCount = 0;
        for (COSBase value : values) {
            if (dereference(value) instanceof COSStream) embeddedObjectCount++;
            else malformedEntryCount++;
        }
        if (malformedEntryCount > 0) state.malformed = true;
        return evidenceCollection(
            true,
            referenceCount,
            embeddedObjectCount,
            malformedEntryCount,
            true
        );
    }
    private static Map<String, Object> evidenceCollection(
        boolean present,
        Integer referenceCount,
        Integer embeddedObjectCount,
        Integer malformedEntryCount,
        boolean inspectionComplete
    ) {
        Map<String, Object> result = orderedMap();
        result.put("present", present);
        result.put("referenceCount", referenceCount);
        result.put("embeddedObjectCount", embeddedObjectCount);
        result.put("malformedEntryCount", malformedEntryCount);
        result.put("inspectionComplete", inspectionComplete);
        return result;
    }
    private static String signatureKind(PDSignature dictionary, boolean designatedCertification) {
        if ("ETSI.RFC3161".equals(dictionary.getSubFilter())) return "document-timestamp";
        if (designatedCertification) return "certification";
        return "approval";
    }
    private static boolean isCatalogCertification(PDDocument pdf, PDSignature dictionary) {
        COSDictionary permissions = asDictionary(
            pdf.getDocumentCatalog().getCOSObject().getDictionaryObject(COSName.getPDFName("Perms"))
        );
        if (permissions == null) return false;
        COSDictionary certification = asDictionary(
            permissions.getDictionaryObject(COSName.getPDFName("DocMDP"))
        );
        return certification != null && certification == dictionary.getCOSObject();
    }
    private static List<Map<String, Object>> certificateInventory(
        AdvancedSignature signature,
        CertificateToken signingCertificate
    ) {
        if (signature == null) return List.of();
        return certificateInventory(signature.getCertificates(), signingCertificate);
    }
    static List<Map<String, Object>> certificateInventory(
        Collection<CertificateToken> certificates,
        CertificateToken signingCertificate
    ) {
        List<Map<String, Object>> result = new ArrayList<>();
        Set<String> seen = new java.util.HashSet<>();
        for (CertificateToken token : certificates) {
            String fingerprint = sha256(token.getEncoded());
            if (!seen.add(fingerprint)) continue;
            Map<String, Object> item = orderedMap();
            item.put("subject", token.getCertificate().getSubjectX500Principal().getName());
            item.put("issuer", token.getCertificate().getIssuerX500Principal().getName());
            item.put("serialNumber", token.getSerialNumber().toString(16));
            item.put("sha256Fingerprint", fingerprint);
            item.put("validFrom", iso(token.getNotBefore()));
            item.put("validTo", iso(token.getNotAfter()));
            item.put("keyAlgorithm", token.getPublicKey().getAlgorithm());
            item.put("signingCertificate", token == signingCertificate || token.equals(signingCertificate));
            result.add(item);
        }
        result.sort(Comparator.comparing(item -> (String) item.get("sha256Fingerprint")));
        return result;
    }
    private static RevocationEvidence revocationEvidence(
        AdvancedSignature signature,
        CertificateToken signingCertificate
    ) {
        if (signature == null || signingCertificate == null) return RevocationEvidence.none();
        try {
            CertificateToken certificateIssuer = signature.getCertificates().stream()
                .filter(candidate -> !candidate.equals(signingCertificate))
                .filter(signingCertificate::isSignedBy)
                .findFirst()
                .orElse(null);
            if (certificateIssuer == null) return RevocationEvidence.none();
            RevocationEvidence best = RevocationEvidence.none();
            for (RevocationToken<?> token : signature.getCompleteCRLSource()
                .getRevocationTokens(signingCertificate, certificateIssuer)) {
                best = preferRevocationEvidence(best, verifiedRevocationEvidence(
                    token,
                    signingCertificate,
                    certificateIssuer,
                    false,
                    signature.getCRLSource().getAllRevocationBinariesWithOrigins()
                ));
            }
            for (RevocationToken<?> token : signature.getCompleteOCSPSource()
                .getRevocationTokens(signingCertificate, certificateIssuer)) {
                best = preferRevocationEvidence(best, verifiedRevocationEvidence(
                    token,
                    signingCertificate,
                    certificateIssuer,
                    true,
                    signature.getOCSPSource().getAllRevocationBinariesWithOrigins()
                ));
            }
            return best;
        } catch (Throwable exception) {
            throwIfFatal(exception);
            return RevocationEvidence.none();
        }
    }
    private static RevocationEvidence verifiedRevocationEvidence(
        RevocationToken<?> token,
        CertificateToken signingCertificate,
        CertificateToken certificateIssuer,
        boolean ocsp,
        Map<?, ?> embeddedBinariesWithOrigins
    ) {
        if (token == null
            || token.getRelatedCertificate() == null
            || !token.getRelatedCertificate().equals(signingCertificate)
            || token.getIssuerCertificateToken() == null
            || token.getStatus() == null
            || !token.getStatus().isKnown()
            || !isEmbeddedToken(token, embeddedBinariesWithOrigins)
            || !authorizedRevocationSigner(token, certificateIssuer, ocsp)
            || !token.isSignatureIntact()
            || !token.isValid()) {
            return RevocationEvidence.none();
        }
        String status = token.getStatus() == CertificateStatus.REVOKED ? "revoked" : "good";
        return new RevocationEvidence(status, true, token.getProductionDate(), token.getNextUpdate());
    }
    private static boolean authorizedRevocationSigner(
        RevocationToken<?> token,
        CertificateToken certificateIssuer,
        boolean ocsp
    ) {
        CertificateToken evidenceSigner = token.getIssuerCertificateToken();
        if (!token.isSignedBy(evidenceSigner)) return false;
        if (evidenceSigner.equals(certificateIssuer)) return true;
        if (!ocsp || !evidenceSigner.isSignedBy(certificateIssuer)) return false;
        try {
            List<String> extendedKeyUsage = evidenceSigner.getCertificate().getExtendedKeyUsage();
            return extendedKeyUsage != null && extendedKeyUsage.contains("1.3.6.1.5.5.7.3.9");
        } catch (CertificateParsingException exception) {
            return false;
        }
    }
    private static boolean isEmbeddedToken(RevocationToken<?> token, Map<?, ?> binariesWithOrigins) {
        for (Map.Entry<?, ?> entry : binariesWithOrigins.entrySet()) {
            if (!(entry.getKey() instanceof EncapsulatedRevocationTokenIdentifier<?> identifier)
                || !Arrays.equals(identifier.getBinaries(), token.getEncoded())
                || !(entry.getValue() instanceof Collection<?> origins)) {
                continue;
            }
            if (origins.stream().anyMatch(origin -> origin instanceof RevocationOrigin value
                && value.isInternalOrigin())) {
                return true;
            }
        }
        return false;
    }
    private static RevocationEvidence preferRevocationEvidence(
        RevocationEvidence current,
        RevocationEvidence candidate
    ) {
        if (candidate.status() == null) return current;
        if (current.status() == null || "revoked".equals(candidate.status()) && !"revoked".equals(current.status())) {
            return candidate;
        }
        if (!candidate.status().equals(current.status())) return current;
        if (current.producedAt() == null) return candidate;
        return candidate.producedAt() != null && candidate.producedAt().after(current.producedAt())
            ? candidate : current;
    }
    private static List<Map<String, Object>> timestampInventory(
        AdvancedSignature signature,
        Date claimedTime
    ) {
        List<Map<String, Object>> result = new ArrayList<>();
        if (claimedTime != null) {
            Map<String, Object> claimed = orderedMap();
            claimed.put("kind", "claimed-time");
            claimed.put("time", iso(claimedTime));
            claimed.put("verified", false);
            claimed.put("tsaClaim", null);
            result.add(claimed);
        }
        if (signature != null) {
            for (TimestampToken token : signature.getSignatureTimestamps()) {
                result.add(timestampItem(token, "signature-timestamp"));
            }
            for (TimestampToken token : signature.getDocumentTimestamps()) {
                result.add(timestampItem(token, "document-timestamp"));
            }
        }
        return result;
    }
    private static Map<String, Object> timestampItem(TimestampToken token, String kind) {
        CertificateToken tsa = null;
        boolean verified = false;
        try {
            var candidate = token.getCandidatesForSigningCertificate().getTheBestCandidate();
            tsa = candidate == null ? null : candidate.getCertificateToken();
            verified = tsa != null
                && token.isSignedBy(tsa)
                && token.isSignatureIntact()
                && token.isMessageImprintDataFound()
                && token.isMessageImprintDataIntact()
                && token.areReferenceValidationsValid();
        } catch (Throwable exception) {
            throwIfFatal(exception);
        }
        Map<String, Object> timestamp = orderedMap();
        timestamp.put("kind", kind);
        timestamp.put("time", iso(token.getGenerationTime()));
        timestamp.put("verified", verified);
        timestamp.put("tsaClaim", tsa == null ? null
            : safeNullableClaim(tsa.getCertificate().getSubjectX500Principal().getName()));
        return timestamp;
    }
    static String signingTimeStatus(List<Map<String, Object>> timestamps, Date claimedTime) {
        for (Map<String, Object> timestamp : timestamps) {
            if (Boolean.TRUE.equals(timestamp.get("verified"))
                && "document-timestamp".equals(timestamp.get("kind"))) {
                return "document-timestamp-verified";
            }
        }
        for (Map<String, Object> timestamp : timestamps) {
            if (Boolean.TRUE.equals(timestamp.get("verified"))
                && "signature-timestamp".equals(timestamp.get("kind"))) {
                return "timestamp-verified";
            }
        }
        return claimedTime == null ? "missing" : "claimed-only";
    }
    private static String padesProfile(AdvancedSignature signature) {
        if (signature == null) return "unknown";
        if (signature.hasLTAProfile()) return "B-LTA";
        if (signature.hasLTProfile()) return "B-LT";
        if (signature.hasTProfile()) return "B-T";
        if (signature.hasBProfile()) return "B-B";
        return "legacy";
    }
    private static String certificateStatus(
        CertificateToken certificate,
        Date signingTime,
        Instant validationTime
    ) {
        if (certificate == null) return "unknown";
        try {
            if (signingTime != null) certificate.getCertificate().checkValidity(signingTime);
        } catch (CertificateNotYetValidException exception) {
            return "not-yet-valid";
        } catch (CertificateExpiredException exception) {
            return "expired-at-signing";
        }
        try {
            certificate.getCertificate().checkValidity(Date.from(validationTime));
        } catch (CertificateNotYetValidException exception) {
            return "not-yet-valid";
        } catch (CertificateExpiredException exception) {
            return "expired-now";
        }
        return "offline";
    }
    private static List<List<Long>> byteRangeSegments(int[] byteRange) {
        if (byteRange == null || byteRange.length % 2 != 0) return List.of();
        List<List<Long>> segments = new ArrayList<>();
        for (int index = 0; index < byteRange.length; index += 2) {
            segments.add(List.of((long) byteRange[index], (long) byteRange[index + 1]));
        }
        return segments;
    }
    private static boolean validByteRange(int[] byteRange, long fileLength) {
        if (byteRange == null || byteRange.length < 4 || byteRange.length % 2 != 0) return false;
        long priorEnd = -1;
        for (int index = 0; index < byteRange.length; index += 2) {
            long offset = byteRange[index];
            long length = byteRange[index + 1];
            if (offset < 0 || length < 0 || offset < priorEnd) return false;
            long end = offset + length;
            if (end < offset || end > fileLength) return false;
            priorEnd = end;
        }
        return byteRange[0] == 0;
    }
    private static long coveredRevisionEnd(int[] byteRange) {
        if (byteRange == null || byteRange.length < 2) return -1;
        int index = byteRange.length - 2;
        return (long) byteRange[index] + byteRange[index + 1];
    }
    private static String certificationPermission(int permission) {
        return switch (permission) {
            case 1 -> "no-changes";
            case 2 -> "form-filling-and-signatures";
            case 3 -> "form-filling-signatures-and-annotations";
            default -> "unknown";
        };
    }
    private static InputSnapshot readInput(String inputPath) throws ValidationException {
        if (inputPath == null || inputPath.isBlank() || inputPath.length() > Protocol.MAX_PATH_LENGTH) {
            throw new ValidationException("INVALID_INPUT_PATH");
        }
        Path path;
        try {
            path = Path.of(inputPath);
        } catch (RuntimeException exception) {
            throw new ValidationException("INVALID_INPUT_PATH");
        }
        if (!path.isAbsolute()) throw new ValidationException("INVALID_INPUT_PATH");
        BasicFileAttributes attributes;
        try {
            attributes = Files.readAttributes(path, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS);
        } catch (IOException exception) {
            throw new ValidationException("INPUT_UNAVAILABLE");
        }
        if (attributes.isSymbolicLink()) throw new ValidationException("INPUT_SYMLINK_REJECTED");
        if (!attributes.isRegularFile()) throw new ValidationException("INPUT_NOT_REGULAR_FILE");
        if (attributes.size() > Protocol.MAX_INPUT_BYTES) throw new ValidationException("INPUT_TOO_LARGE");
        MessageDigest digest = sha256Digest();
        ByteArrayOutputStream bytes = new ByteArrayOutputStream((int) attributes.size());
        try (SeekableByteChannel channel = Files.newByteChannel(
            path,
            Set.<OpenOption>of(StandardOpenOption.READ, LinkOption.NOFOLLOW_LINKS)
        )) {
            ByteBuffer buffer = ByteBuffer.allocate(64 * 1024);
            long total = 0;
            while (channel.read(buffer) != -1) {
                buffer.flip();
                byte[] chunk = new byte[buffer.remaining()];
                buffer.get(chunk);
                buffer.clear();
                total += chunk.length;
                if (total > Protocol.MAX_INPUT_BYTES) throw new ValidationException("INPUT_TOO_LARGE");
                digest.update(chunk);
                bytes.write(chunk);
            }
        } catch (ValidationException exception) {
            throw exception;
        } catch (IOException exception) {
            throw new ValidationException("INPUT_READ_FAILED");
        }
        return new InputSnapshot(bytes.toByteArray(), HexFormat.of().formatHex(digest.digest()));
    }
    private static Map<String, Object> issue(
        String code,
        String severity,
        String message,
        String signatureId
    ) {
        Map<String, Object> issue = orderedMap();
        issue.put("code", code);
        issue.put("severity", severity);
        issue.put("message", message);
        if (signatureId != null) issue.put("signatureId", signatureId);
        return issue;
    }
    private static Date calendarDate(Calendar calendar) {
        return calendar == null ? null : calendar.getTime();
    }
    private static String iso(Date date) {
        return date == null ? null : date.toInstant().toString();
    }
    private static String safeClaim(String value) {
        String safe = safeNullableClaim(value);
        return safe == null ? "" : safe;
    }
    private static String safeNullableClaim(String value) {
        if (value == null) return null;
        String normalized = value.replaceAll("[\\p{Cntrl}&&[^\\t]]", " ").strip();
        return normalized.length() <= 1024 ? normalized : normalized.substring(0, 1024);
    }
    private static String sha256(byte[] bytes) {
        return HexFormat.of().formatHex(sha256Digest().digest(bytes));
    }
    private static MessageDigest sha256Digest() {
        try {
            return MessageDigest.getInstance("SHA-256");
        } catch (NoSuchAlgorithmException impossible) {
            throw new IllegalStateException("SHA-256 unavailable", impossible);
        }
    }
    private static COSBase dereference(COSBase value) {
        while (value instanceof COSObject object) value = object.getObject();
        return value;
    }
    private static COSDictionary asDictionary(COSBase value) {
        COSBase direct = dereference(value);
        return direct instanceof COSDictionary dictionary ? dictionary : null;
    }
    private static COSArray asArray(COSBase value) {
        COSBase direct = dereference(value);
        return direct instanceof COSArray array ? array : null;
    }
    private static <K, V> Map<K, V> orderedMap() {
        return new LinkedHashMap<>();
    }
    private static void throwIfFatal(Throwable exception) {
        if (exception instanceof VirtualMachineError fatal) throw fatal;
        if (exception instanceof ThreadDeath fatal) throw fatal;
    }
    private record InputSnapshot(byte[] bytes, String sha256) {}
    private static final class EvidenceInventoryState {
        private int remainingReferences = Protocol.MAX_STRUCTURAL_MARKERS;
        private boolean malformed;
        private boolean limitExceeded;
    }
    private record DssAnalysis(PDFDocumentValidator validator, List<AdvancedSignature> signatures, boolean failure) {}
    private record RevocationEvidence(String status, boolean embedded, Date producedAt, Date nextUpdateAt) {
        static RevocationEvidence none() {
            return new RevocationEvidence(null, false, null, null);
        }
    }
    private record TransformAnalysis(
        List<Map<String, Object>> transforms,
        String certificationPermission,
        Map<String, Object> fieldLock,
        boolean complete
    ) {}
    private record RevisionAnalysis(int totalRevisions, boolean complete, List<Integer> signedRevisions) {
        static RevisionAnalysis incomplete(int signatureCount) {
            return new RevisionAnalysis(0, false, Arrays.asList(new Integer[signatureCount]));
        }
        Integer signedRevision(int index) {
            return index < signedRevisions.size() ? signedRevisions.get(index) : null;
        }
    }
}
