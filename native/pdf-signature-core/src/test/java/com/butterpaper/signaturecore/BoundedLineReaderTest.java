package com.butterpaper.signaturecore;

import org.junit.jupiter.api.Test;

import java.io.ByteArrayInputStream;
import java.nio.charset.StandardCharsets;

import static org.junit.jupiter.api.Assertions.*;

class BoundedLineReaderTest {
    @Test
    void acceptsCrLfAndEndOfStreamWithoutNewline() throws Exception {
        BoundedLineReader reader = new BoundedLineReader(
            new ByteArrayInputStream("one\r\ntwo".getBytes(StandardCharsets.UTF_8)), 16
        );
        assertEquals("one", reader.read().value());
        assertEquals("two", reader.read().value());
        assertNull(reader.read());
    }

    @Test
    void drainsAnOversizedLineSoTheNextRequestCanBeRead() throws Exception {
        BoundedLineReader reader = new BoundedLineReader(
            new ByteArrayInputStream("12345\nok\n".getBytes(StandardCharsets.UTF_8)), 4
        );
        assertTrue(reader.read().tooLarge());
        assertEquals("ok", reader.read().value());
    }

    @Test
    void rejectsMalformedUtf8() throws Exception {
        BoundedLineReader reader = new BoundedLineReader(
            new ByteArrayInputStream(new byte[]{(byte) 0xc3, 0x28, '\n'}), 4
        );
        assertTrue(reader.read().invalidUtf8());
    }
}
