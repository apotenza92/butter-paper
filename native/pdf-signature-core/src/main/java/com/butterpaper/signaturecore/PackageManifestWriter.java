package com.butterpaper.signaturecore;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.MapperFeature;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.SerializationFeature;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashSet;
import java.util.HexFormat;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;

/** Build-time manifest and post-sign inventory helper. Never reachable from the sidecar protocol. */
public final class PackageManifestWriter {
    private static final String MANIFEST_NAME = "manifest.json";
    private static final String POST_SIGN_NAME = "post-sign-inventory.json";
    private static final ObjectMapper JSON = new ObjectMapper()
        .enable(MapperFeature.SORT_PROPERTIES_ALPHABETICALLY)
        .enable(SerializationFeature.ORDER_MAP_ENTRIES_BY_KEYS)
        .enable(SerializationFeature.INDENT_OUTPUT);

    private PackageManifestWriter() {}

    public static void main(String[] args) throws Exception {
        if (args.length == 5 && args[0].equals("manifest")) {
            writeManifest(Path.of(args[1]), args[2], args[3], args[4]);
            return;
        }
        if (args.length == 2 && args[0].equals("post-sign")) {
            writePostSignInventory(Path.of(args[1]));
            return;
        }
        throw new IllegalArgumentException(
            "usage: PackageManifestWriter manifest <root> <platform> <arch> <launcher> | post-sign <root>"
        );
    }

    static void writeManifest(Path packageRoot, String requestedPlatform, String requestedArch, String launcherValue)
        throws Exception {
        Path root = packageRoot.toAbsolutePath().normalize();
        String platform = requireOneOf(requestedPlatform, "darwin", "win32", "linux");
        String arch = requireOneOf(requestedArch, "arm64", "x64");
        Path launcher = safeRelative(launcherValue);
        Path resolvedLauncher = root.resolve(launcher).normalize();
        if (!resolvedLauncher.startsWith(root) || !Files.isRegularFile(resolvedLauncher, LinkOption.NOFOLLOW_LINKS)) {
            throw new IllegalArgumentException("launcher is not a regular file under package root");
        }
        if (!platform.equals("win32") && !Files.isExecutable(resolvedLauncher)) {
            throw new IllegalArgumentException("POSIX launcher is not executable");
        }

        List<Map<String, Object>> immutable = new ArrayList<>();
        List<Map<String, Object>> mutable = new ArrayList<>();
        for (Path file : regularFiles(root)) {
            String relative = relative(root, file);
            if (isInventoryFile(relative)) continue;
            if (isSigningMutable(file, relative)) {
                Map<String, Object> component = new LinkedHashMap<>();
                component.put("path", relative);
                component.put("reason", "platform-signing");
                mutable.add(component);
            } else {
                immutable.add(hashedComponent(file, relative));
            }
        }
        long launcherCount = immutable.stream().filter(component -> launcherValue.equals(component.get("path"))).count()
            + mutable.stream().filter(component -> launcherValue.equals(component.get("path"))).count();
        if (launcherCount != 1) throw new IllegalArgumentException("launcher must occur exactly once in inventory");

        Map<String, Object> manifest = new LinkedHashMap<>();
        manifest.put("schemaVersion", 2);
        manifest.put("protocolVersion", Protocol.VERSION);
        manifest.put("engineVersion", Protocol.ENGINE_VERSION);
        manifest.put("javaVersion", "21.0.12");
        manifest.put("platform", platform);
        manifest.put("arch", arch);
        manifest.put("launcher", launcherValue);
        manifest.put("buildState", "unsigned-build");
        manifest.put("immutableComponents", immutable);
        manifest.put("signingMutableComponents", mutable);
        manifest.put("signingMutablePathRules", List.of("**/_CodeSignature/**", "**/CodeResources"));
        manifest.put("postSignInventory", POST_SIGN_NAME);
        manifest.put("postSignInventoryRequiredForRelease", true);
        manifest.put("postSignTrust", "requires-enclosing-signed-or-tuf-verified-package");
        JSON.writeValue(root.resolve(MANIFEST_NAME).toFile(), manifest);
    }

