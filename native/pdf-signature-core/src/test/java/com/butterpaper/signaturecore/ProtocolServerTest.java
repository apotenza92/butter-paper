package com.butterpaper.signaturecore;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;
import java.time.Instant;

import static org.junit.jupiter.api.Assertions.*;

class ProtocolServerTest {
    private final ObjectMapper json = new ObjectMapper();
    @TempDir Path temporaryDirectory;

    @Test
    void handshakeReportsPinnedEnginesAndOnlyImplementedCapabilities() throws Exception {
        JsonNode response = request("""
            {"protocolVersion":1,"requestId":"hello-1","operation":"handshake","payload":{}}
            """);
        assertTrue(response.get("ok").booleanValue());
        assertEquals("result", response.get("event").textValue());
        assertEquals("0.1.0", response.at("/result/versions/engine").textValue());
        assertEquals("6.4", response.at("/result/versions/dss").textValue());
        assertEquals(21, response.at("/result/versions/javaFeature").intValue());
        assertTrue(response.at("/result/capabilities/inspect").booleanValue());
        assertTrue(response.at("/result/capabilities/signatureRead").booleanValue());
        assertTrue(response.at("/result/capabilities/signatureValidation").booleanValue());
        assertTrue(response.at("/result/capabilities/createUnsignedCopy").booleanValue());
        assertTrue(response.at("/result/operations").toString().contains("createUnsignedCopy"));
        assertFalse(response.at("/result/capabilities/certificateSign").booleanValue());
    }

    @Test
    void validateRequiresExplicitOfflineModeAndNeverDowngradesOnlineRequests() throws Exception {
        JsonNode missing = request("""
            {"protocolVersion":1,"requestId":"validate-missing","operation":"validate","payload":{}}
            """);
        assertEquals("INVALID_REQUEST", missing.at("/error/code").textValue());

        JsonNode online = request("""
            {"protocolVersion":1,"requestId":"validate-online","operation":"validate","payload":{"inputPath":"/not/opened.pdf","onlineValidation":true}}
            """);
        assertEquals("ONLINE_VALIDATION_UNSUPPORTED", online.at("/error/code").textValue());
    }

    @Test
    void validatesUnsignedPdfWithoutReturningItsPathOrAnAggregateValidity() throws Exception {
        Path input = temporaryDirectory.resolve("customer-unsigned.pdf").toAbsolutePath();
        try (org.apache.pdfbox.pdmodel.PDDocument document = new org.apache.pdfbox.pdmodel.PDDocument()) {
            document.addPage(new org.apache.pdfbox.pdmodel.PDPage());
            document.save(input.toFile());
        }
        String encoded = json.writeValueAsString(java.util.Map.of(
            "protocolVersion", 1,
            "requestId", "validate-offline",
            "operation", "validate",
            "payload", java.util.Map.of("inputPath", input.toString(), "onlineValidation", false)
        ));
        Instant before = Instant.now();
        String response = requestRaw(encoded);
        Instant after = Instant.now();
        assertFalse(response.contains(input.toString()));
        JsonNode parsed = json.readTree(response);
        assertTrue(parsed.get("ok").booleanValue());
        assertEquals("offline", parsed.at("/result/validationMode").textValue());
        Instant validationTime = Instant.parse(parsed.at("/result/validationTime").textValue());
        assertFalse(validationTime.isBefore(before));
        assertFalse(validationTime.isAfter(after));
        assertEquals("observed-system-utc", parsed.at("/result/validationTimeProvenance").textValue());
        assertEquals("unsigned", parsed.at("/result/inventory/presence").textValue());
        assertEquals(0, parsed.at("/result/signatures").size());
        assertEquals(false, parsed.at("/result/trust/onlineSourcesUsed").booleanValue());
        assertEquals("butter-paper-local-explicit-certificates", parsed.at("/result/trust/policyId").textValue());
        assertEquals(1, parsed.at("/result/trust/policyVersion").intValue());
        assertEquals("65621a8373d3e6869d50a8572da7d20ae5c4d7c91a915eeda34493187f071f0e",
            parsed.at("/result/trust/configurationSha256").textValue());
        assertTrue(parsed.at("/result/trust/configuredExactCertificateFingerprints").isEmpty());
        assertTrue(parsed.findValues("valid").isEmpty(), "report must not expose an aggregate valid boolean");
    }

