package com.butterpaper.signaturecore;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.nio.charset.StandardCharsets;
import java.nio.file.FileSystemException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Locale;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

class InspectionServiceTest {
    @TempDir Path temporaryDirectory;

    @Test
    void hashesAndReportsOnlyStructuralIndicators() throws Exception {
        Path input = temporaryDirectory.resolve("sample.pdf");
        Files.writeString(input, "%PDF-1.7\n/FT /Sig\n/Type /Sig\n/ByteRange [0 1 2 3]\n%%EOF\n", StandardCharsets.US_ASCII);

        Map<String, Object> result = new InspectionService().inspect(input.toString());

        assertEquals("3a5ab8432cc07880748f0f3de271c95defe7446265fc53928027fb2ad7178ffb", result.get("inputSha256"));
        assertEquals(Files.size(input), result.get("byteLength"));
        assertEquals(true, result.get("startsWithPdfHeader"));
        assertEquals(true, result.get("eofMarkerPresent"));
        assertEquals(1, result.get("byteRangeMarkerCount"));
        assertEquals(1, result.get("signatureDictionaryMarkerCount"));
        assertEquals(1, result.get("signatureFieldMarkerCount"));
        assertEquals(true, result.get("structuralOnly"));
        assertEquals(false, result.get("validationPerformed"));
    }

    @Test
    void rejectsDirectoriesAndSymlinks() throws Exception {
        InspectionService service = new InspectionService();
        assertEquals("INPUT_NOT_REGULAR_FILE", assertThrows(
            InspectionService.InspectionException.class,
            () -> service.inspect(temporaryDirectory.toString())
        ).code());

        Path target = temporaryDirectory.resolve("target.pdf");
        Files.writeString(target, "%PDF-1.7\n%%EOF\n");
        Path link = temporaryDirectory.resolve("link.pdf");
        boolean symlinkCreated = tryCreateSymlink(link, target.getFileName());
        if (!symlinkCreated) Files.createDirectory(link);
        assertEquals(symlinkCreated ? "INPUT_SYMLINK_REJECTED" : "INPUT_NOT_REGULAR_FILE", assertThrows(
            InspectionService.InspectionException.class,
            () -> service.inspect(link.toString())
        ).code());
    }

    @Test
    void rejectsMissingAndInvalidPathsWithoutEchoingThem() {
        InspectionService service = new InspectionService();
        assertEquals("INVALID_INPUT_PATH", assertThrows(
            InspectionService.InspectionException.class,
            () -> service.inspect("")
        ).code());
        assertEquals("INPUT_UNAVAILABLE", assertThrows(
            InspectionService.InspectionException.class,
            () -> service.inspect(temporaryDirectory.resolve("secret-name.pdf").toString())
        ).code());
    }

    private static boolean tryCreateSymlink(Path link, Path target) throws Exception {
        try {
            Files.createSymbolicLink(link, target);
            return true;
        } catch (FileSystemException error) {
            assertTrue(
                System.getProperty("os.name", "").toLowerCase(Locale.ROOT).startsWith("windows"),
                () -> "Unexpected symlink setup failure: " + error
            );
            return false;
        }
    }
}
