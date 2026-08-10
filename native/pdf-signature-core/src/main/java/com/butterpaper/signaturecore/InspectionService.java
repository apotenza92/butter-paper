package com.butterpaper.signaturecore;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.channels.SeekableByteChannel;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.OpenOption;
import java.nio.file.Path;
import java.nio.file.attribute.BasicFileAttributes;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HexFormat;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Set;
class InspectionService {
    static final class InspectionException extends Exception {
        private final String code;
        InspectionException(String code) {
            super(code);
            this.code = code;
        }
        String code() { return code; }
    }
    Map<String, Object> inspect(String inputPath) throws InspectionException {
        if (inputPath == null || inputPath.isBlank() || inputPath.length() > Protocol.MAX_PATH_LENGTH) {
            throw new InspectionException("INVALID_INPUT_PATH");
        }
        Path path;
        try {
            path = Path.of(inputPath);
        } catch (RuntimeException exception) {
            throw new InspectionException("INVALID_INPUT_PATH");
        }
        BasicFileAttributes attributes;
        try {
            attributes = Files.readAttributes(path, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS);
        } catch (IOException exception) {
            throw new InspectionException("INPUT_UNAVAILABLE");
        }
        if (attributes.isSymbolicLink()) throw new InspectionException("INPUT_SYMLINK_REJECTED");
        if (!attributes.isRegularFile()) throw new InspectionException("INPUT_NOT_REGULAR_FILE");
        if (attributes.size() > Protocol.MAX_INPUT_BYTES) throw new InspectionException("INPUT_TOO_LARGE");
        MessageDigest digest;
        try {
            digest = MessageDigest.getInstance("SHA-256");
        } catch (NoSuchAlgorithmException impossible) {
            throw new IllegalStateException("SHA-256 unavailable", impossible);
        }
        long bytesRead = 0;
        MarkerCounter markers = new MarkerCounter();
        byte[] header = new byte[5];
        int headerLength = 0;
        try (SeekableByteChannel input = Files.newByteChannel(
            path,
            Set.<OpenOption>of(java.nio.file.StandardOpenOption.READ, LinkOption.NOFOLLOW_LINKS)
        )) {
            ByteBuffer buffer = ByteBuffer.allocate(64 * 1024);
            while (input.read(buffer) != -1) {
                buffer.flip();
                byte[] chunk = new byte[buffer.remaining()];
                buffer.get(chunk);
                buffer.clear();
                bytesRead += chunk.length;
                if (bytesRead > Protocol.MAX_INPUT_BYTES) throw new InspectionException("INPUT_TOO_LARGE");
                digest.update(chunk);
                if (headerLength < header.length) {
                    int copyLength = Math.min(header.length - headerLength, chunk.length);
                    System.arraycopy(chunk, 0, header, headerLength, copyLength);
                    headerLength += copyLength;
                }
                markers.accept(chunk);
            }
        } catch (InspectionException exception) {
            throw exception;
        } catch (IOException exception) {
            throw new InspectionException("INPUT_READ_FAILED");
        }
        Map<String, Object> result = new LinkedHashMap<>();
        result.put("inputSha256", HexFormat.of().formatHex(digest.digest()));
        result.put("byteLength", bytesRead);
        result.put("startsWithPdfHeader", headerLength == 5
            && header[0] == '%' && header[1] == 'P' && header[2] == 'D' && header[3] == 'F' && header[4] == '-');
        result.put("eofMarkerPresent", markers.eofMarkers > 0);
        result.put("byteRangeMarkerCount", markers.byteRangeMarkers);
        result.put("signatureDictionaryMarkerCount", markers.signatureDictionaryMarkers);
        result.put("signatureFieldMarkerCount", markers.signatureFieldMarkers);
        result.put("structuralOnly", true);
        result.put("validationPerformed", false);
        result.put("warning", "Phase 0 inspection does not establish signature presence, integrity, identity, trust, or validity.");
        return result;
    }
    private static final class MarkerCounter {
        private static final byte[] BYTE_RANGE = "/ByteRange".getBytes(java.nio.charset.StandardCharsets.US_ASCII);
        private static final byte[] TYPE_SIG = "/Type /Sig".getBytes(java.nio.charset.StandardCharsets.US_ASCII);
        private static final byte[] FIELD_SIG = "/FT /Sig".getBytes(java.nio.charset.StandardCharsets.US_ASCII);
        private static final byte[] EOF = "%%EOF".getBytes(java.nio.charset.StandardCharsets.US_ASCII);
        private static final int OVERLAP = 10;
        private byte[] tail = new byte[0];
        private int byteRangeMarkers;
        private int signatureDictionaryMarkers;
        private int signatureFieldMarkers;
        private int eofMarkers;
        void accept(byte[] chunk) {
            byte[] combined = new byte[tail.length + chunk.length];
            System.arraycopy(tail, 0, combined, 0, tail.length);
            System.arraycopy(chunk, 0, combined, tail.length, chunk.length);
            byteRangeMarkers = boundedAdd(byteRangeMarkers, count(combined, BYTE_RANGE, tail.length));
            signatureDictionaryMarkers = boundedAdd(signatureDictionaryMarkers, count(combined, TYPE_SIG, tail.length));
            signatureFieldMarkers = boundedAdd(signatureFieldMarkers, count(combined, FIELD_SIG, tail.length));
            eofMarkers = boundedAdd(eofMarkers, count(combined, EOF, tail.length));
            int tailLength = Math.min(OVERLAP, combined.length);
            tail = new byte[tailLength];
            System.arraycopy(combined, combined.length - tailLength, tail, 0, tailLength);
        }
        private static int count(byte[] haystack, byte[] needle, int priorTailLength) {
            int count = 0;
            int firstPossibleNewMatch = Math.max(0, priorTailLength - needle.length + 1);
            outer: for (int index = firstPossibleNewMatch; index <= haystack.length - needle.length; index++) {
                for (int offset = 0; offset < needle.length; offset++) {
                    if (haystack[index + offset] != needle[offset]) continue outer;
                }
                count++;
            }
            return count;
        }
        private static int boundedAdd(int current, int increment) {
            return Math.min(Protocol.MAX_STRUCTURAL_MARKERS, current + increment);
        }
    }
}
