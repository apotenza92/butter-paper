package com.butterpaper.signaturecore;

import com.fasterxml.jackson.databind.JsonNode;
import eu.europa.esig.dss.enumerations.CertificationPermission;
import eu.europa.esig.dss.enumerations.DigestAlgorithm;
import eu.europa.esig.dss.enumerations.SignatureLevel;
import eu.europa.esig.dss.model.DSSDocument;
import eu.europa.esig.dss.model.InMemoryDocument;
import eu.europa.esig.dss.model.SignatureValue;
import eu.europa.esig.dss.model.ToBeSigned;
import eu.europa.esig.dss.pades.PAdESSignatureParameters;
import eu.europa.esig.dss.pades.SignatureImageParameters;
import eu.europa.esig.dss.pades.signature.PAdESService;
import org.apache.pdfbox.Loader;
import org.apache.pdfbox.cos.COSDictionary;
import org.apache.pdfbox.cos.COSName;
import org.apache.pdfbox.pdmodel.PDDocument;
import org.apache.pdfbox.pdmodel.PDPage;
import org.apache.pdfbox.pdmodel.common.PDRectangle;
import org.apache.pdfbox.pdmodel.interactive.annotation.PDAnnotationWidget;
import org.apache.pdfbox.pdmodel.interactive.form.PDSignatureField;

import javax.imageio.ImageIO;
import javax.imageio.ImageReader;
import javax.imageio.stream.ImageInputStream;
import javax.imageio.stream.MemoryCacheImageInputStream;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.security.interfaces.ECKey;
import java.security.interfaces.RSAKey;
import java.time.Clock;
import java.util.Arrays;
import java.util.Date;
import java.util.HashSet;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;

final class SigningService {
    private record SourceExpectation(
        SignedMutationPostcheck.AppearanceExpectation appearance,
        SignatureFieldSpec.FieldLock fieldLock
    ) {}

    static final class SigningException extends Exception {
        private final String code;

        SigningException(String code) {
            super(code);
            this.code = code;
        }

        SigningException(String code, Throwable cause) {
            super(code, cause);
            this.code = code;
        }

        String code() { return code; }
    }

    @FunctionalInterface
    interface SignaturePostcheck {
        Map<String, Object> verify(
            byte[] source,
            byte[] output,
            String expectedFieldName,
            String expectedCertificateSha256,
            boolean certification,
            Integer expectedCertificationPermission,
            SignedMutationPostcheck.AppearanceExpectation appearance,
            SignatureFieldSpec.FieldLock expectedFieldLock
        ) throws SignedMutationPostcheck.PostcheckException;
    }

    private final Pkcs12IdentityService identities;
    private final SafePdfMutation safeFiles;
    private final SignaturePostcheck postcheck;
    private final SignatureFieldService fields;
    private final Clock clock;

    SigningService(Pkcs12PasswordPrompt prompt) {
        this(prompt, Clock.systemUTC());
    }

    SigningService(Pkcs12PasswordPrompt prompt, Clock clock) {
        this(prompt, clock, new SafePdfMutation(), new SignedMutationPostcheck()::verifySignature);
    }

    SigningService(
        Pkcs12PasswordPrompt prompt,
        Clock clock,
        SafePdfMutation safeFiles,
        SignaturePostcheck postcheck
    ) {
        this.identities = new Pkcs12IdentityService(prompt);
        this.safeFiles = java.util.Objects.requireNonNull(safeFiles);
        this.postcheck = java.util.Objects.requireNonNull(postcheck);
        this.fields = new SignatureFieldService();
        this.clock = clock;
    }

