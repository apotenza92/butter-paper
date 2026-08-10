package com.butterpaper.signaturecore;

import com.fasterxml.jackson.core.JsonProcessingException;
import com.fasterxml.jackson.core.StreamReadConstraints;
import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.MapperFeature;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.SerializationFeature;
import com.fasterxml.jackson.databind.node.ObjectNode;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.ByteBuffer;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.regex.Pattern;

/** One-shot length-framed protocol for requests carrying bounded secret bytes. */
final class FramedProtocolServer {
    static final byte[] MAGIC = new byte[]{'B', 'P', 'S', '2'};
    static final int VERSION = 2;
    static final int MAX_HEADER_BYTES = 1_048_576;
    static final int MAX_FRAME_COUNT = 3;
    static final int MAX_FRAME_BYTES = 16 * 1024 * 1024;
    static final int MAX_TOTAL_FRAME_BYTES = 32 * 1024 * 1024;
    static final int MAX_RESPONSE_BYTES = 16 * 1024 * 1024;
    private static final Pattern SAFE_REQUEST_ID = Pattern.compile("[A-Za-z0-9._:-]{1,128}");
    private static final Pattern SAFE_FRAME_ID = Pattern.compile("[a-z][a-z0-9-]{0,31}");
    private static final Set<String> DISABLED_OPERATIONS = Set.of(
        "sign", "certify", "addSignatureField", "postvalidateSignedMutation"
    );

    private final ObjectMapper json;
    private final SigningService signing;
    private final SignatureFieldService fields;
    private final SafePdfMutation safeFiles;
    private final SignedMutationPostvalidationService postvalidation;

    FramedProtocolServer(Pkcs12PasswordPrompt prompt) {
        this(new SigningService(prompt), new SignatureFieldService(), new SafePdfMutation());
    }

    FramedProtocolServer(SigningService signing, SignatureFieldService fields, SafePdfMutation safeFiles) {
        this.signing = signing;
        this.fields = fields;
        this.safeFiles = safeFiles;
        this.postvalidation = new SignedMutationPostvalidationService(safeFiles);
        this.json = new ObjectMapper();
        json.getFactory().setStreamReadConstraints(StreamReadConstraints.builder()
            .maxNestingDepth(Protocol.MAX_JSON_DEPTH)
            .maxStringLength(Protocol.MAX_JSON_STRING_LENGTH)
            .maxNumberLength(Protocol.MAX_JSON_NUMBER_LENGTH)
            .maxNameLength(256)
            .build());
        json.enable(MapperFeature.SORT_PROPERTIES_ALPHABETICALLY);
        json.enable(SerializationFeature.ORDER_MAP_ENTRIES_BY_KEYS);
        json.enable(DeserializationFeature.FAIL_ON_TRAILING_TOKENS);
    }

    int runAfterMagic(InputStream input, OutputStream output) throws IOException {
        String requestId = null;
        String operation = null;
        Map<String, byte[]> frames = new LinkedHashMap<>();
        ObjectNode response;
        try {
            int headerLength = readLength(input, MAX_HEADER_BYTES, "INVALID_HEADER_LENGTH");
            byte[] headerBytes = readExact(input, headerLength);
            JsonNode header = parseHeader(headerBytes);
            requestId = text(header.get("requestId"));
            operation = text(header.get("operation"));
            validateEnvelope(header, requestId, operation);
            if (DISABLED_OPERATIONS.contains(operation)) throw new FramedException("CAPABILITY_DISABLED");
            frames.putAll(readFrames(input, header.get("frames")));
            if (input.read() != -1) throw new FramedException("TRAILING_REQUEST_BYTES");
            Object result = dispatch(operation, header.get("payload"), frames);
            response = success(requestId, operation, result);
        } catch (FramedException exception) {
            response = error(requestId, operation, exception.code());
        } catch (SigningService.SigningException exception) {
            response = error(requestId, operation, exception.code());
        } catch (SafePdfMutation.MutationException exception) {
            response = error(requestId, operation, exception.code());
        } catch (SignatureFieldService.FieldException exception) {
            response = error(requestId, operation, exception.code());
        } catch (SignedMutationPostcheck.PostcheckException exception) {
            response = error(requestId, operation, exception.code());
        } catch (Throwable exception) {
            if (exception instanceof VirtualMachineError fatal) throw fatal;
            if (exception instanceof ThreadDeath fatal) throw fatal;
            response = error(requestId, operation, "INTERNAL_ERROR");
        } finally {
            frames.values().forEach(bytes -> Arrays.fill(bytes, (byte) 0));
        }
        writeFrame(output, response);
        return 0;
    }

