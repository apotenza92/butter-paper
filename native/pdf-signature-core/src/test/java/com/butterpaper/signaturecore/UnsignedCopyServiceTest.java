package com.butterpaper.signaturecore;

import com.sun.nio.file.ExtendedOpenOption;
import eu.europa.esig.dss.model.FileDocument;
import eu.europa.esig.dss.pades.validation.PDFDocumentValidator;
import org.apache.pdfbox.Loader;
import org.apache.pdfbox.cos.COSDictionary;
import org.apache.pdfbox.cos.COSArray;
import org.apache.pdfbox.cos.COSName;
import org.apache.pdfbox.pdmodel.PDDocument;
import org.apache.pdfbox.pdmodel.PDPage;
import org.apache.pdfbox.pdmodel.PDPageContentStream;
import org.apache.pdfbox.pdmodel.PDResources;
import org.apache.pdfbox.pdmodel.common.PDRectangle;
import org.apache.pdfbox.pdmodel.interactive.annotation.PDAppearanceDictionary;
import org.apache.pdfbox.pdmodel.interactive.annotation.PDAppearanceStream;
import org.apache.pdfbox.pdmodel.interactive.annotation.PDAnnotationWidget;
import org.apache.pdfbox.pdmodel.interactive.annotation.PDAnnotationText;
import org.apache.pdfbox.pdmodel.interactive.form.PDAcroForm;
import org.apache.pdfbox.pdmodel.interactive.form.PDSignatureField;
import org.apache.pdfbox.pdmodel.interactive.form.PDTextField;
import org.apache.pdfbox.pdfwriter.compress.CompressParameters;
import org.apache.pdfbox.rendering.PDFRenderer;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.math.BigInteger;
import java.awt.image.BufferedImage;
import java.io.IOException;
import java.nio.channels.FileChannel;
import java.nio.channels.FileLock;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.time.Instant;
import java.util.HexFormat;
import java.util.Map;
import java.util.EnumSet;
import java.nio.file.attribute.PosixFilePermission;
import java.nio.file.attribute.BasicFileAttributes;
import java.util.concurrent.TimeUnit;

import static org.junit.jupiter.api.Assertions.*;

class UnsignedCopyServiceTest {
    private static final PDRectangle VISIBLE_SIGNATURE_RECT = new PDRectangle(320, 40, 180, 60);

    @TempDir Path temporaryDirectory;