    static void writePostSignInventory(Path packageRoot) throws Exception {
        Path root = packageRoot.toAbsolutePath().normalize();
        Path manifestFile = root.resolve(MANIFEST_NAME);
        if (!Files.isRegularFile(manifestFile, LinkOption.NOFOLLOW_LINKS)) throw new IllegalArgumentException("manifest is missing");
        JsonNode manifest = JSON.readTree(manifestFile.toFile());
        if (manifest.path("schemaVersion").intValue() != 2) throw new IllegalArgumentException("manifest schema is not 2");
        requireOneOf(requireText(manifest, "platform"), "darwin", "win32", "linux");
        requireOneOf(requireText(manifest, "arch"), "arm64", "x64");
        if (!manifest.path("postSignInventory").asText().equals(POST_SIGN_NAME)) {
            throw new IllegalArgumentException("unexpected post-sign inventory path");
        }

        Set<String> expected = new HashSet<>();
        for (JsonNode component : manifest.path("immutableComponents")) {
            String path = requireText(component, "path");
            expected.add(path);
            Path file = safeChild(root, path);
            if (!Files.isRegularFile(file, LinkOption.NOFOLLOW_LINKS)
                || !sha256(file).equals(requireHash(component, "sha256"))
                || Files.size(file) != component.path("size").longValue()) {
                throw new IllegalArgumentException("immutable component changed after manifest: " + path);
            }
        }
        for (JsonNode component : manifest.path("signingMutableComponents")) expected.add(requireText(component, "path"));

        List<Map<String, Object>> components = new ArrayList<>();
        for (Path file : regularFiles(root)) {
            String path = relative(root, file);
            if (isInventoryFile(path)) continue;
            if (!expected.remove(path) && !matchesSigningMetadataRule(path)) {
                throw new IllegalArgumentException("unexpected post-manifest component: " + path);
            }
            components.add(hashedComponent(file, path));
        }
        if (!expected.isEmpty()) throw new IllegalArgumentException("manifest components disappeared: " + expected);

        Map<String, Object> inventory = new LinkedHashMap<>();
        inventory.put("schemaVersion", 1);
        inventory.put("manifestSha256", sha256(manifestFile));
        inventory.put("platform", requireText(manifest, "platform"));
        inventory.put("arch", requireText(manifest, "arch"));
        inventory.put("evidenceState", "post-nested-signing-unsealed");
        inventory.put("releaseSealed", false);
        inventory.put("components", components);
        inventory.put("trustRequirement", "must-be-covered-by-enclosing-signed-or-tuf-verified-package");
        JSON.writeValue(root.resolve(POST_SIGN_NAME).toFile(), inventory);
    }

    private static List<Path> regularFiles(Path root) throws IOException {
        List<Path> files = new ArrayList<>();
        try (var paths = Files.walk(root)) {
            for (Path file : paths.sorted(Comparator.comparing(Path::toString)).toList()) {
                if (Files.isSymbolicLink(file)) throw new IllegalArgumentException("package contains a symlink");
                if (Files.isDirectory(file, LinkOption.NOFOLLOW_LINKS)) continue;
                if (!Files.isRegularFile(file, LinkOption.NOFOLLOW_LINKS)) {
                    throw new IllegalArgumentException("package contains a non-regular component");
                }
                files.add(file);
            }
        }
        return files;
    }

    private static boolean isSigningMutable(Path file, String relative) throws IOException {
        if (matchesSigningMetadataRule(relative)) return true;
        byte[] header = new byte[4];
        try (InputStream input = Files.newInputStream(file)) {
            if (input.read(header) < 4) return false;
        }
        int magic = ((header[0] & 0xff) << 24) | ((header[1] & 0xff) << 16) | ((header[2] & 0xff) << 8) | (header[3] & 0xff);
        boolean machO = magic == 0xfeedface || magic == 0xfeedfacf || magic == 0xcefaedfe
            || magic == 0xcffaedfe || magic == 0xcafebabe || magic == 0xbebafeca;
        boolean elf = magic == 0x7f454c46;
        boolean portableExecutable = header[0] == 'M' && header[1] == 'Z';
        return machO || elf || portableExecutable;
    }

    private static boolean matchesSigningMetadataRule(String path) {
        return path.contains("/_CodeSignature/") || path.endsWith("/CodeResources");
    }

    private static boolean isInventoryFile(String path) {
        return path.equals(MANIFEST_NAME) || path.equals(POST_SIGN_NAME);
    }

    private static Map<String, Object> hashedComponent(Path file, String relative) throws IOException {
        Map<String, Object> component = new LinkedHashMap<>();
        component.put("path", relative);
        component.put("sha256", sha256(file));
        component.put("size", Files.size(file));
        component.put("executable", Files.isExecutable(file));
        return component;
    }

    private static String relative(Path root, Path file) {
        return root.relativize(file).toString().replace(file.getFileSystem().getSeparator(), "/");
    }

    private static Path safeRelative(String value) {
        if (value.contains("\\")) throw new IllegalArgumentException("path must use POSIX separators");
        Path path = Path.of(value);
        if (path.isAbsolute() || value.isBlank()) throw new IllegalArgumentException("path must be relative");
        for (Path segment : path) if (segment.toString().equals("..")) throw new IllegalArgumentException("path cannot traverse");
        return path;
    }

    private static Path safeChild(Path root, String relative) {
        Path child = root.resolve(safeRelative(relative)).normalize();
        if (!child.startsWith(root)) throw new IllegalArgumentException("path traversal");
        return child;
    }

    private static String requireText(JsonNode node, String field) {
        String value = node.path(field).asText("");
        if (value.isBlank()) throw new IllegalArgumentException("missing " + field);
        return value;
    }

    private static String requireHash(JsonNode node, String field) {
        String value = requireText(node, field).toLowerCase(Locale.ROOT);
        if (!value.matches("[0-9a-f]{64}")) throw new IllegalArgumentException("invalid SHA-256");
        return value;
    }

    private static String requireOneOf(String value, String... allowed) {
        for (String candidate : allowed) if (candidate.equals(value)) return value;
        throw new IllegalArgumentException("unsupported target value");
    }

    private static String sha256(Path file) throws IOException {
        MessageDigest digest;
        try {
            digest = MessageDigest.getInstance("SHA-256");
        } catch (NoSuchAlgorithmException impossible) {
            throw new IllegalStateException("SHA-256 unavailable", impossible);
        }
        try (InputStream input = Files.newInputStream(file)) {
            byte[] buffer = new byte[64 * 1024];
            int read;
            while ((read = input.read(buffer)) != -1) digest.update(buffer, 0, read);
        }
        return HexFormat.of().formatHex(digest.digest());
    }
}
