package com.butterpaper.signaturecore;
final class Protocol {
    static final int VERSION = 1;
    static final String ENGINE_VERSION = "0.1.0";
    static final int MAX_LINE_BYTES = 1_048_576;
    static final int MAX_JSON_DEPTH = 32;
    static final int MAX_JSON_STRING_LENGTH = 262_144;
    static final int MAX_JSON_NUMBER_LENGTH = 1_000;
    static final int MAX_CONTAINER_ENTRIES = 1_024;
    static final int MAX_REQUEST_ID_LENGTH = 128;
    static final int MAX_PATH_LENGTH = 4_096;
    static final long MAX_INPUT_BYTES = 512L * 1024L * 1024L;
    static final int MAX_STRUCTURAL_MARKERS = 4_096;
    private Protocol() {}
}
