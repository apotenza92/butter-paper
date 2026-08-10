package com.butterpaper.signaturecore;

import eu.europa.esig.dss.model.x509.CertificateToken;
import eu.europa.esig.dss.token.DSSPrivateKeyEntry;
import eu.europa.esig.dss.token.Pkcs12SignatureToken;

import javax.security.auth.DestroyFailedException;
import java.security.KeyStore;
import java.security.interfaces.ECKey;
import java.security.interfaces.RSAKey;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

final class Pkcs12IdentityService {
    static final int MAX_PKCS12_BYTES = 16 * 1024 * 1024;

    static final class IdentityException extends Exception {
        private final String code;

        IdentityException(String code) {
            super(code);
            this.code = code;
        }

        String code() { return code; }
    }

    record Identity(DSSPrivateKeyEntry key, String fingerprint) {}

    static final class UnlockedToken implements AutoCloseable {
        private final byte[] pkcs12;
        private final char[] password;
        private final KeyStore.PasswordProtection protection;
        private final Pkcs12SignatureToken token;
        private final List<Identity> identities;

        UnlockedToken(
            byte[] pkcs12,
            char[] password,
            KeyStore.PasswordProtection protection,
            Pkcs12SignatureToken token,
            List<Identity> identities
        ) {
            this.pkcs12 = pkcs12;
            this.password = password;
            this.protection = protection;
            this.token = token;
            this.identities = identities;
        }

        Pkcs12SignatureToken token() { return token; }
        List<Identity> identities() { return identities; }

        Identity select(String fingerprint) throws IdentityException {
            List<Identity> matches = identities.stream()
                .filter(identity -> identity.fingerprint().equals(fingerprint))
                .toList();
            if (matches.size() != 1) throw new IdentityException("IDENTITY_NOT_FOUND");
            return matches.getFirst();
        }

        @Override
        public void close() {
            try {
                token.close();
            } finally {
                try {
                    protection.destroy();
                } catch (DestroyFailedException ignored) {
                    // The mutable password and container buffers are still cleared below.
                }
                Arrays.fill(password, '\0');
                Arrays.fill(pkcs12, (byte) 0);
            }
        }
    }

    private final Pkcs12PasswordPrompt prompt;

    Pkcs12IdentityService(Pkcs12PasswordPrompt prompt) {
        this.prompt = prompt;
    }

    UnlockedToken unlock(byte[] pkcs12) throws IdentityException {
        if (pkcs12 == null || pkcs12.length == 0 || pkcs12.length > MAX_PKCS12_BYTES) {
            throw new IdentityException("INVALID_PKCS12");
        }
        char[] password;
        try {
            password = prompt.requestPassword();
        } catch (Pkcs12PasswordPrompt.PromptException exception) {
            Arrays.fill(pkcs12, (byte) 0);
            throw new IdentityException(exception.code());
        }
        KeyStore.PasswordProtection protection = new KeyStore.PasswordProtection(password);
        Pkcs12SignatureToken token = null;
        try {
            token = new Pkcs12SignatureToken(pkcs12, protection);
            List<Identity> identities = new ArrayList<>();
            for (DSSPrivateKeyEntry key : token.getKeys()) {
                identities.add(new Identity(key, fingerprint(key.getCertificate())));
            }
            identities.sort(Comparator.comparing(Identity::fingerprint));
            if (identities.isEmpty()) throw new IdentityException("PRIVATE_KEY_MISSING");
            return new UnlockedToken(pkcs12, password, protection, token, List.copyOf(identities));
        } catch (IdentityException exception) {
            if (token != null) token.close();
            destroy(protection, password, pkcs12);
            throw exception;
        } catch (RuntimeException exception) {
            if (token != null) token.close();
            destroy(protection, password, pkcs12);
            throw new IdentityException("PKCS12_UNLOCK_FAILED");
        }
    }

    List<Map<String, Object>> describe(UnlockedToken unlocked) throws IdentityException {
        List<Map<String, Object>> descriptions = new ArrayList<>();
        for (Identity identity : unlocked.identities()) {
            CertificateToken certificate = identity.key().getCertificate();
            Map<String, Object> value = new LinkedHashMap<>();
            value.put("certificateSha256", identity.fingerprint());
            value.put("subject", bounded(certificate.getSubject().getRFC2253()));
            value.put("issuer", bounded(certificate.getIssuer().getRFC2253()));
            value.put("serialNumber", certificate.getSerialNumber().toString(16));
            value.put("validFrom", certificate.getNotBefore().toInstant().toString());
            value.put("validTo", certificate.getNotAfter().toInstant().toString());
            value.put("keyAlgorithm", certificate.getPublicKey().getAlgorithm());
            value.put("keyBits", keyBits(certificate));
            value.put("chainSha256", Arrays.stream(identity.key().getCertificateChain())
                .map(Pkcs12IdentityService::fingerprintUnchecked)
                .toList());
            value.put("supportedDigests", supportedDigests(certificate));
            value.put("hasPrivateKey", true);
            descriptions.add(value);
        }
        return List.copyOf(descriptions);
    }

    static List<String> supportedDigests(CertificateToken certificate) {
        String algorithm = certificate.getPublicKey().getAlgorithm();
        if ("RSA".equalsIgnoreCase(algorithm) || "EC".equalsIgnoreCase(algorithm)) {
            return List.of("SHA-256", "SHA-384", "SHA-512");
        }
        return List.of();
    }

    static String fingerprint(CertificateToken certificate) throws IdentityException {
        try {
            return SafePdfMutation.sha256(certificate.getEncoded());
        } catch (SafePdfMutation.MutationException exception) {
            throw new IdentityException("CERTIFICATE_INVALID");
        }
    }

    private static String fingerprintUnchecked(CertificateToken certificate) {
        try {
            return fingerprint(certificate);
        } catch (IdentityException exception) {
            return "";
        }
    }

    private static int keyBits(CertificateToken certificate) {
        if (certificate.getPublicKey() instanceof RSAKey rsa) return rsa.getModulus().bitLength();
        if (certificate.getPublicKey() instanceof ECKey ec) return ec.getParams().getOrder().bitLength();
        return 0;
    }

    private static String bounded(String value) {
        if (value == null) return "";
        return value.length() <= 2_048 ? value : value.substring(0, 2_048);
    }

    private static void destroy(
        KeyStore.PasswordProtection protection,
        char[] password,
        byte[] pkcs12
    ) {
        try {
            protection.destroy();
        } catch (DestroyFailedException ignored) {
            // Mutable arrays are still cleared.
        }
        Arrays.fill(password, '\0');
        Arrays.fill(pkcs12, (byte) 0);
    }
}