    @Test
    void acceptsOnlyCanonicalHistoricalFixedReferenceClock() throws Exception {
        Path input = temporaryDirectory.resolve("fixed-reference.pdf").toAbsolutePath();
        try (org.apache.pdfbox.pdmodel.PDDocument document = new org.apache.pdfbox.pdmodel.PDDocument()) {
            document.addPage(new org.apache.pdfbox.pdmodel.PDPage());
            document.save(input.toFile());
        }
        String fixed = "2026-08-05T00:00:00Z";
        JsonNode response = request(json.writeValueAsString(java.util.Map.of(
            "protocolVersion", 1,
            "requestId", "validate-fixed-reference",
            "operation", "validate",
            "payload", java.util.Map.of(
                "inputPath", input.toString(),
                "onlineValidation", false,
                "validationClock", java.util.Map.of("mode", "fixed-reference", "instant", fixed)
            )
        )));

        assertTrue(response.get("ok").booleanValue());
        assertEquals(fixed, response.at("/result/validationTime").textValue());
        assertEquals(
            "caller-supplied-fixed-reference",
            response.at("/result/validationTimeProvenance").textValue()
        );
        assertEquals("offline", response.at("/result/validationMode").textValue());
        assertFalse(response.at("/result/trust/onlineSourcesUsed").booleanValue());
    }

    @Test
    void rejectsMalformedAmbiguousAndFutureFixedReferenceClocks() throws Exception {
        String future = Instant.now().plus(Duration.ofDays(1)).toString();
        for (Object validationClock : java.util.List.of(
            "2026-08-05T00:00:00Z",
            java.util.Map.of("mode", "system-utc", "instant", "2026-08-05T00:00:00Z"),
            java.util.Map.of("mode", "fixed-reference", "instant", "2026-08-05T10:00:00+10:00"),
            java.util.Map.of("mode", "fixed-reference", "instant", "2026-08-05T00:00:00.000Z"),
            java.util.Map.of("mode", "fixed-reference", "instant", "1899-12-31T23:59:59Z"),
            java.util.Map.of("mode", "fixed-reference", "instant", future),
            java.util.Map.of("mode", "fixed-reference", "instant", "2026-08-05T00:00:00Z", "extra", true)
        )) {
            JsonNode response = request(json.writeValueAsString(java.util.Map.of(
                "protocolVersion", 1,
                "requestId", "validate-invalid-clock",
                "operation", "validate",
                "payload", java.util.Map.of(
                    "inputPath", "/not/opened.pdf",
                    "onlineValidation", false,
                    "validationClock", validationClock
                )
            )));
            assertFalse(response.get("ok").booleanValue());
            assertEquals("INVALID_VALIDATION_CLOCK", response.at("/error/code").textValue());
            assertFalse(response.toString().contains(validationClock.toString()));
        }
    }

    @Test
    void rejectsMalformedTrustPolicyWithoutFallingBackToEmptyTrust() throws Exception {
        Path input = temporaryDirectory.resolve("unsigned-trust.pdf").toAbsolutePath();
        try (org.apache.pdfbox.pdmodel.PDDocument document = new org.apache.pdfbox.pdmodel.PDDocument()) {
            document.addPage(new org.apache.pdfbox.pdmodel.PDPage());
            document.save(input.toFile());
        }
        String encoded = json.writeValueAsString(java.util.Map.of(
            "protocolVersion", 1,
            "requestId", "validate-invalid-trust",
            "operation", "validate",
            "payload", java.util.Map.of(
                "inputPath", input.toString(),
                "onlineValidation", false,
                "trustPolicy", java.util.Map.of(
                    "policyId", "butter-paper-local-explicit-certificates",
                    "policyVersion", 1,
                    "configurationSha256", "0".repeat(64),
                    "exactCertificateAnchors", java.util.List.of()
                )
            )
        ));

        JsonNode response = json.readTree(requestRaw(encoded));

        assertFalse(response.get("ok").booleanValue());
        assertEquals("INVALID_TRUST_POLICY", response.at("/error/code").textValue());
        assertFalse(response.toString().contains(input.toString()));
    }

