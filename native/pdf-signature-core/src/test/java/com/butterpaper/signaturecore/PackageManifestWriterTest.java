package com.butterpaper.signaturecore;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.nio.file.FileSystemException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Locale;

import static org.junit.jupiter.api.Assertions.*;

class PackageManifestWriterTest {
    @TempDir Path temporaryDirectory;
    private final ObjectMapper json = new ObjectMapper();

    @Test
    void separatesSigningMutableFilesAndBindsPostSignInventory() throws Exception {
        Path launcher = temporaryDirectory.resolve("pdf-signature-core.exe");
        Files.write(launcher, new byte[]{'M', 'Z', 0, 0, 1});
        Files.createDirectories(temporaryDirectory.resolve("notices"));
        Files.writeString(temporaryDirectory.resolve("notices/NOTICE.txt"), "notice");

        PackageManifestWriter.writeManifest(temporaryDirectory, "win32", "x64", "pdf-signature-core.exe");
        JsonNode manifest = json.readTree(temporaryDirectory.resolve("manifest.json").toFile());
        assertEquals(2, manifest.get("schemaVersion").intValue());
        assertEquals("unsigned-build", manifest.get("buildState").textValue());
        assertEquals(1, manifest.get("immutableComponents").size());
        assertEquals("notices/NOTICE.txt", manifest.at("/immutableComponents/0/path").textValue());
        assertEquals("pdf-signature-core.exe", manifest.at("/signingMutableComponents/0/path").textValue());
        assertFalse(manifest.at("/signingMutableComponents/0").has("sha256"));

        Files.write(launcher, new byte[]{'M', 'Z', 0, 0, 2, 3, 4});
        Path signatureMetadata = temporaryDirectory.resolve("app/_CodeSignature/CodeResources");
        Files.createDirectories(signatureMetadata.getParent());
        Files.writeString(signatureMetadata, "signature metadata");
        PackageManifestWriter.writePostSignInventory(temporaryDirectory);

        JsonNode inventory = json.readTree(temporaryDirectory.resolve("post-sign-inventory.json").toFile());
        assertEquals("post-nested-signing-unsealed", inventory.get("evidenceState").textValue());
        assertFalse(inventory.get("releaseSealed").booleanValue());
        assertEquals(3, inventory.get("components").size());
        assertEquals(64, inventory.get("manifestSha256").textValue().length());
        assertFalse(inventory.toString().contains("post-sign-inventory.json"));
        assertFalse(inventory.toString().contains("manifest.json"));
    }

    @Test
    void postSignFailsIfImmutableDataChangesOrUnexpectedFilesAppear() throws Exception {
        Path launcher = temporaryDirectory.resolve("pdf-signature-core.exe");
        Files.write(launcher, new byte[]{'M', 'Z', 0, 0});
        Path notice = temporaryDirectory.resolve("NOTICE.txt");
        Files.writeString(notice, "original");
        PackageManifestWriter.writeManifest(temporaryDirectory, "win32", "x64", "pdf-signature-core.exe");

        Files.writeString(notice, "tampered");
        assertThrows(IllegalArgumentException.class, () -> PackageManifestWriter.writePostSignInventory(temporaryDirectory));
        Files.writeString(notice, "original");
        Files.writeString(temporaryDirectory.resolve("unexpected.txt"), "unexpected");
        assertThrows(IllegalArgumentException.class, () -> PackageManifestWriter.writePostSignInventory(temporaryDirectory));
    }

    @Test
    void rejectsTraversalSymlinksAndNonRegularInventoryInputs() throws Exception {
        Path launcher = temporaryDirectory.resolve("pdf-signature-core.exe");
        Files.write(launcher, new byte[]{'M', 'Z', 0, 0});
        assertThrows(IllegalArgumentException.class, () -> PackageManifestWriter.writeManifest(
            temporaryDirectory, "win32", "x64", "../pdf-signature-core.exe"
        ));
        Path nonRegular = temporaryDirectory.resolve("linked");
        boolean symlinkCreated = tryCreateSymlink(nonRegular, launcher.getFileName());
        if (!symlinkCreated) Files.createDirectory(nonRegular);
        assertThrows(IllegalArgumentException.class, () -> PackageManifestWriter.writeManifest(
            temporaryDirectory, "win32", "x64", symlinkCreated ? "pdf-signature-core.exe" : "linked"
        ));
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
