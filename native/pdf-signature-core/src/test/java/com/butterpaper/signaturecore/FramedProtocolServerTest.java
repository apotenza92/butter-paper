package com.butterpaper.signaturecore;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.junit.jupiter.api.Test;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.HexFormat;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class FramedProtocolServerTest {
    private static final ObjectMapper JSON = new ObjectMapper();

    @Test
    void advertisesExperimentalOperationsWithEveryMutationCapabilityDisabled() throws Exception {
        ObjectNode header = header("request-1", "handshake", JSON.createObjectNode(), JSON.createArrayNode());
        JsonNode response = run(header, new byte[0], SigningTestFixtures.fixedPrompt());
        assertEquals("result", response.get("event").textValue());
        JsonNode capabilities = response.at("/result/capabilities");
        capabilities.fields().forEachRemaining(entry -> assertFalse(entry.getValue().booleanValue(), entry.getKey()));
        assertEquals("PAdES-B-B", response.at("/result/profiles/0").textValue());
        assertEquals("pkcs12", response.at("/result/providers/0").textValue());
    }

    @Test
    void inspectsPkcs12WithoutReturningPasswordOrContainerBytes() throws Exception {
        SigningTestFixtures.IdentityFixture identity = SigningTestFixtures.identity("RSA");
        byte[] pkcs12 = identity.pkcs12();
        ArrayNode descriptors = JSON.createArrayNode();
        descriptors.add(frameDescriptor("pkcs12", "pkcs12", pkcs12));
        ObjectNode header = header("request-2", "inspectPkcs12", JSON.createObjectNode(), descriptors);
        JsonNode response = run(header, frame(pkcs12), SigningTestFixtures.fixedPrompt());
        byte[] encodedResponse = JSON.writeValueAsBytes(response);
        assertEquals(identity.fingerprint(), response.at("/result/identities/0/certificateSha256").textValue());
        assertEquals(false, response.at("/result/passwordRemembered").booleanValue());
        assertFalse(new String(encodedResponse, StandardCharsets.UTF_8).contains(new String(SigningTestFixtures.PASSWORD)));
        assertFalse(indexOf(encodedResponse, pkcs12) >= 0);
    }

    @Test
    void rejectsDisabledMutationOperationsBeforeDispatchOrFrameReads() throws Exception {
        for (String operation : java.util.List.of(
            "sign", "certify", "addSignatureField", "postvalidateSignedMutation"
        )) {
            JsonNode response = run(
                header("disabled-" + operation, operation, JSON.createObjectNode(), JSON.createArrayNode()),
                new byte[0],
                () -> { throw new AssertionError(operation + " must not prompt for an identity password"); }
            );
            assertEquals("CAPABILITY_DISABLED", response.at("/error/code").textValue(), operation);
        }
    }

    @Test
    void rejectsTruncatedAndHashMismatchedSecretFramesWithGenericErrors() throws Exception {
        byte[] pkcs12 = SigningTestFixtures.identity("RSA").pkcs12();
        ArrayNode descriptors = JSON.createArrayNode();
        descriptors.add(frameDescriptor("pkcs12", "pkcs12", pkcs12));
        ObjectNode header = header("request-3", "inspectPkcs12", JSON.createObjectNode(), descriptors);

        byte[] truncated = Arrays.copyOf(frame(pkcs12), frame(pkcs12).length - 1);
        JsonNode truncatedResponse = run(header, truncated, SigningTestFixtures.fixedPrompt());
        assertEquals("TRUNCATED_REQUEST", truncatedResponse.at("/error/code").textValue());
        assertEquals("The framed signature operation did not complete.", truncatedResponse.at("/error/message").textValue());

        ((ObjectNode) descriptors.get(0)).put("sha256", "0".repeat(64));
        JsonNode mismatchResponse = run(header, frame(pkcs12), SigningTestFixtures.fixedPrompt());
        assertEquals("FRAME_HASH_MISMATCH", mismatchResponse.at("/error/code").textValue());
    }

    @Test
    void mainDetectsFramedMagicWithoutChangingLegacyNdjson() throws Exception {
        byte[] legacy = "{\"protocolVersion\":1,\"requestId\":\"legacy\",\"operation\":\"handshake\",\"payload\":{}}\n"
            .getBytes(StandardCharsets.UTF_8);
        assertFalse(startsWith(legacy, FramedProtocolServer.MAGIC));
        byte[] framed = concat(FramedProtocolServer.MAGIC, requestBytes(
            header("framed", "handshake", JSON.createObjectNode(), JSON.createArrayNode()),
            new byte[0]
        ));
        assertTrue(startsWith(framed, FramedProtocolServer.MAGIC));
    }

    @Test
    void rejectsNonIntegralEnvelopeAndFrameNumbersOrDisabledMutationPayloads() throws Exception {
        ObjectNode fractionalVersion = header(
            "fractional-version", "handshake", JSON.createObjectNode(), JSON.createArrayNode()
        );
        fractionalVersion.put("protocolVersion", 2.5);
        assertEquals("INVALID_REQUEST", run(fractionalVersion, new byte[0], SigningTestFixtures.fixedPrompt())
            .at("/error/code").textValue());

        byte[] one = new byte[]{1};
        ArrayNode descriptors = JSON.createArrayNode();
        ObjectNode descriptor = frameDescriptor("pkcs12", "pkcs12", one);
        descriptor.put("byteLength", 1.5);
        descriptors.add(descriptor);
        assertEquals("INVALID_FRAME_DESCRIPTOR", run(
            header("fractional-frame", "inspectPkcs12", JSON.createObjectNode(), descriptors),
            new byte[0],
            SigningTestFixtures.fixedPrompt()
        ).at("/error/code").textValue());

        ObjectNode payload = JSON.createObjectNode();
        payload.put("inputPath", "/private/tmp/source.pdf");
        payload.put("outputPath", "/private/tmp/output.pdf");
        payload.put("expectedInputSha256", "0".repeat(64));
        ObjectNode field = JSON.createObjectNode();
        field.put("kind", "new");
        field.put("name", "Fractional");
        ObjectNode widget = field.putObject("widget");
        widget.put("pageIndex", 0.5);
        widget.put("x", 72.5);
        widget.put("y", 72);
        widget.put("width", 180);
        widget.put("height", 60);
        widget.put("pageRotation", 0);
        widget.put("coordinateSpace", "unrotated-pdf-default-user-space");
        payload.set("field", field);
        assertEquals("CAPABILITY_DISABLED", run(
            header("fractional-field", "addSignatureField", payload, JSON.createArrayNode()),
            new byte[0],
            SigningTestFixtures.fixedPrompt()
        ).at("/error/code").textValue());
    }

    @Test
    void rejectsDisabledMutationBeforePayloadSchemaValidation() throws Exception {
        for (String operation : java.util.List.of(
            "addSignatureField", "sign", "postvalidateSignedMutation"
        )) {
            ObjectNode payload = JSON.createObjectNode();
            payload.put("privateKey", "must-never-be-an-accepted-header-field");
            assertEquals("CAPABILITY_DISABLED", run(
                header("disabled-schema-" + operation, operation, payload, JSON.createArrayNode()),
                new byte[0],
                SigningTestFixtures.fixedPrompt()
            ).at("/error/code").textValue(), operation);
        }
    }

    @Test
    void rejectsDisabledSigningBeforeReadingSecretFrames() throws Exception {
        byte[] secret = new byte[]{1};
        byte[] appearance = new byte[]{2};
        ArrayNode descriptors = JSON.createArrayNode();
        descriptors.add(frameDescriptor("pkcs12", "pkcs12", secret));
        descriptors.add(frameDescriptor("appearance", "appearance", appearance));
        ObjectNode header = header("zero-on-failure", "sign", JSON.createObjectNode(), descriptors);
        byte[] encodedHeader = JSON.writeValueAsBytes(header);
        SequencedInputStream input = new SequencedInputStream(
            ByteBuffer.allocate(4).putInt(encodedHeader.length).array(),
            encodedHeader
        );
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        new FramedProtocolServer(SigningTestFixtures.fixedPrompt()).runAfterMagic(input, output);
        JsonNode response = JSON.readTree(output.toByteArray(), 8, output.size() - 8);
        assertEquals("CAPABILITY_DISABLED", response.at("/error/code").textValue());
        assertEquals(2, input.valuesRead());
    }

    private static JsonNode run(ObjectNode header, byte[] frames, Pkcs12PasswordPrompt prompt) throws Exception {
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        new FramedProtocolServer(prompt).runAfterMagic(
            new ByteArrayInputStream(requestBytes(header, frames)),
            output
        );
        byte[] response = output.toByteArray();
        assertArrayEquals(FramedProtocolServer.MAGIC, Arrays.copyOf(response, 4));
        int length = ByteBuffer.wrap(response, 4, 4).getInt();
        assertEquals(response.length - 8, length);
        return JSON.readTree(response, 8, length);
    }

    private static ObjectNode header(String requestId, String operation, ObjectNode payload, ArrayNode frames) {
        ObjectNode header = JSON.createObjectNode();
        header.put("protocolVersion", 2);
        header.put("requestId", requestId);
        header.put("operation", operation);
        header.set("payload", payload);
        header.set("frames", frames);
        return header;
    }

    private static ObjectNode frameDescriptor(String id, String kind, byte[] bytes) throws Exception {
        ObjectNode descriptor = JSON.createObjectNode();
        descriptor.put("id", id);
        descriptor.put("kind", kind);
        descriptor.put("byteLength", bytes.length);
        descriptor.put("sha256", HexFormat.of().formatHex(
            java.security.MessageDigest.getInstance("SHA-256").digest(bytes)
        ));
        descriptor.put("sensitive", true);
        return descriptor;
    }

    private static byte[] requestBytes(ObjectNode header, byte[] frames) throws Exception {
        byte[] encoded = JSON.writeValueAsBytes(header);
        return concat(ByteBuffer.allocate(4).putInt(encoded.length).array(), encoded, frames);
    }

    private static byte[] frame(byte[] bytes) {
        return concat(ByteBuffer.allocate(4).putInt(bytes.length).array(), bytes);
    }

    private static byte[] concat(byte[]... arrays) {
        int length = Arrays.stream(arrays).mapToInt(array -> array.length).sum();
        byte[] result = new byte[length];
        int offset = 0;
        for (byte[] array : arrays) {
            System.arraycopy(array, 0, result, offset, array.length);
            offset += array.length;
        }
        return result;
    }

    private static boolean startsWith(byte[] value, byte[] prefix) {
        return value.length >= prefix.length && Arrays.equals(Arrays.copyOf(value, prefix.length), prefix);
    }

    private static final class SequencedInputStream extends InputStream {
        private final byte[][] values;
        private int index;

        private SequencedInputStream(byte[]... values) {
            this.values = values;
        }

        private int valuesRead() {
            return index;
        }

        @Override
        public byte[] readNBytes(int length) throws IOException {
            if (index >= values.length) return new byte[0];
            byte[] value = values[index++];
            if (value.length > length) throw new IOException("test sequence exceeded requested length");
            return value;
        }

        @Override
        public int read() {
            return -1;
        }
    }

    private static int indexOf(byte[] haystack, byte[] needle) {
        return indexOf(haystack, needle, 0);
    }

    private static int indexOf(byte[] haystack, byte[] needle, int fromIndex) {
        outer: for (int start = fromIndex; start <= haystack.length - needle.length; start++) {
            for (int index = 0; index < needle.length; index++) {
                if (haystack[start + index] != needle[index]) continue outer;
            }
            return start;
        }
        return -1;
    }
}