    Map<String, Object> inspectPkcs12(byte[] pkcs12) throws SigningException {
        try (Pkcs12IdentityService.UnlockedToken unlocked = identities.unlock(pkcs12)) {
            Map<String, Object> result = new LinkedHashMap<>();
            result.put("provider", "pkcs12");
            result.put("identities", identities.describe(unlocked));
            result.put("passwordRemembered", false);
            result.put("privateKeyExported", false);
            result.put("engineVersion", Protocol.ENGINE_VERSION);
            return result;
        } catch (Pkcs12IdentityService.IdentityException exception) {
            throw new SigningException(exception.code());
        }
    }

    Map<String, Object> sign(JsonNode payload, Map<String, byte[]> frames, boolean certification)
        throws SigningException {
        byte[] pkcs12 = frames.get("pkcs12");
        byte[] appearance = frames.get("appearance");
        try {
            Request request = Request.parse(payload, certification);
            requiredFrame(frames, "pkcs12");
            if ((request.appearanceVisible() && appearance == null)
                || (!request.appearanceVisible() && appearance != null)) {
                throw new SigningException("APPEARANCE_FRAME_MISMATCH");
            }
            if (appearance != null) validateAppearance(appearance);

            SafePdfMutation.Input input;
            SafePdfMutation.Output output;
            SourceExpectation sourceExpectation;
            try {
                input = safeFiles.readInput(request.inputPath(), request.expectedInputSha256());
                output = safeFiles.validateOutput(input, request.outputPath());
                sourceExpectation = assertSourcePolicy(
                    input.bytes(), request.field(), certification, request.appearanceVisible()
                );
            } catch (SafePdfMutation.MutationException exception) {
                throw new SigningException(exception.code());
            }

            byte[] preparedBytes = input.bytes();
            byte[] signedBytes = null;
            byte[] reopenedBytes = null;
            boolean outputWritten = false;
            try (Pkcs12IdentityService.UnlockedToken unlocked = identities.unlock(pkcs12)) {
                Pkcs12IdentityService.Identity identity = unlocked.select(request.certificateSha256());
                assertAlgorithm(identity);
                if (!request.field().existing()) preparedBytes = fields.addField(input.bytes(), request.field());
                PAdESService service = new PAdESService(ValidationService.newOfflineVerifier());
                PAdESSignatureParameters parameters = parameters(request, identity, appearance);
                InMemoryDocument document = new InMemoryDocument(preparedBytes, "document.pdf");
                ToBeSigned data = service.getDataToSign(document, parameters);
                SignatureValue signature = unlocked.token().sign(data, request.digest(), identity.key());
                DSSDocument signed = service.signDocument(document, parameters, signature);
                ByteArrayOutputStream encoded = new ByteArrayOutputStream(input.bytes().length + 64 * 1024);
                signed.writeTo(encoded);
                signedBytes = encoded.toByteArray();

                postcheck.verify(
                    input.bytes(),
                    signedBytes,
                    request.field().name(),
                    request.certificateSha256(),
                    certification,
                    request.certificationPermissionCode(),
                    sourceExpectation.appearance(),
                    sourceExpectation.fieldLock()
                );
                String outputSha256 = sha256(signedBytes);
                safeFiles.write(output, input, signedBytes);
                outputWritten = true;
                safeFiles.verifyInputIdentity(input);
                reopenedBytes = safeFiles.readOutput(output, outputSha256);
                Map<String, Object> structural = postcheck.verify(
                    input.bytes(),
                    reopenedBytes,
                    request.field().name(),
                    request.certificateSha256(),
                    certification,
                    request.certificationPermissionCode(),
                    sourceExpectation.appearance(),
                    sourceExpectation.fieldLock()
                );

                Map<String, Object> result = new LinkedHashMap<>();
                result.put("inputSha256", input.sha256());
                result.put("outputSha256", outputSha256);
                result.put("outputBytes", reopenedBytes.length);
                result.put("sourcePreserved", true);
                result.put("validatedOutput", true);
                result.put("appendOnly", true);
                result.put("kind", certification ? "certification" : "approval");
                result.put("profile", "PAdES-B-B");
                result.put("digestAlgorithm", canonicalDigestName(request.digest()));
                result.put("fieldName", request.field().name());
                result.put("certificateSha256", request.certificateSha256());
                result.put("postcheck", structural);
                result.put("engineVersion", Protocol.ENGINE_VERSION);
                return result;
            } catch (Pkcs12IdentityService.IdentityException exception) {
                if (outputWritten) safeFiles.discard(output);
                throw new SigningException(exception.code());
            } catch (SignedMutationPostcheck.PostcheckException exception) {
                if (outputWritten) safeFiles.discard(output);
                throw new SigningException(exception.code());
            } catch (SafePdfMutation.MutationException exception) {
                if (outputWritten) safeFiles.discard(output);
                throw new SigningException(exception.code());
            } catch (SignatureFieldService.FieldException exception) {
                if (outputWritten) safeFiles.discard(output);
                throw new SigningException(exception.code());
            } catch (IOException | RuntimeException exception) {
                if (outputWritten) safeFiles.discard(output);
                throw new SigningException("SIGNING_FAILED", exception);
            } finally {
                if (signedBytes != null) Arrays.fill(signedBytes, (byte) 0);
                if (reopenedBytes != null) Arrays.fill(reopenedBytes, (byte) 0);
                if (preparedBytes != input.bytes()) Arrays.fill(preparedBytes, (byte) 0);
                Arrays.fill(input.bytes(), (byte) 0);
            }
        } finally {
            if (pkcs12 != null) Arrays.fill(pkcs12, (byte) 0);
            if (appearance != null) Arrays.fill(appearance, (byte) 0);
        }
    }