    @Test
    void createsUnsignedCopyThroughPathsOnStdinWithoutReturningThosePaths() throws Exception {
        Path input = ValidationServiceTest.createSignedPdf(
            temporaryDirectory.resolve("protocol-signed.pdf").toAbsolutePath()
        );
        byte[] before = Files.readAllBytes(input);
        Path output = temporaryDirectory.resolve("protocol-unsigned-output.pdf").toAbsolutePath();
        try {
            Files.setPosixFilePermissions(temporaryDirectory, java.util.Set.of(
                java.nio.file.attribute.PosixFilePermission.OWNER_READ,
                java.nio.file.attribute.PosixFilePermission.OWNER_WRITE,
                java.nio.file.attribute.PosixFilePermission.OWNER_EXECUTE
            ));
        } catch (UnsupportedOperationException ignored) {}
        Files.createFile(output);
        try {
            Files.setPosixFilePermissions(output, java.util.Set.of(
                java.nio.file.attribute.PosixFilePermission.OWNER_READ,
                java.nio.file.attribute.PosixFilePermission.OWNER_WRITE
            ));
        } catch (UnsupportedOperationException ignored) {}
        String encoded = json.writeValueAsString(java.util.Map.of(
            "protocolVersion", 1,
            "requestId", "unsigned-copy",
            "operation", "createUnsignedCopy",
            "payload", java.util.Map.of("inputPath", input.toString(), "outputPath", output.toString())
        ));

        String raw = requestRaw(encoded);
        JsonNode response = json.readTree(raw);

        assertTrue(response.get("ok").booleanValue());
        assertTrue(response.at("/result/validatedUnsigned").booleanValue());
        assertTrue(response.at("/result/sourcePreserved").booleanValue());
        assertEquals("butter-paper-structurally-unsigned-copy",
            response.at("/result/removalPolicyId").textValue());
        assertEquals(1, response.at("/result/removalPolicyVersion").intValue());
        for (JsonNode value : response.at("/result/structuralPostcheck")) {
            assertEquals(0, value.intValue());
        }
        assertFalse(raw.contains(input.toString()));
        assertFalse(raw.contains(output.toString()));
        assertArrayEquals(before, Files.readAllBytes(input));
        try (org.apache.pdfbox.pdmodel.PDDocument copy = org.apache.pdfbox.Loader.loadPDF(output.toFile())) {
            assertTrue(copy.getSignatureDictionaries().isEmpty());
        }

        JsonNode inspection = request(json.writeValueAsString(java.util.Map.of(
            "protocolVersion", 1,
            "requestId", "unsigned-structure",
            "operation", "inspectUnsignedStructure",
            "payload", java.util.Map.of("inputPath", output.toString())
        )));
        assertTrue(inspection.get("ok").booleanValue());
        assertTrue(inspection.at("/result/structurallyReadable").booleanValue());
        assertEquals(response.at("/result/outputSha256"), inspection.at("/result/inputSha256"));
        assertEquals(8, inspection.at("/result").size());
        for (String field : java.util.List.of(
            "byteRangeMarkerCount", "signatureDictionaryCount", "signedSignatureFieldCount",
            "docMdpReferenceCount", "fieldMdpReferenceCount", "dssOrVriEntryCount"
        )) {
            assertEquals(0, inspection.at("/result/" + field).intValue());
        }
    }

    @Test
    void inspectNeverReturnsTheInputPath() throws Exception {
        Path input = temporaryDirectory.resolve("commercial-secret-name.pdf");
        Files.writeString(input, "%PDF-1.4\n%%EOF\n");
        String encoded = json.writeValueAsString(java.util.Map.of(
            "protocolVersion", 1,
            "requestId", "inspect-1",
            "operation", "inspect",
            "payload", java.util.Map.of("inputPath", input.toString())
        ));
        String response = requestRaw(encoded);
        assertFalse(response.contains(input.toString()));
        JsonNode parsed = json.readTree(response);
        assertEquals(true, parsed.at("/result/structuralOnly").booleanValue());
        assertEquals(false, parsed.at("/result/validationPerformed").booleanValue());
    }

    @Test
    void rejectsMalformedOversizedDeepAndVersionSkewedMessagesThenContinues() throws Exception {
        String oversized = "x".repeat(Protocol.MAX_LINE_BYTES + 1);
        String deep = "{\"protocolVersion\":1,\"requestId\":\"deep\",\"operation\":\"inspect\",\"payload\":"
            + "[".repeat(Protocol.MAX_JSON_DEPTH + 1) + "0" + "]".repeat(Protocol.MAX_JSON_DEPTH + 1) + "}";
        String input = "not-json\n" + oversized + "\n" + deep + "\n"
            + "{\"protocolVersion\":2,\"requestId\":\"skew\",\"operation\":\"handshake\",\"payload\":{}}\n"
            + "{\"protocolVersion\":1,\"requestId\":\"last\",\"operation\":\"handshake\",\"payload\":{}}\n";
        String[] output = run(input).split("\\n");
        assertEquals(5, output.length);
        assertEquals("MALFORMED_JSON", json.readTree(output[0]).at("/error/code").textValue());
        assertEquals("MESSAGE_TOO_LARGE", json.readTree(output[1]).at("/error/code").textValue());
        assertEquals("MALFORMED_JSON", json.readTree(output[2]).at("/error/code").textValue());
        assertEquals("UNSUPPORTED_PROTOCOL_VERSION", json.readTree(output[3]).at("/error/code").textValue());
        assertTrue(json.readTree(output[4]).get("ok").booleanValue());
    }

    @Test
    void returnsDeterministicUnsupportedAndCancellationResults() throws Exception {
        JsonNode unsupported = request("""
            {"protocolVersion":1,"requestId":"unknown","operation":"sign","payload":{}}
            """);
        assertEquals("UNSUPPORTED_OPERATION", unsupported.at("/error/code").textValue());

        JsonNode cancelled = request("""
            {"protocolVersion":1,"requestId":"cancel-1","operation":"cancel","payload":{"targetRequestId":"inspect-9"}}
            """);
        assertEquals("not-running", cancelled.at("/result/status").textValue());
        assertTrue(cancelled.at("/result/processCancellationRequired").booleanValue());
    }

