package com.butterpaper.signaturecore;
import java.util.regex.Pattern;
final class SecretScrubber {
    private static final Pattern SENSITIVE_ASSIGNMENT = Pattern.compile(
        "(?i)(password|passphrase|pin|private[-_ ]?key|token|secret)\\s*[:=]\\s*([^\\s,;]+)"
    );
    private static final Pattern FILE_PATH = Pattern.compile(
        "(?:(?:[A-Za-z]:[\\\\/])|/)[^\\r\\n\\t ]+"
    );
    private static final Pattern CONTROL = Pattern.compile("[\\p{Cntrl}&&[^\\r\\n\\t]]");
    private SecretScrubber() {}
    static String scrub(String diagnostic) {
        if (diagnostic == null || diagnostic.isBlank()) return "diagnostic unavailable";
        String safe = CONTROL.matcher(diagnostic).replaceAll("?");
        safe = SENSITIVE_ASSIGNMENT.matcher(safe).replaceAll("$1=[REDACTED]");
        safe = FILE_PATH.matcher(safe).replaceAll("[PATH]");
        return safe.length() <= 1_024 ? safe : safe.substring(0, 1_024) + "…";
    }
}
