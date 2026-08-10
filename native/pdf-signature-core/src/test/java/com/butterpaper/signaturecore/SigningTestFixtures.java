package com.butterpaper.signaturecore;

import org.apache.pdfbox.pdmodel.PDDocument;
import org.apache.pdfbox.pdmodel.PDPage;
import org.apache.pdfbox.pdmodel.common.PDRectangle;
import org.apache.pdfbox.pdfwriter.compress.CompressParameters;
import org.bouncycastle.asn1.x500.X500Name;
import org.bouncycastle.asn1.x509.BasicConstraints;
import org.bouncycastle.asn1.x509.Extension;
import org.bouncycastle.asn1.x509.KeyUsage;
import org.bouncycastle.cert.X509v3CertificateBuilder;
import org.bouncycastle.cert.jcajce.JcaX509CertificateConverter;
import org.bouncycastle.cert.jcajce.JcaX509v3CertificateBuilder;
import org.bouncycastle.operator.ContentSigner;
import org.bouncycastle.operator.jcajce.JcaContentSignerBuilder;

import javax.imageio.ImageIO;
import java.awt.Color;
import java.awt.image.BufferedImage;
import java.io.ByteArrayOutputStream;
import java.math.BigInteger;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.attribute.PosixFilePermission;
import java.security.KeyPair;
import java.security.KeyPairGenerator;
import java.security.KeyStore;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.security.cert.X509Certificate;
import java.security.spec.ECGenParameterSpec;
import java.time.Instant;
import java.util.Date;
import java.util.HexFormat;
import java.util.Set;

final class SigningTestFixtures {
    static final char[] PASSWORD = "Butter Paper TEST ONLY password".toCharArray();

    record IdentityFixture(byte[] pkcs12, X509Certificate certificate, String fingerprint) {}
    record Workspace(Path directory, Path source, Path output, byte[] sourceBytes, String sourceSha256) {}

    private SigningTestFixtures() {}

    static IdentityFixture identity(String algorithm) throws Exception {
        SecureRandom random = SecureRandom.getInstance("SHA1PRNG");
        random.setSeed(("Butter Paper TEST ONLY " + algorithm).getBytes(java.nio.charset.StandardCharsets.UTF_8));
        KeyPairGenerator generator = KeyPairGenerator.getInstance(algorithm);
        if ("RSA".equals(algorithm)) generator.initialize(2048, random);
        else generator.initialize(new ECGenParameterSpec("secp256r1"), random);
        KeyPair pair = generator.generateKeyPair();
        X500Name name = new X500Name("CN=Butter Paper TEST ONLY " + algorithm + ",O=Butter Paper Tests,C=AU");
        X509v3CertificateBuilder builder = new JcaX509v3CertificateBuilder(
            name,
            BigInteger.valueOf("RSA".equals(algorithm) ? 101 : 102),
            Date.from(Instant.parse("2020-01-01T00:00:00Z")),
            Date.from(Instant.parse("2045-01-01T00:00:00Z")),
            name,
            pair.getPublic()
        );
        builder.addExtension(Extension.basicConstraints, true, new BasicConstraints(false));
        builder.addExtension(Extension.keyUsage, true, new KeyUsage(KeyUsage.digitalSignature));
        ContentSigner signer = new JcaContentSignerBuilder(
            "RSA".equals(algorithm) ? "SHA256withRSA" : "SHA256withECDSA"
        ).setSecureRandom(random).build(pair.getPrivate());
        X509Certificate certificate = new JcaX509CertificateConverter().getCertificate(builder.build(signer));
        certificate.verify(pair.getPublic());

        KeyStore store = KeyStore.getInstance("PKCS12");
        store.load(null, PASSWORD);
        store.setKeyEntry("butter-paper-test-only", pair.getPrivate(), PASSWORD, new java.security.cert.Certificate[]{certificate});
        ByteArrayOutputStream encoded = new ByteArrayOutputStream();
        store.store(encoded, PASSWORD);
        byte[] pkcs12 = encoded.toByteArray();
        String fingerprint = HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(certificate.getEncoded()));
        return new IdentityFixture(pkcs12, certificate, fingerprint);
    }

    static byte[] pdf() throws Exception {
        try (PDDocument document = new PDDocument()) {
            document.addPage(new PDPage(PDRectangle.A4));
            document.getDocument().setIsXRefStream(false);
            ByteArrayOutputStream encoded = new ByteArrayOutputStream();
            document.save(encoded, CompressParameters.NO_COMPRESSION);
            return encoded.toByteArray();
        }
    }

    static byte[] pdfWithGeometry(int rotation, PDRectangle cropBox, float userUnit) throws Exception {
        try (PDDocument document = new PDDocument()) {
            PDPage page = new PDPage(new PDRectangle(800, 600));
            page.setRotation(rotation);
            page.setCropBox(cropBox);
            page.setUserUnit(userUnit);
            document.addPage(page);
            document.getDocument().setIsXRefStream(false);
            ByteArrayOutputStream encoded = new ByteArrayOutputStream();
            document.save(encoded, CompressParameters.NO_COMPRESSION);
            return encoded.toByteArray();
        }
    }

    static Workspace workspace(byte[] sourceBytes) throws Exception {
        Path directory = Files.createTempDirectory("butter-paper-signing-test-");
        setPermissions(directory, Set.of(
            PosixFilePermission.OWNER_READ,
            PosixFilePermission.OWNER_WRITE,
            PosixFilePermission.OWNER_EXECUTE
        ));
        Path source = directory.resolve("source.pdf");
        Path output = directory.resolve("output.pdf");
        Files.write(source, sourceBytes);
        Files.write(output, new byte[0]);
        setPermissions(source, Set.of(PosixFilePermission.OWNER_READ, PosixFilePermission.OWNER_WRITE));
        setPermissions(output, Set.of(PosixFilePermission.OWNER_READ, PosixFilePermission.OWNER_WRITE));
        return new Workspace(
            directory,
            source,
            output,
            sourceBytes,
            HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(sourceBytes))
        );
    }

    static byte[] png() throws Exception {
        BufferedImage image = new BufferedImage(8, 4, BufferedImage.TYPE_INT_ARGB);
        for (int y = 0; y < image.getHeight(); y++) {
            for (int x = 0; x < image.getWidth(); x++) image.setRGB(x, y, new Color(30, 60, 120, 220).getRGB());
        }
        ByteArrayOutputStream encoded = new ByteArrayOutputStream();
        ImageIO.write(image, "png", encoded);
        return encoded.toByteArray();
    }

    static Pkcs12PasswordPrompt fixedPrompt() {
        return () -> PASSWORD.clone();
    }

    private static void setPermissions(Path path, Set<PosixFilePermission> permissions) throws Exception {
        try {
            Files.setPosixFilePermissions(path, permissions);
        } catch (UnsupportedOperationException ignored) {
            // Windows ACL behavior is covered by the platform harness.
        }
    }
}