    private Object dispatch(String operation, JsonNode payload, Map<String, byte[]> frames)
        throws FramedException, SigningService.SigningException, SafePdfMutation.MutationException,
        SignatureFieldService.FieldException, SignedMutationPostcheck.PostcheckException {
        return switch (operation) {
            case "handshake" -> {
                if (!payload.isObject() || payload.size() != 0 || !frames.isEmpty()) {
                    throw new FramedException("INVALID_REQUEST");
                }
                yield handshake();
            }
            case "inspectPkcs12" -> {
                if (!payload.isObject() || payload.size() != 0 || !frames.keySet().equals(Set.of("pkcs12"))) {
                    throw new FramedException("INVALID_REQUEST");
                }
                yield signing.inspectPkcs12(frames.get("pkcs12"));
            }
            case "sign", "certify", "addSignatureField", "postvalidateSignedMutation" ->
                throw new FramedException("CAPABILITY_DISABLED");
            default -> throw new FramedException("UNSUPPORTED_OPERATION");
        };
    }

    private Map<String, Object> postvalidateSignedMutation(JsonNode payload, Map<String, byte[]> frames)
        throws FramedException, SafePdfMutation.MutationException,
        SignedMutationPostcheck.PostcheckException {
        if (!frames.isEmpty() || payload == null || !payload.isObject()) {
            throw new FramedException("INVALID_REQUEST");
        }
        Set<String> keys = new java.util.HashSet<>();
        payload.fieldNames().forEachRemaining(keys::add);
        Set<String> requiredKeys = new java.util.HashSet<>(Set.of(
            "inputPath", "outputPath", "expectedInputSha256", "expectedOutputSha256",
            "expectedCertificateSha256", "expectedFieldName", "expectedOperation", "expectedAppearance"
        ));
        if (payload.has("expectedCertificationPermission")) requiredKeys.add("expectedCertificationPermission");
        if (!keys.equals(requiredKeys)) {
            throw new FramedException("INVALID_REQUEST");
        }
        String inputPath = requiredText(payload, "inputPath");
        String outputPath = requiredText(payload, "outputPath");
        String expectedInputSha256 = requiredText(payload, "expectedInputSha256");
        String expectedOutputSha256 = requiredText(payload, "expectedOutputSha256");
        String expectedCertificateSha256 = requiredText(payload, "expectedCertificateSha256");
        String expectedFieldName = requiredText(payload, "expectedFieldName");
        String expectedOperation = requiredText(payload, "expectedOperation");
        String expectedAppearance = requiredText(payload, "expectedAppearance");
        if (!Set.of("approval", "certification").contains(expectedOperation)
            || !Set.of("visible", "invisible").contains(expectedAppearance)) {
            throw new FramedException("INVALID_REQUEST");
        }
        Integer expectedCertificationPermission = null;
        if ("certification".equals(expectedOperation)) {
            String permission = requiredText(payload, "expectedCertificationPermission");
            expectedCertificationPermission = switch (permission) {
                case "no-changes" -> 1;
                case "form-filling-and-signatures" -> 2;
                case "form-filling-signatures-and-annotations" -> 3;
                default -> throw new FramedException("INVALID_REQUEST");
            };
        } else if (payload.has("expectedCertificationPermission")) {
            throw new FramedException("INVALID_REQUEST");
        }
        if (!expectedInputSha256.matches("[a-f0-9]{64}")
            || !expectedOutputSha256.matches("[a-f0-9]{64}")
            || !expectedCertificateSha256.matches("[a-f0-9]{64}")) {
            throw new FramedException("INVALID_REQUEST");
        }
        return postvalidation.validate(
            inputPath,
            outputPath,
            expectedInputSha256,
            expectedOutputSha256,
            expectedFieldName,
            expectedCertificateSha256,
            expectedOperation,
            expectedAppearance,
            expectedCertificationPermission
        );
    }

