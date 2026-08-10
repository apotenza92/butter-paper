package com.butterpaper.signaturecore;

import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.regex.Pattern;

/** Read-only, credential-free validation for one completed incremental signature mutation. */
final class SignedMutationPostvalidationService {
    /** Matches the existing-field contract used by SignatureFieldSpec and the TS client. */
    private static final Pattern FIELD_NAME = Pattern.compile("[^\\p{Cntrl}]{1,512}");
    private static final Pattern SHA256 = Pattern.compile("[a-f0-9]{64}");

    private final SafePdfMutation files;
    private final SignedMutationPostcheck postcheck;

    SignedMutationPostvalidationService(SafePdfMutation files) {
        this.files = files;
        this.postcheck = new SignedMutationPostcheck();
    }

    Map<String, Object> validate(
        String inputPath,
        String outputPath,
        String expectedInputSha256,
        String expectedOutputSha256,
        String expectedFieldName
    ) throws SafePdfMutation.MutationException, SignedMutationPostcheck.PostcheckException {
        // Keep the legacy dispatcher entry point fail-closed. It cannot bind
        // the newly added signature to the certificate selected by main.
        throw new SignedMutationPostcheck.PostcheckException("INVALID_POSTVALIDATION_REQUEST");
    }

    Map<String, Object> validate(
        String inputPath,
        String outputPath,
        String expectedInputSha256,
        String expectedOutputSha256,
        String expectedFieldName,
        String expectedCertificateSha256,
        String expectedOperation,
        String expectedAppearance,
        Integer expectedCertificationPermission
    ) throws SafePdfMutation.MutationException, SignedMutationPostcheck.PostcheckException {
        if (expectedFieldName == null || expectedFieldName.trim().isEmpty()
            || !FIELD_NAME.matcher(expectedFieldName).matches()) {
            throw new SignedMutationPostcheck.PostcheckException("INVALID_POSTVALIDATION_REQUEST");
        }
        if (expectedCertificateSha256 == null || !SHA256.matcher(expectedCertificateSha256).matches()) {
            throw new SignedMutationPostcheck.PostcheckException("INVALID_POSTVALIDATION_REQUEST");
        }
        SafePdfMutation.Input input = files.readInput(inputPath, expectedInputSha256);
        SafePdfMutation.Input output = null;
        try {
            output = files.readPostvalidationOutput(input, outputPath, expectedOutputSha256);
            postcheck.verifyIndependent(
                input.bytes(), output.bytes(), expectedFieldName, expectedCertificateSha256,
                expectedOperation, expectedAppearance, expectedCertificationPermission
            );
            files.verifyInputIdentity(input);
            files.verifyPostvalidationOutputIdentity(output);

            Map<String, Object> result = new LinkedHashMap<>();
            result.put("inputSha256", input.sha256());
            result.put("outputSha256", output.sha256());
            result.put("fieldName", expectedFieldName);
            result.put("certificateSha256", expectedCertificateSha256);
            result.put("inputPrefixPreserved", true);
            result.put("addedSignatureCount", 1);
            result.put("priorSignaturesPreserved", true);
            result.put("newSignatureCoversOutputExceptContents", true);
            result.put("cryptographicallyValid", true);
            result.put("structurallyReadable", true);
            result.put("independentProcess", true);
            result.put("validator", "pdf-signature-core-v1-validate-plus-main-prefix");
            return result;
        } finally {
            Arrays.fill(input.bytes(), (byte) 0);
            if (output != null) Arrays.fill(output.bytes(), (byte) 0);
        }
    }
}