    @Test
    void deterministicResponseBytesContainNoDiagnostics() throws Exception {
        String first = requestRaw("{\"protocolVersion\":1,\"requestId\":\"v\",\"operation\":\"version\",\"payload\":{}}");
        String second = requestRaw("{\"protocolVersion\":1,\"requestId\":\"v\",\"operation\":\"version\",\"payload\":{}}");
        assertEquals(first, second);
        assertFalse(first.contains("Exception"));
    }

    @Test
    void containsAnOperationCrashAndContinuesWithoutLeakingItsCause() throws Exception {
        InspectionService crashingInspection = new InspectionService() {
            @Override
            java.util.Map<String, Object> inspect(String inputPath) {
                throw new IllegalStateException("password=hunter2 /tmp/customer-secret.pdf");
            }
        };
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        ByteArrayOutputStream diagnostics = new ByteArrayOutputStream();
        ProtocolServer server = new ProtocolServer(
            crashingInspection,
            new PrintStream(diagnostics, true, StandardCharsets.UTF_8)
        );
        String input = "{\"protocolVersion\":1,\"requestId\":\"crash\",\"operation\":\"inspect\",\"payload\":{\"inputPath\":\"/tmp/customer-secret.pdf\"}}\n"
            + "{\"protocolVersion\":1,\"requestId\":\"after\",\"operation\":\"handshake\",\"payload\":{}}\n";
        assertEquals(0, server.run(new ByteArrayInputStream(input.getBytes(StandardCharsets.UTF_8)), output));
        String[] responses = output.toString(StandardCharsets.UTF_8).split("\\n");
        assertEquals("INTERNAL_ERROR", json.readTree(responses[0]).at("/error/code").textValue());
        assertTrue(json.readTree(responses[1]).get("ok").booleanValue());
        assertFalse(output.toString(StandardCharsets.UTF_8).contains("hunter2"));
        assertFalse(diagnostics.toString(StandardCharsets.UTF_8).contains("hunter2"));
        assertFalse(diagnostics.toString(StandardCharsets.UTF_8).contains("customer-secret"));
    }

    @Test
    void containsServiceInitializationErrorsAndContinuesWithoutLeakingTheirCause() throws Exception {
        InspectionService crashingInspection = new InspectionService() {
            @Override
            java.util.Map<String, Object> inspect(String inputPath) {
                throw new ExceptionInInitializerError("password=hunter2 /tmp/customer-secret.pdf");
            }
        };
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        ByteArrayOutputStream diagnostics = new ByteArrayOutputStream();
        ProtocolServer server = new ProtocolServer(
            crashingInspection,
            new PrintStream(diagnostics, true, StandardCharsets.UTF_8)
        );
        String input = "{\"protocolVersion\":1,\"requestId\":\"init-crash\",\"operation\":\"inspect\",\"payload\":{\"inputPath\":\"/tmp/customer-secret.pdf\"}}\n"
            + "{\"protocolVersion\":1,\"requestId\":\"after\",\"operation\":\"handshake\",\"payload\":{}}\n";

        assertEquals(0, server.run(new ByteArrayInputStream(input.getBytes(StandardCharsets.UTF_8)), output));

        String[] responses = output.toString(StandardCharsets.UTF_8).split("\\n");
        assertEquals("INTERNAL_ERROR", json.readTree(responses[0]).at("/error/code").textValue());
        assertTrue(json.readTree(responses[1]).get("ok").booleanValue());
        assertFalse(output.toString(StandardCharsets.UTF_8).contains("hunter2"));
        assertFalse(diagnostics.toString(StandardCharsets.UTF_8).contains("hunter2"));
        assertFalse(diagnostics.toString(StandardCharsets.UTF_8).contains("customer-secret"));
    }

    private JsonNode request(String encoded) throws Exception {
        return json.readTree(requestRaw(encoded.strip()));
    }

    private String requestRaw(String encoded) throws Exception {
        return run(encoded + "\n").strip();
    }

    private String run(String encoded) throws Exception {
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        ByteArrayOutputStream diagnostics = new ByteArrayOutputStream();
        int exit = new ProtocolServer(new PrintStream(diagnostics, true, StandardCharsets.UTF_8)).run(
            new ByteArrayInputStream(encoded.getBytes(StandardCharsets.UTF_8)), output
        );
        assertEquals(0, exit);
        assertEquals("", diagnostics.toString(StandardCharsets.UTF_8));
        return output.toString(StandardCharsets.UTF_8);
    }
}