    private Map<String, Object> addSignatureField(JsonNode payload, Map<String, byte[]> frames)
        throws FramedException, SafePdfMutation.MutationException, SignatureFieldService.FieldException {
        if (!frames.isEmpty() || payload == null || !payload.isObject()) throw new FramedException("INVALID_REQUEST");
        Set<String> keys = new java.util.HashSet<>();
        payload.fieldNames().forEachRemaining(keys::add);
        if (!keys.equals(Set.of("inputPath", "outputPath", "expectedInputSha256", "field"))) {
            throw new FramedException("INVALID_REQUEST");
        }
        String inputPath = requiredText(payload, "inputPath");
        String outputPath = requiredText(payload, "outputPath");
        String expectedHash = requiredText(payload, "expectedInputSha256");
        if (!expectedHash.matches("[a-f0-9]{64}")) throw new FramedException("INVALID_REQUEST");
        SignatureFieldSpec field;
        try {
            field = SignatureFieldSpec.parse(payload.get("field"));
        } catch (IllegalArgumentException exception) {
            throw new FramedException("INVALID_REQUEST");
        }
        SafePdfMutation.Input input = safeFiles.readInput(inputPath, expectedHash);
        SafePdfMutation.Output output = safeFiles.validateOutput(input, outputPath);
        byte[] mutated = null;
        byte[] reopened = null;
        boolean outputWritten = false;
        try {
            mutated = fields.addField(input.bytes(), field);
            fields.inspect(mutated, field.name());
            String outputSha256 = SafePdfMutation.sha256(mutated);
            safeFiles.write(output, input, mutated);
            outputWritten = true;
            reopened = safeFiles.readOutput(output, outputSha256);
            Map<String, Object> postcheck = fields.inspect(reopened, field.name());
            Map<String, Object> result = new LinkedHashMap<>();
            result.put("inputSha256", input.sha256());
            result.put("outputSha256", outputSha256);
            result.put("outputBytes", reopened.length);
            result.put("sourcePreserved", true);
            result.put("appendOnly", true);
            result.put("fieldName", field.name());
            result.put("postcheck", postcheck);
            result.put("engineVersion", Protocol.ENGINE_VERSION);
            return result;
        } catch (SafePdfMutation.MutationException | SignatureFieldService.FieldException exception) {
            if (outputWritten) safeFiles.discard(output);
            throw exception;
        } finally {
            Arrays.fill(input.bytes(), (byte) 0);
            if (mutated != null) Arrays.fill(mutated, (byte) 0);
            if (reopened != null) Arrays.fill(reopened, (byte) 0);
        }
    }

    private Map<String, Object> handshake() {
        Map<String, Object> result = new LinkedHashMap<>();
        result.put("versions", Map.of(
            "engine", Protocol.ENGINE_VERSION,
            "framedProtocol", VERSION,
            "java", Runtime.version().toString()
        ));
        result.put("operations", List.of(
            "handshake", "inspectPkcs12", "addSignatureField", "sign", "certify",
            "postvalidateSignedMutation"
        ));
        result.put("profiles", List.of("PAdES-B-B"));
        result.put("providers", List.of("pkcs12"));
        result.put("capabilities", Map.of(
            "certificateSign", false,
            "certify", false,
            "signatureFieldCreate", false,
            "signatureIncrementalWrite", false,
            "signedIncrementalEdit", false,
            "timestamp", false,
            "onlineValidation", false
        ));
        result.put("limits", Map.of(
            "headerBytes", MAX_HEADER_BYTES,
            "frameBytes", MAX_FRAME_BYTES,
            "totalFrameBytes", MAX_TOTAL_FRAME_BYTES,
            "signingInputBytes", SafePdfMutation.MAX_SIGNING_INPUT_BYTES
        ));
        return result;
    }