    private PAdESSignatureParameters parameters(
        Request request,
        Pkcs12IdentityService.Identity identity,
        byte[] appearance
    ) throws SigningException {
        PAdESSignatureParameters parameters = new PAdESSignatureParameters();
        parameters.setSignatureLevel(SignatureLevel.PAdES_BASELINE_B);
        parameters.setDigestAlgorithm(request.digest());
        parameters.setSigningCertificate(identity.key().getCertificate());
        parameters.setCertificateChain(identity.key().getCertificateChain());
        parameters.bLevel().setSigningDate(Date.from(clock.instant()));
        parameters.setAppName("Butter Paper");
        if (request.reason() != null) parameters.setReason(request.reason());
        if (request.location() != null) parameters.setLocation(request.location());
        if (request.contact() != null) parameters.setContactInfo(request.contact());
        if (request.certificationPermissionCode() != null) {
            parameters.setPermission(CertificationPermission.fromCode(request.certificationPermissionCode()));
        }

        SignatureImageParameters image = new SignatureImageParameters();
        image.setFieldParameters(request.field().toDssParameters());
        if (appearance != null) image.setImage(new InMemoryDocument(appearance, "appearance.png"));
        parameters.setImageParameters(image);
        return parameters;
    }

    private static void assertAlgorithm(Pkcs12IdentityService.Identity identity)
        throws SigningException {
        var key = identity.key().getCertificate().getPublicKey();
        if (key instanceof RSAKey rsa && rsa.getModulus().bitLength() >= 2048) return;
        if (key instanceof ECKey ec && ec.getParams().getOrder().bitLength() >= 256) return;
        throw new SigningException("UNSUPPORTED_SIGNING_KEY");
    }

    private static String canonicalDigestName(DigestAlgorithm digest) throws SigningException {
        return switch (digest) {
            case SHA256 -> "SHA-256";
            case SHA384 -> "SHA-384";
            case SHA512 -> "SHA-512";
            default -> throw new SigningException("UNSUPPORTED_DIGEST_ALGORITHM");
        };
    }

