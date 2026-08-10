package com.butterpaper.signaturecore;
import com.fasterxml.jackson.databind.JsonNode;
import eu.europa.esig.dss.model.x509.CertificateToken;
import java.io.ByteArrayInputStream;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.security.cert.CertificateException;
import java.security.cert.CertificateFactory;
import java.security.cert.X509Certificate;
import java.util.ArrayList;
import java.util.Base64;
import java.util.Comparator;
import java.util.HashSet;
import java.util.HexFormat;
import java.util.List;
import java.util.Set;
import java.util.regex.Pattern;
/** A bounded exact-certificate decision set. It is never a CA/path trust store. */
final class ExactTrustPolicy {
    static final String DEFAULT_POLICY_ID = "butter-paper-local-explicit-certificates";
    static final String POLICY_DISPLAY_NAME = "Butter Paper local exact-certificate trust (offline)";
    static final int DEFAULT_POLICY_VERSION = 1;
    static final int MAX_ANCHORS = 16;
    static final int MAX_CERTIFICATE_DER_BYTES = 32 * 1024;
    static final int MAX_TOTAL_DER_BYTES = 512 * 1024;
    private static final long MAX_SAFE_INTEGER = 9_007_199_254_740_991L;
    private static final Pattern POLICY_ID = Pattern.compile("[A-Za-z0-9][A-Za-z0-9._:-]{0,127}");
    private static final Pattern SHA256 = Pattern.compile("[a-f0-9]{64}");
    static final class PolicyException extends Exception {
        PolicyException() { super("INVALID_TRUST_POLICY"); }
    }
    private final String policyId;
    private final long policyVersion;
    private final String configurationSha256;
    private final List<Anchor> anchors;
    private ExactTrustPolicy(
        String policyId,
        long policyVersion,
        String configurationSha256,
        List<Anchor> anchors
    ) {
        this.policyId = policyId;
        this.policyVersion = policyVersion;
        this.configurationSha256 = configurationSha256;
        this.anchors = List.copyOf(anchors);
    }
    static ExactTrustPolicy empty() {
        List<Anchor> anchors = List.of();
        return new ExactTrustPolicy(
            DEFAULT_POLICY_ID,
            DEFAULT_POLICY_VERSION,
            configurationDigest(DEFAULT_POLICY_ID, DEFAULT_POLICY_VERSION, anchors),
            anchors
        );
    }
    static ExactTrustPolicy parse(JsonNode node) throws PolicyException {
        if (node == null || !node.isObject() || node.size() != 4) throw new PolicyException();
        String policyId = requiredText(node.get("policyId"));
        if (policyId == null || !POLICY_ID.matcher(policyId).matches()) throw new PolicyException();
        JsonNode versionNode = node.get("policyVersion");
        if (versionNode == null || !versionNode.isIntegralNumber()) throw new PolicyException();
        long policyVersion = versionNode.longValue();
        if (policyVersion < 1 || policyVersion > MAX_SAFE_INTEGER) throw new PolicyException();
        String configurationSha256 = requiredText(node.get("configurationSha256"));
        if (configurationSha256 == null || !SHA256.matcher(configurationSha256).matches()) {
            throw new PolicyException();
        }
        JsonNode anchorNodes = node.get("exactCertificateAnchors");
        if (anchorNodes == null || !anchorNodes.isArray() || anchorNodes.size() > MAX_ANCHORS) {
            throw new PolicyException();
        }
        List<Anchor> anchors = new ArrayList<>();
        Set<String> fingerprints = new HashSet<>();
        int totalDerBytes = 0;
        for (JsonNode anchorNode : anchorNodes) {
            if (!anchorNode.isObject() || anchorNode.size() != 2) throw new PolicyException();
            String fingerprint = requiredText(anchorNode.get("sha256Fingerprint"));
            String encoded = requiredText(anchorNode.get("derBase64"));
            if (fingerprint == null || !SHA256.matcher(fingerprint).matches()
                || encoded == null || !fingerprints.add(fingerprint)) {
                throw new PolicyException();
            }
            byte[] der;
            try {
                der = Base64.getDecoder().decode(encoded);
            } catch (IllegalArgumentException exception) {
                throw new PolicyException();
            }
            if (der.length == 0 || der.length > MAX_CERTIFICATE_DER_BYTES) throw new PolicyException();
            totalDerBytes += der.length;
            if (totalDerBytes > MAX_TOTAL_DER_BYTES || !fingerprint.equals(sha256(der))) {
                throw new PolicyException();
            }
            validateExactCertificate(der);
            anchors.add(new Anchor(fingerprint, der));
        }
        anchors.sort(Comparator.comparing(Anchor::fingerprint));
        if (!configurationSha256.equals(configurationDigest(policyId, policyVersion, anchors))) {
            throw new PolicyException();
        }
        return new ExactTrustPolicy(policyId, policyVersion, configurationSha256, anchors);
    }
    boolean explicitlyTrusts(CertificateToken certificate) {
        if (certificate == null) return false;
        byte[] encoded = certificate.getEncoded();
        String fingerprint = sha256(encoded);
        return anchors.stream().anyMatch(anchor -> anchor.fingerprint().equals(fingerprint)
            && MessageDigest.isEqual(anchor.der(), encoded));
    }
    String policyId() { return policyId; }
    long policyVersion() { return policyVersion; }
    String configurationSha256() { return configurationSha256; }
    List<String> configuredFingerprints() {
        return anchors.stream().map(Anchor::fingerprint).toList();
    }
    private static void validateExactCertificate(byte[] der) throws PolicyException {
        try {
            ByteArrayInputStream input = new ByteArrayInputStream(der);
            X509Certificate certificate = (X509Certificate) CertificateFactory
                .getInstance("X.509")
                .generateCertificate(input);
            if (input.available() != 0 || !MessageDigest.isEqual(certificate.getEncoded(), der)) {
                throw new PolicyException();
            }
        } catch (CertificateException exception) {
            throw new PolicyException();
        }
    }
    private static String configurationDigest(String policyId, long policyVersion, List<Anchor> anchors) {
        String fingerprints = anchors.stream()
            .map(anchor -> "\"" + anchor.fingerprint() + "\"")
            .reduce((left, right) -> left + "," + right)
            .orElse("");
        String canonical = "{\"policyId\":\"" + policyId
            + "\",\"policyVersion\":" + policyVersion
            + ",\"enabledExactCertificateFingerprints\":[" + fingerprints + "]}";
        return sha256(canonical.getBytes(StandardCharsets.UTF_8));
    }
    private static String requiredText(JsonNode node) {
        return node != null && node.isTextual() && !node.textValue().isEmpty() ? node.textValue() : null;
    }
    private static String sha256(byte[] bytes) {
        try {
            return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));
        } catch (NoSuchAlgorithmException impossible) {
            throw new IllegalStateException("SHA-256 unavailable", impossible);
        }
    }
    private record Anchor(String fingerprint, byte[] der) {
        private Anchor {
            der = der.clone();
        }
        @Override
        public byte[] der() {
            return der.clone();
        }
    }
}