    @Test
    void createsValidatedUnsignedCopyAndPreservesSourceContentFormsAndAnnotations() throws Exception {
        Path unsigned = temporaryDirectory.resolve("source-unsigned.pdf").toAbsolutePath();
        try (PDDocument document = new PDDocument()) {
            PDPage page = new PDPage();
            document.addPage(page);
            try (PDPageContentStream content = new PDPageContentStream(document, page)) {
                content.setNonStrokingColor(0.15f, 0.45f, 0.75f);
                content.addRect(70, 500, 220, 90);
                content.fill();
            }
            document.getDocumentInformation().setTitle("Unsigned-copy preservation fixture");
            document.getDocumentInformation().setCustomMetadataValue(
                "ButterPaperFixture", "preserve-this-metadata"
            );
            PDAnnotationText annotation = new PDAnnotationText();
            annotation.setContents("Unrelated /ByteRange annotation");
            annotation.setRectangle(new PDRectangle(10, 20, 30, 40));
            annotation.getCOSObject().setString(COSName.getPDFName("ByteRangeNote"), "ordinary metadata");
            page.getAnnotations().add(annotation);
            PDAcroForm form = new PDAcroForm(document);
            document.getDocumentCatalog().setAcroForm(form);
            PDTextField text = new PDTextField(form);
            text.setPartialName("UnrelatedText");
            text.getCOSObject().setString(COSName.V, "preserved value");
            form.getFields().add(text);
            document.save(unsigned.toFile());
        }
        Path signed = temporaryDirectory.resolve("source-signed.pdf").toAbsolutePath();
        ValidationServiceTest.appendSignature(
            unsigned,
            signed,
            "unsigned-copy-test",
            "Butter Paper Unsigned Copy Test",
            BigInteger.valueOf(20260808),
            Instant.parse("2026-08-05T04:00:00Z"),
            true
        );
        Files.delete(unsigned);
        Path visibleSigned = temporaryDirectory.resolve("source-visible-signed.pdf").toAbsolutePath();
        try (PDDocument document = Loader.loadPDF(signed.toFile())) {
            PDSignatureField field = document.getSignatureFields().getFirst();
            PDAnnotationWidget widget = field.getWidgets().getFirst();
            widget.setRectangle(VISIBLE_SIGNATURE_RECT);
            widget.setPage(document.getPage(0));
            if (document.getPage(0).getAnnotations().stream()
                .noneMatch(annotation -> annotation.getCOSObject() == widget.getCOSObject())) {
                document.getPage(0).getAnnotations().add(widget);
            }
            PDAppearanceStream appearance = new PDAppearanceStream(document);
            appearance.setResources(new PDResources());
            appearance.setBBox(new PDRectangle(
                VISIBLE_SIGNATURE_RECT.getWidth(), VISIBLE_SIGNATURE_RECT.getHeight()
            ));
            try (PDPageContentStream content = new PDPageContentStream(document, appearance)) {
                content.setNonStrokingColor(0.86f, 0.34f, 0.10f);
                content.addRect(0, 0, VISIBLE_SIGNATURE_RECT.getWidth(), VISIBLE_SIGNATURE_RECT.getHeight());
                content.fill();
                content.setStrokingColor(0.15f, 0.15f, 0.15f);
                content.setLineWidth(4);
                content.moveTo(18, 18);
                content.lineTo(55, 43);
                content.lineTo(92, 16);
                content.stroke();
            }
            PDAppearanceDictionary appearances = new PDAppearanceDictionary();
            appearances.setNormalAppearance(appearance);
            widget.setAppearance(appearances);
            try (java.io.OutputStream output = Files.newOutputStream(visibleSigned)) {
                document.saveIncremental(output);
            }
        }
        Files.move(visibleSigned, signed, java.nio.file.StandardCopyOption.REPLACE_EXISTING);
        Path withDss = temporaryDirectory.resolve("source-signed-with-dss.pdf").toAbsolutePath();
        try (PDDocument document = Loader.loadPDF(signed.toFile())) {
            document.getDocumentCatalog().getCOSObject().setItem(
                COSName.getPDFName("DSS"), new COSDictionary()
            );
            COSDictionary fieldMdp = new COSDictionary();
            fieldMdp.setName(COSName.getPDFName("TransformMethod"), "FieldMDP");
            COSArray references = new COSArray();
            references.add(fieldMdp);
            document.getSignatureDictionaries().getFirst().getCOSObject().setItem(
                COSName.getPDFName("Reference"), references
            );
            document.getSignatureFields().getFirst().getCOSObject().setItem(
                COSName.getPDFName("Lock"), new COSDictionary()
            );
            try (java.io.OutputStream output = Files.newOutputStream(withDss)) {
                document.saveIncremental(output);
            }
        }
        Files.move(withDss, signed, java.nio.file.StandardCopyOption.REPLACE_EXISTING);
        byte[] sourceBefore = Files.readAllBytes(signed);
        BufferedImage signedRendering = renderFirstPage(signed);

        Path output = temporaryDirectory.resolve("private-output.pdf").toAbsolutePath();
        createPrivateOutput(output);
        Object outputIdentityBefore = Files.readAttributes(
            output, BasicFileAttributes.class, java.nio.file.LinkOption.NOFOLLOW_LINKS
        ).fileKey();
        Map<String, Object> result = new UnsignedCopyService().create(signed.toString(), output.toString());

        assertArrayEquals(sourceBefore, Files.readAllBytes(signed));
        assertEquals(sha256(sourceBefore), result.get("inputSha256"));
        assertEquals(sha256(Files.readAllBytes(output)), result.get("outputSha256"));
        assertEquals(true, result.get("sourcePreserved"));
        assertEquals(true, result.get("validatedUnsigned"));
        assertEquals(outputIdentityBefore, Files.readAttributes(
            output, BasicFileAttributes.class, java.nio.file.LinkOption.NOFOLLOW_LINKS
        ).fileKey(), "native writer must fill the main-owned inode without replacing it");
        @SuppressWarnings("unchecked")
        Map<String, Object> removed = (Map<String, Object>) result.get("removed");
        assertEquals(1, removed.get("signatureValues"));
        assertEquals(1, removed.get("signatureFields"));
        assertEquals(1, removed.get("signatureWidgets"));
        assertEquals(1, removed.get("certificationReferences"));
        assertEquals(2, removed.get("fieldMdpReferences"));
        assertTrue((int) removed.get("validationEvidenceEntries") >= 1);
        @SuppressWarnings("unchecked")
        Map<String, Object> structural = (Map<String, Object>) result.get("structuralPostcheck");
        assertTrue(structural.values().stream().allMatch(value -> Integer.valueOf(0).equals(value)));
        try (PDDocument copy = Loader.loadPDF(output.toFile())) {
            assertTrue(copy.getSignatureDictionaries().isEmpty());
            assertTrue(copy.getSignatureFields().stream().noneMatch(field -> field.getSignature() != null));
            assertEquals("preserved value", copy.getDocumentCatalog().getAcroForm()
                .getField("UnrelatedText").getValueAsString());
            assertTrue(copy.getPage(0).getAnnotations().stream()
                .anyMatch(annotation -> "Unrelated /ByteRange annotation".equals(annotation.getContents())));
            assertTrue(copy.getPage(0).getAnnotations().stream().anyMatch(annotation ->
                "ordinary metadata".equals(annotation.getCOSObject().getString(
                    COSName.getPDFName("ByteRangeNote")
                ))));
            COSDictionary permissions = (COSDictionary) copy.getDocumentCatalog().getCOSObject()
                .getDictionaryObject(COSName.getPDFName("Perms"));
            assertTrue(permissions == null || !permissions.containsKey(COSName.getPDFName("DocMDP")));
            assertFalse(copy.getDocumentCatalog().getCOSObject().containsKey(COSName.getPDFName("DSS")));
            assertEquals("Unsigned-copy preservation fixture", copy.getDocumentInformation().getTitle());
            assertEquals(
                "preserve-this-metadata",
                copy.getDocumentInformation().getCustomMetadataValue("ButterPaperFixture")
            );
        }
        assertRenderingDiffConfinedToSignatureRegion(signedRendering, renderFirstPage(output));
        PDFDocumentValidator validator = new PDFDocumentValidator(new FileDocument(output.toFile()));
        validator.setCertificateVerifier(ValidationService.newOfflineVerifier());
        assertTrue(validator.getSignatures().isEmpty());
    }

