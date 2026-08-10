package com.butterpaper.signaturecore;
import com.fasterxml.jackson.core.JsonProcessingException;
import com.fasterxml.jackson.core.StreamReadConstraints;
import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.MapperFeature;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.SerializationFeature;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.time.Instant;
import java.time.format.DateTimeParseException;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.regex.Pattern;
final class ProtocolServer {
    private static final Pattern SAFE_REQUEST_ID = Pattern.compile("[A-Za-z0-9._:-]{1,128}");
    private static final Pattern CANONICAL_UTC_INSTANT = Pattern.compile(
        "[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(?:\\.[0-9]{1,9})?Z"
    );
    private final ObjectMapper json;
    private final InspectionService inspections;
    private final ValidationService validations;
    private final UnsignedCopyService unsignedCopies;
    private final PrintStream diagnostics;
    ProtocolServer(PrintStream diagnostics) {
        this(new InspectionService(), new ValidationService(), new UnsignedCopyService(), diagnostics);
    }
    ProtocolServer(InspectionService inspections, PrintStream diagnostics) {
        this(inspections, new ValidationService(), new UnsignedCopyService(), diagnostics);
    }
    ProtocolServer(InspectionService inspections, ValidationService validations, PrintStream diagnostics) {
        this(inspections, validations, new UnsignedCopyService(), diagnostics);
    }
    ProtocolServer(
        InspectionService inspections,
        ValidationService validations,
        UnsignedCopyService unsignedCopies,
        PrintStream diagnostics
    ) {
        this.inspections = inspections;
        this.validations = validations;
        this.unsignedCopies = unsignedCopies;
        this.diagnostics = diagnostics;
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
    int run(InputStream input, OutputStream output) throws IOException {
        BoundedLineReader lines = new BoundedLineReader(input, Protocol.MAX_LINE_BYTES);
        while (true) {
            BoundedLineReader.Line line = lines.read();
            if (line == null) return 0;
            ObjectNode response;
            if (line.tooLarge()) {
                response = error(null, null, "MESSAGE_TOO_LARGE", "Request exceeds the 1048576-byte protocol limit.");
            } else if (line.invalidUtf8()) {
                response = error(null, null, "MALFORMED_UTF8", "Request must be valid UTF-8.");
            } else if (line.value().isBlank()) {
                response = error(null, null, "MALFORMED_JSON", "Request must be one JSON object per line.");
            } else {
                response = handle(line.value());
            }
            output.write(json.writeValueAsBytes(response));
            output.write('\n');
            output.flush();
        }
    }
    ObjectNode handle(String encoded) {
        JsonNode parsed;
        try {
            parsed = json.readTree(encoded);
        } catch (JsonProcessingException exception) {
            return error(null, null, "MALFORMED_JSON", "Request must be one valid JSON object.");
        }
        if (!parsed.isObject() || containerEntryLimitExceeded(parsed)) {
            return error(null, null, "INVALID_REQUEST", "Request shape exceeds the protocol limits.");
        }
        String requestId = safeText(parsed.get("requestId"));
        String operation = safeText(parsed.get("operation"));
        if (requestId == null || !SAFE_REQUEST_ID.matcher(requestId).matches()) {
            return error(null, operation, "INVALID_REQUEST_ID", "requestId must contain 1-128 safe ASCII characters.");
        }
        JsonNode version = parsed.get("protocolVersion");
        if (version == null || !version.canConvertToInt() || version.intValue() != Protocol.VERSION) {
            return error(requestId, operation, "UNSUPPORTED_PROTOCOL_VERSION", "Supported protocolVersion is 1.");
        }
        JsonNode payload = parsed.get("payload");
        if (operation == null || payload == null || !payload.isObject()) {
            return error(requestId, operation, "INVALID_REQUEST", "operation and object payload are required.");
        }
        try {
            return switch (operation) {
                case "handshake", "version" -> success(requestId, operation, versionResult());
                case "inspect" -> inspect(requestId, operation, payload);
                case "validate" -> validate(requestId, operation, payload);
                case "createUnsignedCopy" -> createUnsignedCopy(requestId, operation, payload);
                case "inspectUnsignedStructure" -> inspectUnsignedStructure(requestId, operation, payload);
                case "cancel" -> cancel(requestId, operation, payload);
                default -> error(requestId, operation, "UNSUPPORTED_OPERATION", "Operation is not supported by this engine version.");
            };
        } catch (Throwable exception) {
            if (exception instanceof VirtualMachineError fatal) throw fatal;
            if (exception instanceof ThreadDeath fatal) throw fatal;
            diagnostics.println(SecretScrubber.scrub(
                "ERROR code=INTERNAL_ERROR requestId=" + requestId + " type=" + exception.getClass().getSimpleName()
            ));
            return error(requestId, operation, "INTERNAL_ERROR", "The sidecar could not complete the request.");
        }
    }
    private ObjectNode inspect(String requestId, String operation, JsonNode payload) {
        String inputPath = safeText(payload.get("inputPath"));
        try {
            return success(requestId, operation, inspections.inspect(inputPath));
        } catch (InspectionService.InspectionException exception) {
            return error(requestId, operation, exception.code(), "The input was rejected or could not be inspected.");
        }
    }
    private ObjectNode validate(String requestId, String operation, JsonNode payload) {
        String inputPath = safeText(payload.get("inputPath"));
        JsonNode onlineValidation = payload.get("onlineValidation");
        if (onlineValidation == null || !onlineValidation.isBoolean()) {
            return error(requestId, operation, "INVALID_REQUEST", "onlineValidation must be an explicit boolean.");
        }
        if (onlineValidation.booleanValue()) {
            return error(
                requestId,
                operation,
                "ONLINE_VALIDATION_UNSUPPORTED",
                "Online validation is not supported by this engine version."
            );
        }
        ExactTrustPolicy trustPolicy;
        try {
            JsonNode trustPolicyNode = payload.get("trustPolicy");
            trustPolicy = trustPolicyNode == null
                ? ExactTrustPolicy.empty()
                : ExactTrustPolicy.parse(trustPolicyNode);
        } catch (ExactTrustPolicy.PolicyException exception) {
            return error(
                requestId,
                operation,
                "INVALID_TRUST_POLICY",
                "The exact-certificate trust policy is malformed or inconsistent."
            );
        }
        ValidationService.ValidationClock validationClock;
        try {
            validationClock = validationClock(payload.get("validationClock"));
        } catch (IllegalArgumentException exception) {
            return error(
                requestId,
                operation,
                "INVALID_VALIDATION_CLOCK",
                "validationClock must be an omitted system default or a safe fixed historical reference."
            );
        }
        try {
            return success(requestId, operation, validations.validate(inputPath, trustPolicy, validationClock));
        } catch (ValidationService.ValidationException exception) {
            return error(requestId, operation, exception.code(), "The input was rejected or could not be validated.");
        }
    }
    private static ValidationService.ValidationClock validationClock(JsonNode value) {
        Instant observedSystemUtc = Instant.now();
        if (value == null) return ValidationService.ValidationClock.observedSystemUtc(observedSystemUtc);
        if (!value.isObject() || value.size() != 2
            || !value.has("mode") || !value.has("instant")
            || !"fixed-reference".equals(safeText(value.get("mode")))) {
            throw new IllegalArgumentException("invalid validation clock shape");
        }
        String encodedInstant = safeText(value.get("instant"));
        if (encodedInstant == null || !CANONICAL_UTC_INSTANT.matcher(encodedInstant).matches()) {
            throw new IllegalArgumentException("invalid validation clock instant");
        }
        try {
            Instant instant = Instant.parse(encodedInstant);
            if (!instant.toString().equals(encodedInstant)) {
                throw new IllegalArgumentException("validation clock instant is not canonical");
            }
            return ValidationService.ValidationClock.fixedReference(instant, observedSystemUtc);
        } catch (DateTimeParseException exception) {
            throw new IllegalArgumentException("invalid validation clock instant", exception);
        }
    }
    private ObjectNode cancel(String requestId, String operation, JsonNode payload) {
        String targetRequestId = safeText(payload.get("targetRequestId"));
        if (targetRequestId == null || !SAFE_REQUEST_ID.matcher(targetRequestId).matches()) {
            return error(requestId, operation, "INVALID_TARGET_REQUEST_ID", "A valid targetRequestId is required.");
        }
        Map<String, Object> result = new LinkedHashMap<>();
        result.put("targetRequestId", targetRequestId);
        result.put("status", "not-running");
        result.put("processCancellationRequired", true);
        return success(requestId, operation, result);
    }
    private ObjectNode createUnsignedCopy(String requestId, String operation, JsonNode payload) {
        String inputPath = safeText(payload.get("inputPath"));
        String outputPath = safeText(payload.get("outputPath"));
        try {
            return success(requestId, operation, unsignedCopies.create(inputPath, outputPath));
        } catch (UnsignedCopyService.CopyException exception) {
            return error(requestId, operation, exception.code(), "The unsigned copy could not be created safely.");
        }
    }
    private ObjectNode inspectUnsignedStructure(String requestId, String operation, JsonNode payload) {
        String inputPath = safeText(payload.get("inputPath"));
        try {
            return success(requestId, operation, unsignedCopies.inspect(inputPath));
        } catch (UnsignedCopyService.CopyException exception) {
            return error(requestId, operation, exception.code(), "The PDF structure could not be inspected safely.");
        }
    }
    private Map<String, Object> versionResult() {
        Map<String, Object> result = new LinkedHashMap<>();
        result.put("versions", EngineVersions.describe());
        result.put("operations", new String[]{
            "handshake", "version", "inspect", "validate", "createUnsignedCopy",
            "inspectUnsignedStructure", "cancel"
        });
        result.put("profiles", new String[]{});
        result.put("providers", new String[]{});
        Map<String, Boolean> capabilities = new LinkedHashMap<>();
        capabilities.put("inspect", true);
        capabilities.put("signatureRead", true);
        capabilities.put("signatureValidation", true);
        capabilities.put("createUnsignedCopy", true);
        capabilities.put("certificateSign", false);
        capabilities.put("certify", false);
        capabilities.put("onlineValidation", false);
        capabilities.put("pkcs11", false);
        capabilities.put("ltv", false);
        capabilities.put("signedIncrementalEdit", false);
        result.put("capabilities", capabilities);
        result.put("limits", Map.of(
            "inputBytes", Protocol.MAX_INPUT_BYTES,
            "jsonDepth", Protocol.MAX_JSON_DEPTH,
            "lineBytes", Protocol.MAX_LINE_BYTES,
            "requestIdLength", Protocol.MAX_REQUEST_ID_LENGTH
        ));
        return result;
    }
    private ObjectNode success(String requestId, String operation, Object result) {
        ObjectNode response = base(requestId, operation, "result", true);
        response.set("result", json.valueToTree(result));
        return response;
    }
    private ObjectNode error(String requestId, String operation, String code, String message) {
        ObjectNode response = base(requestId, operation, "error", false);
        ObjectNode error = response.putObject("error");
        error.put("code", code);
        error.put("message", message);
        return response;
    }
    private ObjectNode base(String requestId, String operation, String event, boolean ok) {
        ObjectNode response = json.createObjectNode();
        response.put("engineVersion", Protocol.ENGINE_VERSION);
        response.put("event", event);
        response.put("ok", ok);
        if (operation == null) response.putNull("operation"); else response.put("operation", operation);
        response.put("protocolVersion", Protocol.VERSION);
        if (requestId == null) response.putNull("requestId"); else response.put("requestId", requestId);
        return response;
    }
    private static String safeText(JsonNode node) {
        return node != null && node.isTextual() ? node.textValue() : null;
    }
    private static boolean containerEntryLimitExceeded(JsonNode root) {
        int entries = 0;
        java.util.ArrayDeque<JsonNode> pending = new java.util.ArrayDeque<>();
        pending.add(root);
        while (!pending.isEmpty()) {
            JsonNode current = pending.removeFirst();
            if (current.isObject()) {
                Iterator<Map.Entry<String, JsonNode>> fields = current.fields();
                while (fields.hasNext()) {
                    pending.add(fields.next().getValue());
                    if (++entries > Protocol.MAX_CONTAINER_ENTRIES) return true;
                }
            } else if (current instanceof ArrayNode array) {
                for (JsonNode item : array) {
                    pending.add(item);
                    if (++entries > Protocol.MAX_CONTAINER_ENTRIES) return true;
                }
            }
        }
        return false;
    }
}