    private static SourceExpectation assertSourcePolicy(
        byte[] source,
        SignatureFieldSpec target,
        boolean certification,
        boolean visible
    )
        throws SigningException {
        try (PDDocument document = Loader.loadPDF(source)) {
            assertSupportedSourceSerialization(document);
            COSDictionary permissions = asDictionary(document.getDocumentCatalog().getCOSObject()
                .getDictionaryObject(COSName.getPDFName("Perms")));
            COSDictionary docMdp = permissions == null ? null
                : asDictionary(permissions.getDictionaryObject(COSName.getPDFName("DocMDP")));
            List<org.apache.pdfbox.pdmodel.interactive.digitalsignature.PDSignature> signatures =
                document.getSignatureDictionaries();
            if (certification) {
                if (!signatures.isEmpty() || docMdp != null) {
                    throw new SigningException("CERTIFICATION_REQUIRES_UNSIGNED_SOURCE");
                }
            } else if (!signatures.isEmpty() || docMdp != null) {
                assertAuthoritativePriorPolicies(source, target.name());
            }

            SignedMutationPostcheck.AppearanceExpectation appearanceExpectation;
            SignatureFieldSpec.FieldLock fieldLock;
            if (target.existing()) {
                PDSignatureField field = document.getSignatureFields().stream()
                    .filter(candidate -> target.name().equals(candidate.getFullyQualifiedName()))
                    .findFirst()
                    .orElseThrow(() -> new SigningException("SIGNATURE_FIELD_NOT_FOUND"));
                if (field.getSignature() != null) throw new SigningException("SIGNATURE_FIELD_ALREADY_SIGNED");
                appearanceExpectation = appearanceExpectation(document, field, visible);
                try {
                    fieldLock = SignatureFieldService.fieldLock(field);
                } catch (SignatureFieldService.FieldException exception) {
                    throw new SigningException(exception.code());
                }
            } else if (!signatures.isEmpty()) {
                throw new SigningException("NEW_FIELD_ON_SIGNED_SOURCE_BLOCKED");
            } else {
                if (visible != target.hasVisibleWidget()) {
                    throw new SigningException("APPEARANCE_WIDGET_MISMATCH");
                }
                appearanceExpectation = new SignedMutationPostcheck.AppearanceExpectation(
                    visible,
                    target.pageIndex(),
                    target.x(),
                    target.y(),
                    target.width(),
                    target.height(),
                    target.pageRotation()
                );
                fieldLock = target.lock();
            }
            return new SourceExpectation(appearanceExpectation, fieldLock);
        } catch (SigningException exception) {
            throw exception;
        } catch (IOException | RuntimeException exception) {
            throw new SigningException("SOURCE_POLICY_INDETERMINATE");
        }
    }

    /**
     * The initial signing release is deliberately limited to classic xref
     * tables. PDFBox/DSS can preserve and sign some xref/object-stream inputs,
     * but strict independent readers can reject the resulting object identity
     * history. Refuse those inputs before unlocking the signing identity until
     * a dedicated allocator/writer has been qualified across all readers.
     */
    static void assertSupportedSourceSerialization(PDDocument document) throws SigningException {
        var cos = document.getDocument();
        if (cos.isXRefStream()
            || cos.hasHybridXRef()
            || cos.getLinearizedDictionary() != null
            || !cos.getObjectsByType(COSName.OBJ_STM).isEmpty()
            || cos.getTrailer() != null && cos.getTrailer().containsKey(COSName.XREF_STM)) {
            throw new SigningException("SOURCE_SERIALIZATION_UNSUPPORTED");
        }
    }

    private static void assertAuthoritativePriorPolicies(
        byte[] source,
        String targetName
    ) throws SigningException {
        AuthoritativeSignedPolicies.assertAllowsApproval(source, targetName);
    }