    @Test
    void refusesUnsignedInputExistingOutputAndSamePathWithoutChangingEither() throws Exception {
        Path unsigned = temporaryDirectory.resolve("unsigned.pdf").toAbsolutePath();
        try (PDDocument document = new PDDocument()) {
            document.addPage(new PDPage());
            document.save(unsigned.toFile());
        }
        byte[] before = Files.readAllBytes(unsigned);
        Path output = temporaryDirectory.resolve("existing.pdf").toAbsolutePath();
        Files.writeString(output, "preserve");
        Path emptyOutput = temporaryDirectory.resolve("new.pdf").toAbsolutePath();
        createPrivateOutput(emptyOutput);

        UnsignedCopyService.CopyException unsignedError = assertThrows(
            UnsignedCopyService.CopyException.class,
            () -> new UnsignedCopyService().create(unsigned.toString(), emptyOutput.toString())
        );
        assertEquals("INPUT_NOT_SIGNED", unsignedError.code());
        UnsignedCopyService.CopyException existingError = assertThrows(
            UnsignedCopyService.CopyException.class,
            () -> new UnsignedCopyService().create(unsigned.toString(), output.toString())
        );
        assertEquals("OUTPUT_NOT_EMPTY", existingError.code());
        UnsignedCopyService.CopyException samePathError = assertThrows(
            UnsignedCopyService.CopyException.class,
            () -> new UnsignedCopyService().create(unsigned.toString(), unsigned.toString())
        );
        assertEquals("INVALID_OUTPUT_PATH", samePathError.code());
        assertArrayEquals(before, Files.readAllBytes(unsigned));
        assertEquals("preserve", Files.readString(output));
        assertEquals(0, Files.size(emptyOutput));
    }