    private JsonNode parseHeader(byte[] encoded) throws FramedException {
        try {
            String value = StandardCharsets.UTF_8.newDecoder()
                .onMalformedInput(CodingErrorAction.REPORT)
                .onUnmappableCharacter(CodingErrorAction.REPORT)
                .decode(ByteBuffer.wrap(encoded))
                .toString();
            JsonNode parsed = json.readTree(value);
            if (parsed == null || !parsed.isObject() || containerEntryLimitExceeded(parsed)) {
                throw new FramedException("INVALID_REQUEST");
            }
            return parsed;
        } catch (JsonProcessingException exception) {
            throw new FramedException("MALFORMED_JSON");
        } catch (java.nio.charset.CharacterCodingException exception) {
            throw new FramedException("MALFORMED_UTF8");
        }
    }

    private void validateEnvelope(JsonNode header, String requestId, String operation) throws FramedException {
        if (header.size() != 5
            || header.get("protocolVersion") == null
            || !header.get("protocolVersion").isIntegralNumber()
            || !header.get("protocolVersion").canConvertToInt()
            || header.get("protocolVersion").intValue() != VERSION
            || requestId == null || !SAFE_REQUEST_ID.matcher(requestId).matches()
            || operation == null || operation.length() > 64
            || header.get("payload") == null || !header.get("payload").isObject()
            || header.get("frames") == null || !header.get("frames").isArray()) {
            throw new FramedException("INVALID_REQUEST");
        }
    }

    private Map<String, byte[]> readFrames(InputStream input, JsonNode descriptors)
        throws IOException, FramedException, SafePdfMutation.MutationException {
        if (descriptors.size() > MAX_FRAME_COUNT) throw new FramedException("TOO_MANY_FRAMES");
        Map<String, byte[]> frames = new LinkedHashMap<>();
        boolean complete = false;
        try {
            int total = 0;
            for (JsonNode descriptor : descriptors) {
                if (!descriptor.isObject() || descriptor.size() != 5) throw new FramedException("INVALID_FRAME_DESCRIPTOR");
                String id = text(descriptor.get("id"));
                String kind = text(descriptor.get("kind"));
                String sha256 = text(descriptor.get("sha256"));
                JsonNode sensitive = descriptor.get("sensitive");
                JsonNode byteLengthNode = descriptor.get("byteLength");
                if (id == null || !SAFE_FRAME_ID.matcher(id).matches() || frames.containsKey(id)
                    || !List.of("pkcs12", "appearance").contains(kind)
                    || !(sensitive != null && sensitive.isBoolean() && sensitive.booleanValue())
                    || sha256 == null || !sha256.matches("[a-f0-9]{64}")
                    || byteLengthNode == null || !byteLengthNode.isIntegralNumber()
                    || !byteLengthNode.canConvertToInt()) {
                    throw new FramedException("INVALID_FRAME_DESCRIPTOR");
                }
                int declared = byteLengthNode.intValue();
                if (declared < 1 || declared > MAX_FRAME_BYTES || total + declared > MAX_TOTAL_FRAME_BYTES) {
                    throw new FramedException("FRAME_TOO_LARGE");
                }
                int framedLength = readLength(input, MAX_FRAME_BYTES, "INVALID_FRAME_LENGTH");
                if (framedLength != declared) throw new FramedException("FRAME_LENGTH_MISMATCH");
                byte[] bytes = readExact(input, declared);
                boolean retained = false;
                try {
                    if (!SafePdfMutation.sha256(bytes).equals(sha256)) {
                        throw new FramedException("FRAME_HASH_MISMATCH");
                    }
                    if (("pkcs12".equals(kind) && !"pkcs12".equals(id))
                        || ("appearance".equals(kind) && !"appearance".equals(id))) {
                        throw new FramedException("INVALID_FRAME_DESCRIPTOR");
                    }
                    frames.put(id, bytes);
                    retained = true;
                    total += declared;
                } finally {
                    if (!retained) Arrays.fill(bytes, (byte) 0);
                }
            }
            complete = true;
            return frames;
        } finally {
            if (!complete) frames.values().forEach(bytes -> Arrays.fill(bytes, (byte) 0));
        }
    }