    private static SignedMutationPostcheck.AppearanceExpectation appearanceExpectation(
        PDDocument document,
        PDSignatureField field,
        boolean visible
    ) throws SigningException {
        List<PDAnnotationWidget> effective = field.getWidgets().stream()
            .filter(widget -> widget.getRectangle() != null
                && widget.getRectangle().getWidth() > 0
                && widget.getRectangle().getHeight() > 0)
            .toList();
        if (!visible) {
            if (!effective.isEmpty()) throw new SigningException("APPEARANCE_WIDGET_MISMATCH");
            return new SignedMutationPostcheck.AppearanceExpectation(false, null, null, null, null, null, 0);
        }
        if (effective.size() != 1 || effective.getFirst().getPage() == null) {
            throw new SigningException("APPEARANCE_WIDGET_MISMATCH");
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
        if (!found) throw new SigningException("APPEARANCE_WIDGET_MISMATCH");
        PDRectangle rectangle = widget.getRectangle();
        try {
            SignatureFieldService.assertGeometry(page, rectangle, null);
        } catch (SignatureFieldService.FieldException exception) {
            throw new SigningException(exception.code());
        }
        return new SignedMutationPostcheck.AppearanceExpectation(
            true,
            pageIndex,
            rectangle.getLowerLeftX(),
            rectangle.getLowerLeftY(),
            rectangle.getWidth(),
            rectangle.getHeight(),
            ((page.getRotation() % 360) + 360) % 360
        );
    }

    private static COSDictionary asDictionary(Object value) {
        return value instanceof COSDictionary dictionary ? dictionary : null;
    }

    static void validateAppearance(byte[] bytes) throws SigningException {
        if (bytes.length == 0 || bytes.length > 16 * 1024 * 1024) throw new SigningException("INVALID_APPEARANCE");
        boolean png = bytes.length >= 8
            && bytes[0] == (byte) 0x89 && bytes[1] == 0x50 && bytes[2] == 0x4e && bytes[3] == 0x47;
        boolean jpeg = bytes.length >= 3
            && bytes[0] == (byte) 0xff && bytes[1] == (byte) 0xd8 && bytes[2] == (byte) 0xff;
        if (!png && !jpeg) throw new SigningException("INVALID_APPEARANCE");
        ImageReader reader = null;
        try (ImageInputStream stream = new MemoryCacheImageInputStream(new ByteArrayInputStream(bytes))) {
            Iterator<ImageReader> readers = ImageIO.getImageReaders(stream);
            if (!readers.hasNext()) throw new SigningException("INVALID_APPEARANCE");
            reader = readers.next();
            reader.setInput(stream, true, true);
            int width = reader.getWidth(0);
            int height = reader.getHeight(0);
            if (width <= 0 || height <= 0 || (long) width * height > 20_000_000L) {
                throw new SigningException("INVALID_APPEARANCE");
            }
            var image = reader.read(0);
            if (image == null || image.getWidth() != width || image.getHeight() != height) {
                throw new SigningException("INVALID_APPEARANCE");
            }
        } catch (IOException | RuntimeException exception) {
            throw new SigningException("INVALID_APPEARANCE");
        } finally {
            if (reader != null) reader.dispose();
        }
    }

    private static byte[] requiredFrame(Map<String, byte[]> frames, String id) throws SigningException {
        byte[] value = frames.get(id);
        if (value == null) throw new SigningException("MISSING_SECRET_FRAME");
        return value;
    }

    private static String sha256(byte[] bytes) throws SigningException {
        try {
            return SafePdfMutation.sha256(bytes);
        } catch (SafePdfMutation.MutationException exception) {
            throw new SigningException(exception.code());
        }
    }

    private record Request(
        String inputPath,
        String outputPath,
        String expectedInputSha256,
        String certificateSha256,
        DigestAlgorithm digest,
        SignatureFieldSpec field,
        String reason,
        String location,
        String contact,
        boolean appearanceVisible,
        Integer certificationPermissionCode
    ) {
        static Request parse(JsonNode payload, boolean certification) throws SigningException {
            if (payload == null || !payload.isObject()) throw new SigningException("INVALID_SIGNING_REQUEST");
            try {
                Set<String> required = new HashSet<>(Set.of(
                    "inputPath", "outputPath", "expectedInputSha256", "certificateSha256",
                    "digestAlgorithm", "profile", "field", "appearance"
                ));
                if (certification) required.add("certificationPermission");
                assertExactKeys(payload, required, Set.of("reason", "location", "contact"));
                String inputPath = requiredText(payload, "inputPath", 4_096);
                String outputPath = requiredText(payload, "outputPath", 4_096);
                String expectedHash = requiredSha(payload, "expectedInputSha256");
                String certificateHash = requiredSha(payload, "certificateSha256");
                DigestAlgorithm digest = switch (requiredText(payload, "digestAlgorithm", 16)) {
                    case "SHA-256" -> DigestAlgorithm.SHA256;
                    case "SHA-384" -> DigestAlgorithm.SHA384;
                    case "SHA-512" -> DigestAlgorithm.SHA512;
                    default -> throw new IllegalArgumentException("digest");
                };
                if (!"PAdES-B-B".equals(requiredText(payload, "profile", 32))) {
                    throw new IllegalArgumentException("profile");
                }
                SignatureFieldSpec field = SignatureFieldSpec.parse(payload.get("field"));
                if (field.existing() && field.lock() != null) throw new IllegalArgumentException("lock");
                String reason = optionalText(payload, "reason", 1_024);
                String location = optionalText(payload, "location", 512);
                String contact = optionalText(payload, "contact", 512);
                String appearance = requiredText(payload, "appearance", 16);
                if (!List.of("visible", "invisible").contains(appearance)) {
                    throw new IllegalArgumentException("appearance");
                }
                boolean visible = "visible".equals(appearance);
                if (!field.existing() && visible != field.hasVisibleWidget()) {
                    throw new IllegalArgumentException("appearance widget");
                }
                Integer permission = null;
                JsonNode permissionNode = payload.get("certificationPermission");
                if (certification) {
                    permission = switch (requiredText(payload, "certificationPermission", 64)) {
                        case "no-changes" -> 1;
                        case "form-filling-and-signatures" -> 2;
                        case "form-filling-signatures-and-annotations" -> 3;
                        default -> throw new IllegalArgumentException("permission");
                    };
                } else if (permissionNode != null) {
                    throw new IllegalArgumentException("approval permission");
                }
                return new Request(
                    inputPath, outputPath, expectedHash, certificateHash, digest, field,
                    reason, location, contact, visible, permission
                );
            } catch (IllegalArgumentException exception) {
                throw new SigningException("INVALID_SIGNING_REQUEST");
            }
        }

        private static void assertExactKeys(JsonNode payload, Set<String> required, Set<String> optional) {
            Set<String> observed = new HashSet<>();
            payload.fieldNames().forEachRemaining(observed::add);
            Set<String> allowed = new HashSet<>(required);
            allowed.addAll(optional);
            if (!observed.containsAll(required) || !allowed.containsAll(observed)) {
                throw new IllegalArgumentException("payload shape");
            }
        }

        private static String requiredSha(JsonNode payload, String name) {
            String value = requiredText(payload, name, 64);
            if (!value.matches("[a-f0-9]{64}")) throw new IllegalArgumentException(name);
            return value;
        }

        private static String requiredText(JsonNode payload, String name, int max) {
            JsonNode node = payload.get(name);
            if (node == null || !node.isTextual() || node.textValue().isBlank()
                || node.textValue().length() > max || containsUnsafeControl(node.textValue())) {
                throw new IllegalArgumentException(name);
            }
            return node.textValue();
        }

        private static String optionalText(JsonNode payload, String name, int max) {
            JsonNode node = payload.get(name);
            if (node == null || node.isNull() || (node.isTextual() && node.textValue().isEmpty())) return null;
            return requiredText(payload, name, max);
        }

        private static boolean containsUnsafeControl(String value) {
            return value.codePoints().anyMatch(code -> Character.isISOControl(code));
        }
    }
}