    @Test
    void rejectsHardlinkedAndNonPrivateOutputTargetsWithoutWriting() throws Exception {
        Path signed = ValidationServiceTest.createSignedPdf(
            temporaryDirectory.resolve("security-source.pdf").toAbsolutePath()
        );
        Path hardlinkedOutput = temporaryDirectory.resolve("hardlinked-output.pdf").toAbsolutePath();
        createPrivateOutput(hardlinkedOutput);
        Path alias = temporaryDirectory.resolve("hardlink-alias.pdf").toAbsolutePath();
        boolean unixAttributes = Files.getFileStore(temporaryDirectory)
            .supportsFileAttributeView("unix");
        if (unixAttributes) {
            Files.createLink(alias, hardlinkedOutput);
            UnsignedCopyService.CopyException hardlinkError = assertThrows(
                UnsignedCopyService.CopyException.class,
                () -> new UnsignedCopyService().create(signed.toString(), hardlinkedOutput.toString())
            );
            assertEquals("OUTPUT_HARDLINK_REJECTED", hardlinkError.code());
            assertEquals(0, Files.size(hardlinkedOutput));
            assertEquals(0, Files.size(alias));
        }

        Path permissiveOutput = temporaryDirectory.resolve("permissive-output.pdf").toAbsolutePath();
        createPrivateOutput(permissiveOutput);
        if (Files.getFileStore(permissiveOutput).supportsFileAttributeView("posix")) {
            Files.setPosixFilePermissions(permissiveOutput, EnumSet.of(
                PosixFilePermission.OWNER_READ,
                PosixFilePermission.OWNER_WRITE,
                PosixFilePermission.GROUP_READ
            ));
            UnsignedCopyService.CopyException permissionsError = assertThrows(
                UnsignedCopyService.CopyException.class,
                () -> new UnsignedCopyService().create(signed.toString(), permissiveOutput.toString())
            );
            assertEquals("OUTPUT_PERMISSIONS_UNSAFE", permissionsError.code());
            assertEquals(0, Files.size(permissiveOutput));
        }
    }

    @Test
    void structuralInspectionDoesNotTreatOrdinaryByteRangeTextAsASignatureMarker() throws Exception {
        Path input = temporaryDirectory.resolve("ordinary-byte-range-text.pdf").toAbsolutePath();
        try (PDDocument document = new PDDocument()) {
            PDPage page = new PDPage();
            page.getCOSObject().setString(COSName.getPDFName("ByteRangeNote"), "/ByteRange text");
            document.addPage(page);
            document.save(input.toFile(), CompressParameters.NO_COMPRESSION);
        }
        String raw = new String(Files.readAllBytes(input), java.nio.charset.StandardCharsets.ISO_8859_1);
        assertTrue(raw.contains("/ByteRangeNote"));
        assertTrue(raw.contains("/ByteRange text"));

        Map<String, Object> inspection = new UnsignedCopyService().inspect(input.toString());

        assertEquals(true, inspection.get("structurallyReadable"));
        assertEquals(0, inspection.get("byteRangeMarkerCount"));
        assertEquals(0, inspection.get("signatureDictionaryCount"));
    }

