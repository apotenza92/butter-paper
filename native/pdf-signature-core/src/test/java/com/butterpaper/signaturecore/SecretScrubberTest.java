package com.butterpaper.signaturecore;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

class SecretScrubberTest {
    @Test
    void removesSecretsPathsControlCharactersAndBoundsOutput() {
        String scrubbed = SecretScrubber.scrub(
            "password=hunter2 pin:1234 private_key=/Users/alex/secret.p12 file=/tmp/customer.pdf \u0000" + "x".repeat(2_000)
        );
        assertFalse(scrubbed.contains("hunter2"));
        assertFalse(scrubbed.contains("1234"));
        assertFalse(scrubbed.contains("/Users/alex"));
        assertFalse(scrubbed.contains("/tmp/customer"));
        assertFalse(scrubbed.contains("\u0000"));
        assertTrue(scrubbed.length() <= 1_025);
    }
}
