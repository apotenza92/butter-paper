package com.butterpaper.signaturecore;

import com.fasterxml.jackson.databind.ObjectMapper;
import eu.europa.esig.dss.model.x509.CertificateToken;
import org.bouncycastle.asn1.x500.X500Name;
import org.bouncycastle.cert.jcajce.JcaX509CertificateConverter;
import org.bouncycastle.cert.jcajce.JcaX509v3CertificateBuilder;
import org.bouncycastle.jce.provider.BouncyCastleProvider;
import org.bouncycastle.operator.jcajce.JcaContentSignerBuilder;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;

import java.math.BigInteger;
import java.nio.charset.StandardCharsets;
import java.security.KeyPairGenerator;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.security.Security;
import java.security.cert.X509Certificate;
import java.time.Instant;
import java.util.Base64;
import java.util.Date;
import java.util.HexFormat;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

class ExactTrustPolicyTest {
    private static final ObjectMapper JSON = new ObjectMapper();

    @BeforeAll
    static void installProvider() {
        if (Security.getProvider(BouncyCastleProvider.PROVIDER_NAME) == null) {
            Security.addProvider(new BouncyCastleProvider());
        }
    }

    @Test
    void emptyPolicyHasTheMainProcessCanonicalConfigurationHash() {
        ExactTrustPolicy policy = ExactTrustPolicy.empty();
        assertEquals("butter-paper-local-explicit-certificates", policy.policyId());
        assertEquals(1, policy.policyVersion());
        assertEquals("65621a8373d3e6869d50a8572da7d20ae5c4d7c91a915eeda34493187f071f0e", policy.configurationSha256());
        assertTrue(policy.configuredFingerprints().isEmpty());
    }

    @Test
    void trustsOnlyTheExactConfiguredCertificateAndBindsTheConfiguration() throws Exception {
        X509Certificate trusted = certificate("trusted", BigInteger.ONE);
        X509Certificate other = certificate("other", BigInteger.TWO);
        byte[] der = trusted.getEncoded();
        String fingerprint = sha256(der);
        String configuration = configurationSha(fingerprint);
        ExactTrustPolicy policy = ExactTrustPolicy.parse(JSON.valueToTree(Map.of(
            "policyId", ExactTrustPolicy.DEFAULT_POLICY_ID,
            "policyVersion", 1,
            "configurationSha256", configuration,
            "exactCertificateAnchors", new Object[]{Map.of(
                "sha256Fingerprint", fingerprint,
                "derBase64", Base64.getEncoder().encodeToString(der)
            )}
        )));

        assertTrue(policy.explicitlyTrusts(new CertificateToken(trusted)));
        assertFalse(policy.explicitlyTrusts(new CertificateToken(other)));
        assertEquals(java.util.List.of(fingerprint), policy.configuredFingerprints());
    }

    @Test
    void rejectsFingerprintConfigurationAndDuplicateMismatchesWithoutFallback() throws Exception {
        byte[] der = certificate("trusted", BigInteger.ONE).getEncoded();
        String fingerprint = sha256(der);
        Map<String, String> anchor = Map.of(
            "sha256Fingerprint", fingerprint,
            "derBase64", Base64.getEncoder().encodeToString(der)
        );

        assertThrows(ExactTrustPolicy.PolicyException.class, () -> ExactTrustPolicy.parse(JSON.valueToTree(Map.of(
            "policyId", ExactTrustPolicy.DEFAULT_POLICY_ID,
            "policyVersion", 1,
            "configurationSha256", "0".repeat(64),
            "exactCertificateAnchors", new Object[]{anchor}
        ))));
        assertThrows(ExactTrustPolicy.PolicyException.class, () -> ExactTrustPolicy.parse(JSON.valueToTree(Map.of(
            "policyId", ExactTrustPolicy.DEFAULT_POLICY_ID,
            "policyVersion", 1,
            "configurationSha256", configurationSha(fingerprint),
            "exactCertificateAnchors", new Object[]{anchor, anchor}
        ))));
        assertThrows(ExactTrustPolicy.PolicyException.class, () -> ExactTrustPolicy.parse(JSON.valueToTree(Map.of(
            "policyId", ExactTrustPolicy.DEFAULT_POLICY_ID,
            "policyVersion", 1,
            "configurationSha256", configurationSha("f".repeat(64)),
            "exactCertificateAnchors", new Object[]{Map.of(
                "sha256Fingerprint", "f".repeat(64),
                "derBase64", Base64.getEncoder().encodeToString(der)
            )}
        ))));
    }

    @Test
    void rejectsOversizedDerAndTooManyAnchors() throws Exception {
        byte[] oversized = new byte[ExactTrustPolicy.MAX_CERTIFICATE_DER_BYTES + 1];
        String fingerprint = sha256(oversized);
        assertThrows(ExactTrustPolicy.PolicyException.class, () -> ExactTrustPolicy.parse(JSON.valueToTree(Map.of(
            "policyId", ExactTrustPolicy.DEFAULT_POLICY_ID,
            "policyVersion", 1,
            "configurationSha256", configurationSha(fingerprint),
            "exactCertificateAnchors", new Object[]{Map.of(
                "sha256Fingerprint", fingerprint,
                "derBase64", Base64.getEncoder().encodeToString(oversized)
            )}
        ))));

        Object[] anchors = new Object[ExactTrustPolicy.MAX_ANCHORS + 1];
        java.util.Arrays.fill(anchors, Map.of("sha256Fingerprint", "0".repeat(64), "derBase64", "AA=="));
        assertThrows(ExactTrustPolicy.PolicyException.class, () -> ExactTrustPolicy.parse(JSON.valueToTree(Map.of(
            "policyId", ExactTrustPolicy.DEFAULT_POLICY_ID,
            "policyVersion", 1,
            "configurationSha256", "0".repeat(64),
            "exactCertificateAnchors", anchors
        ))));
    }

    private static X509Certificate certificate(String seed, BigInteger serial) throws Exception {
        KeyPairGenerator generator = KeyPairGenerator.getInstance("RSA");
        SecureRandom random = SecureRandom.getInstance("SHA1PRNG");
        random.setSeed(seed.getBytes(StandardCharsets.US_ASCII));
        generator.initialize(2048, random);
        var keyPair = generator.generateKeyPair();
        X500Name subject = new X500Name("CN=Butter Paper Exact Trust Test,O=Butter Paper Tests,C=AU");
        var signer = new JcaContentSignerBuilder("SHA256withRSA")
            .setProvider(BouncyCastleProvider.PROVIDER_NAME)
            .build(keyPair.getPrivate());
        return new JcaX509CertificateConverter()
            .setProvider(BouncyCastleProvider.PROVIDER_NAME)
            .getCertificate(new JcaX509v3CertificateBuilder(
                subject,
                serial,
                Date.from(Instant.parse("2025-08-05T00:00:00Z")),
                Date.from(Instant.parse("2036-08-05T00:00:00Z")),
                subject,
                keyPair.getPublic()
            ).build(signer));
    }

    private static String configurationSha(String fingerprint) throws Exception {
        String canonical = "{\"policyId\":\"butter-paper-local-explicit-certificates\",\"policyVersion\":1,"
            + "\"enabledExactCertificateFingerprints\":[\"" + fingerprint + "\"]}";
        return sha256(canonical.getBytes(StandardCharsets.UTF_8));
    }

    private static String sha256(byte[] value) throws Exception {
        return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(value));
    }
}