    @Test
    void windowsWriterRequestsExclusiveDeleteSharingForTheBoundOutputHandle() throws Exception {
        assertTrue(UnsignedCopyService.privateOutputOpenOptions(true)
            .contains(java.nio.file.StandardOpenOption.READ));
        assertTrue(UnsignedCopyService.privateOutputOpenOptions(true)
            .contains(ExtendedOpenOption.NOSHARE_DELETE));
        assertFalse(UnsignedCopyService.privateOutputOpenOptions(false)
            .contains(ExtendedOpenOption.NOSHARE_DELETE));
        if (!System.getProperty("os.name", "").startsWith("Windows")) return;

        Path output = temporaryDirectory.resolve("windows-bound-output.pdf").toAbsolutePath();
        Path replacement = temporaryDirectory.resolve("windows-replacement.pdf").toAbsolutePath();
        createPrivateOutput(output);
        try (FileChannel bound = FileChannel.open(
            output, UnsignedCopyService.privateOutputOpenOptions(true)
        ); FileLock ignored = bound.lock(0L, Long.MAX_VALUE, false)) {
            assertThrows(IOException.class, () -> Files.move(output, replacement));
            Process competingWriter = new ProcessBuilder(
                Path.of(System.getProperty("java.home"), "bin", "java.exe").toString(),
                "-cp",
                System.getProperty("surefire.test.class.path", System.getProperty("java.class.path")),
                WindowsCompetingWriterProbe.class.getName()
            ).redirectErrorStream(true).start();
            competingWriter.getOutputStream().write((output + System.lineSeparator())
                .getBytes(java.nio.charset.StandardCharsets.UTF_8));
            competingWriter.getOutputStream().close();
            assertTrue(competingWriter.waitFor(10, TimeUnit.SECONDS));
            assertEquals(0, competingWriter.exitValue());
            assertEquals("WRITE_BLOCKED", new String(
                competingWriter.getInputStream().readAllBytes(),
                java.nio.charset.StandardCharsets.UTF_8
            ).trim());
            assertTrue(Files.isRegularFile(output));
            assertFalse(Files.exists(replacement));
        }
        Files.move(output, replacement);
        assertTrue(Files.isRegularFile(replacement));
    }

    private static String sha256(byte[] value) throws Exception {
        return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(value));
    }

    private static BufferedImage renderFirstPage(Path input) throws Exception {
        try (PDDocument document = Loader.loadPDF(input.toFile())) {
            return new PDFRenderer(document).renderImageWithDPI(0, 72);
        }
    }

    private static void assertRenderingDiffConfinedToSignatureRegion(
        BufferedImage signed,
        BufferedImage unsignedCopy
    ) {
        assertEquals(signed.getWidth(), unsignedCopy.getWidth());
        assertEquals(signed.getHeight(), unsignedCopy.getHeight());
        int minX = Math.max(0, Math.round(VISIBLE_SIGNATURE_RECT.getLowerLeftX()) - 2);
        int maxX = Math.min(signed.getWidth() - 1, Math.round(VISIBLE_SIGNATURE_RECT.getUpperRightX()) + 2);
        int minY = Math.max(0, signed.getHeight() - Math.round(VISIBLE_SIGNATURE_RECT.getUpperRightY()) - 2);
        int maxY = Math.min(signed.getHeight() - 1,
            signed.getHeight() - Math.round(VISIBLE_SIGNATURE_RECT.getLowerLeftY()) + 2);
        int insideDifferences = 0;
        int outsideDifferences = 0;
        for (int y = 0; y < signed.getHeight(); y++) {
            for (int x = 0; x < signed.getWidth(); x++) {
                if (signed.getRGB(x, y) == unsignedCopy.getRGB(x, y)) continue;
                if (x >= minX && x <= maxX && y >= minY && y <= maxY) insideDifferences++;
                else outsideDifferences++;
            }
        }
        assertTrue(insideDifferences > 1_000, "visible signature appearance must be removed");
        assertEquals(0, outsideDifferences,
            "unsigned-copy rendering may differ only inside the removed signature widget");
    }

    private void createPrivateOutput(Path output) throws Exception {
        try {
            Files.setPosixFilePermissions(temporaryDirectory, EnumSet.of(
                PosixFilePermission.OWNER_READ,
                PosixFilePermission.OWNER_WRITE,
                PosixFilePermission.OWNER_EXECUTE
            ));
        } catch (UnsupportedOperationException ignored) {
            // Windows test workspace permissions are controlled by its ACL.
        }
        Files.createFile(output);
        try {
            Files.setPosixFilePermissions(output, EnumSet.of(
                PosixFilePermission.OWNER_READ,
                PosixFilePermission.OWNER_WRITE
            ));
        } catch (UnsupportedOperationException ignored) {
            // Windows test workspace permissions are controlled by its ACL.
        }
    }
}