    private static int readLength(InputStream input, int maximum, String code) throws IOException, FramedException {
        byte[] encoded = readExact(input, 4);
        long value = Integer.toUnsignedLong(ByteBuffer.wrap(encoded).getInt());
        if (value < 1 || value > maximum) throw new FramedException(code);
        return (int) value;
    }

    private static byte[] readExact(InputStream input, int length) throws IOException, FramedException {
        byte[] bytes = input.readNBytes(length);
        if (bytes.length != length) {
            Arrays.fill(bytes, (byte) 0);
            throw new FramedException("TRUNCATED_REQUEST");
        }
        return bytes;
    }

    private void writeFrame(OutputStream output, ObjectNode response) throws IOException {
        byte[] encoded = json.writeValueAsBytes(response);
        if (encoded.length > MAX_RESPONSE_BYTES) {
            encoded = json.writeValueAsBytes(error(null, null, "RESPONSE_TOO_LARGE"));
        }
        output.write(MAGIC);
        output.write(ByteBuffer.allocate(4).putInt(encoded.length).array());
        output.write(encoded);
        output.flush();
    }

    private ObjectNode success(String requestId, String operation, Object result) {
        ObjectNode response = base(requestId, operation, "result", true);
        response.set("result", json.valueToTree(result));
        return response;
    }

    private ObjectNode error(String requestId, String operation, String code) {
        ObjectNode response = base(requestId, operation, "error", false);
        ObjectNode error = json.createObjectNode();
        error.put("code", code.matches("[A-Z][A-Z0-9_]{0,63}") ? code : "INTERNAL_ERROR");
        error.put("message", "The framed signature operation did not complete.");
        response.set("error", error);
        return response;
    }

    private ObjectNode base(String requestId, String operation, String event, boolean ok) {
        ObjectNode response = json.createObjectNode();
        response.put("protocolVersion", VERSION);
        if (requestId == null) response.putNull("requestId"); else response.put("requestId", requestId);
        if (operation == null) response.putNull("operation"); else response.put("operation", operation);
        response.put("engineVersion", Protocol.ENGINE_VERSION);
        response.put("event", event);
        response.put("ok", ok);
        return response;
    }

    private static String requiredText(JsonNode payload, String name) throws FramedException {
        String value = text(payload.get(name));
        if (value == null || value.isBlank() || value.length() > Protocol.MAX_PATH_LENGTH) {
            throw new FramedException("INVALID_REQUEST");
        }
        return value;
    }

    private static String text(JsonNode node) {
        return node != null && node.isTextual() ? node.textValue() : null;
    }

    private static boolean containerEntryLimitExceeded(JsonNode root) {
        int entries = 0;
        java.util.ArrayDeque<JsonNode> pending = new java.util.ArrayDeque<>();
        pending.add(root);
        while (!pending.isEmpty()) {
            JsonNode current = pending.removeFirst();
            if (current.isContainerNode()) {
                entries += current.size();
                if (entries > Protocol.MAX_CONTAINER_ENTRIES) return true;
                current.elements().forEachRemaining(pending::addLast);
            }
        }
        return false;
    }

    private static final class FramedException extends Exception {
        private final String code;
        FramedException(String code) { super(code); this.code = code; }
        String code() { return code; }
    }
}
