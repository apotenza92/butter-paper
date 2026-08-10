package com.butterpaper.signaturecore;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.ByteBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
final class BoundedLineReader {
    record Line(String value, boolean tooLarge, boolean invalidUtf8) {}
    private final InputStream input;
    private final int limit;
    BoundedLineReader(InputStream input, int limit) {
        this.input = input;
        this.limit = limit;
    }
    Line read() throws IOException {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream(Math.min(limit, 8_192));
        boolean sawAny = false;
        boolean tooLarge = false;
        while (true) {
            int next = input.read();
            if (next == -1 || next == '\n') {
                if (!sawAny && next == -1) return null;
                break;
            }
            sawAny = true;
            if (bytes.size() < limit) {
                bytes.write(next);
            } else {
                tooLarge = true;
            }
        }
        if (tooLarge) return new Line(null, true, false);
        byte[] encoded = bytes.toByteArray();
        if (encoded.length > 0 && encoded[encoded.length - 1] == '\r') {
            byte[] withoutCarriageReturn = new byte[encoded.length - 1];
            System.arraycopy(encoded, 0, withoutCarriageReturn, 0, encoded.length - 1);
            encoded = withoutCarriageReturn;
        }
        try {
            String decoded = StandardCharsets.UTF_8.newDecoder()
                .onMalformedInput(CodingErrorAction.REPORT)
                .onUnmappableCharacter(CodingErrorAction.REPORT)
                .decode(ByteBuffer.wrap(encoded))
                .toString();
            return new Line(decoded, false, false);
        } catch (CharacterCodingException exception) {
            return new Line(null, false, true);
        }
    }
}
