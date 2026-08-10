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
import java.nio.file.StandardCopyOption;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HexFormat;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.TreeSet;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;

/** Build-time dependency licence reconciliation. Not reachable from the sidecar protocol. */
public final class LicenseEvidenceVerifier {
    private static final String DSS_COORDINATE_PREFIX = "eu.europa.ec.joinup.sd-dss:";
    private static final String REVIEWED_DSS_SOURCE_URL = "https://github.com/esig/dss/archive/refs/tags/6.4.tar.gz";
    private static final String REVIEWED_DSS_SOURCE_COMMIT = "26a2e3338d8d4fe6c6281c2b53d13546fa64c9bf";
    private static final String REVIEWED_DSS_SOURCE_SHA256 = "5f2421d6bf1c6073aa1e3c1ed4b44d2f058c6d751a4d89dbf326082860b224a4";
    private static final long REVIEWED_DSS_SOURCE_BYTES = 137_227_450L;
    private static final ObjectMapper JSON = new ObjectMapper()
        .enable(MapperFeature.SORT_PROPERTIES_ALPHABETICALLY)
        .enable(SerializationFeature.ORDER_MAP_ENTRIES_BY_KEYS)
        .enable(SerializationFeature.INDENT_OUTPUT);

    private LicenseEvidenceVerifier() {}

    public static void main(String[] args) throws Exception {
        if (args.length != 7) {
            throw new IllegalArgumentException(
                "usage: LicenseEvidenceVerifier <sbom> <policy> <lib-dir> <licence-dir> <source-dir> <notice-dir> <report>"
            );
        }
        verifyProductionSourcePolicy(Path.of(args[1]));
        verify(
            Path.of(args[0]), Path.of(args[1]), Path.of(args[2]), Path.of(args[3]),
            Path.of(args[4]), Path.of(args[5]), Path.of(args[6])
        );
    }

    static void verifyProductionSourcePolicy(Path policyFile) throws IOException {
        JsonNode policy = JSON.readTree(policyFile.toFile());
        JsonNode sources = policy.path("correspondingSources");
        if (!sources.isArray() || sources.size() != 1) {
            throw new IllegalArgumentException("production DSS corresponding-source policy must contain one entry");
        }
        JsonNode source = sources.get(0);
        if (!source.path("version").asText("").equals("6.4")
            || !source.path("archiveRoot").asText("").equals("dss-6.4/")
            || !source.path("file").asText("").equals("dss-6.4-source.tar.gz")
            || !source.path("packagePath").asText("").equals("source/upstream/dss-6.4-source.tar.gz")
            || !source.path("requiredCoordinatePrefix").asText("").equals(DSS_COORDINATE_PREFIX)
            || !source.path("sourceUrl").asText("").equals(REVIEWED_DSS_SOURCE_URL)
            || !source.path("resolvedCommit").asText("").equals(REVIEWED_DSS_SOURCE_COMMIT)
            || !source.path("sha256").asText("").equals(REVIEWED_DSS_SOURCE_SHA256)
            || source.path("bytes").longValue() != REVIEWED_DSS_SOURCE_BYTES) {
            throw new IllegalArgumentException("production DSS corresponding-source identity differs from the reviewed pin");
        }
    }

    static void verify(Path sbomFile, Path policyFile, Path libraryDirectory, Path licenceDirectory,
                       Path sourceDirectory, Path noticeDirectory, Path reportFile) throws Exception {
        JsonNode sbom = JSON.readTree(sbomFile.toFile());
        JsonNode policy = JSON.readTree(policyFile.toFile());
        if (policy.path("schemaVersion").intValue() != 1) throw new IllegalArgumentException("unsupported licence policy schema");

        Map<String, JsonNode> licences = new HashMap<>();
        policy.path("licenses").fields().forEachRemaining(entry -> licences.put(entry.getKey(), entry.getValue()));
        for (Map.Entry<String, JsonNode> entry : licences.entrySet()) {
            JsonNode evidence = entry.getValue();
            requireStringSet(evidence, "acceptedSbomLicenses");
            requireText(evidence, "file");
            String sourceUrl = requireText(evidence, "sourceUrl");
            if (!sourceUrl.startsWith("https://")) throw new IllegalArgumentException("licence source must use HTTPS");
            String expectedHash = requireHash(evidence, "sha256");
            Path evidenceFile = safeChild(licenceDirectory, requireText(evidence, "file"));
            if (!Files.isRegularFile(evidenceFile, LinkOption.NOFOLLOW_LINKS)) {
                throw new IllegalArgumentException("missing licence evidence " + entry.getKey());
            }
            if (!expectedHash.equals(sha256(evidenceFile))) throw new IllegalArgumentException("licence evidence hash mismatch " + entry.getKey());
        }

        Map<String, JsonNode> sbomComponents = new HashMap<>();
        for (JsonNode component : sbom.path("components")) {
            String coordinate = requireText(component, "group") + ':' + requireText(component, "name") + ':' + requireText(component, "version");
            if (!component.path("licenses").isArray() || component.path("licenses").isEmpty()) {
                throw new IllegalArgumentException("SBOM component has no declared licence: " + coordinate);
            }
            if (sbomComponents.put(coordinate, component) != null) throw new IllegalArgumentException("duplicate SBOM component " + coordinate);
        }

        Set<String> requiredDssSources = new TreeSet<>();
        for (String coordinate : sbomComponents.keySet()) {
            if (coordinate.startsWith(DSS_COORDINATE_PREFIX)) requiredDssSources.add(coordinate);
        }
        if (requiredDssSources.isEmpty()) throw new IllegalArgumentException("SBOM contains no DSS components");

        JsonNode sourcePolicy = policy.path("correspondingSources");
        if (!sourcePolicy.isArray() || sourcePolicy.isEmpty()) {
            throw new IllegalArgumentException("DSS corresponding-source evidence is missing");
        }
        Set<String> coveredDssSources = new TreeSet<>();
        Set<String> sourceEvidenceFiles = new TreeSet<>();
        List<Map<String, Object>> reportSources = new ArrayList<>();
        for (JsonNode source : sourcePolicy) {
            String prefix = requireText(source, "requiredCoordinatePrefix");
            if (!prefix.equals(DSS_COORDINATE_PREFIX)) {
                throw new IllegalArgumentException("unsupported corresponding-source coordinate prefix");
            }
            String version = requireText(source, "version");
            String fileName = requireText(source, "file");
            if (!sourceEvidenceFiles.add(fileName)) {
                throw new IllegalArgumentException("duplicate corresponding-source evidence file");
            }
            String sourceUrl = requireText(source, "sourceUrl");
            if (!sourceUrl.startsWith("https://github.com/esig/dss/archive/refs/tags/")) {
                throw new IllegalArgumentException("DSS corresponding source must use the authoritative upstream tag archive");
            }
            String resolvedCommit = requireText(source, "resolvedCommit").toLowerCase(Locale.ROOT);
            if (!resolvedCommit.matches("[0-9a-f]{40}")) {
                throw new IllegalArgumentException("invalid DSS source commit");
            }
            String archiveRoot = requireText(source, "archiveRoot");
            if (!archiveRoot.equals("dss-" + version + "/")) {
                throw new IllegalArgumentException("DSS source archive root does not match its version");
            }
            String packagePath = safePortablePath(requireText(source, "packagePath"));
            if (!packagePath.equals("source/upstream/" + fileName)) {
                throw new IllegalArgumentException("DSS corresponding source package path is not canonical");
            }
            long expectedBytes = requirePositiveLong(source, "bytes");
            String expectedHash = requireHash(source, "sha256");
            Path sourceFile = safeChild(sourceDirectory, fileName);
            if (!Files.isRegularFile(sourceFile, LinkOption.NOFOLLOW_LINKS)) {
                throw new IllegalArgumentException("missing DSS corresponding-source archive");
            }
            if (Files.size(sourceFile) != expectedBytes || !sha256(sourceFile).equals(expectedHash)) {
                throw new IllegalArgumentException("DSS corresponding-source archive differs from pinned evidence");
            }

            List<String> coveredComponents = new ArrayList<>();
            for (String coordinate : requiredDssSources) {
                if (coordinate.startsWith(prefix) && coordinate.endsWith(':' + version)) {
                    coveredComponents.add(coordinate);
                    coveredDssSources.add(coordinate);
                }
            }
            if (coveredComponents.isEmpty()) {
                throw new IllegalArgumentException("DSS corresponding-source evidence covers no resolved component");
            }

            Map<String, Object> reportSource = new LinkedHashMap<>();
            reportSource.put("archiveRoot", archiveRoot);
            reportSource.put("bytes", expectedBytes);
            reportSource.put("coveredComponents", coveredComponents);
            reportSource.put("evidenceFile", packagePath);
            reportSource.put("evidenceSha256", expectedHash);
            reportSource.put("resolvedCommit", resolvedCommit);
            reportSource.put("sourceUrl", sourceUrl);
            reportSource.put("version", version);
            reportSources.add(reportSource);
        }
        if (!coveredDssSources.equals(requiredDssSources)) {
            Set<String> missing = new TreeSet<>(requiredDssSources);
            missing.removeAll(coveredDssSources);
            throw new IllegalArgumentException("DSS components without corresponding-source evidence: " + missing);
        }

        Set<String> mapped = new TreeSet<>();
        List<Map<String, Object>> reportComponents = new ArrayList<>();
        for (JsonNode mapping : policy.path("components")) {
            String coordinate = requireText(mapping, "coordinate");
            if (!mapped.add(coordinate)) throw new IllegalArgumentException("duplicate licence mapping " + coordinate);
            JsonNode component = sbomComponents.get(coordinate);
            if (component == null) throw new IllegalArgumentException("mapping has no SBOM component " + coordinate);
            String licenceId = requireText(mapping, "license");
            JsonNode evidence = licences.get(licenceId);
            if (evidence == null) throw new IllegalArgumentException("unknown licence evidence " + licenceId);
            String jarName = requireText(mapping, "jar");
            Path jar = safeChild(libraryDirectory, jarName);
            if (!Files.isRegularFile(jar, LinkOption.NOFOLLOW_LINKS)) {
                throw new IllegalArgumentException("missing dependency jar " + jarName);
            }
            String jarHash = sha256(jar);
            if (!jarHash.equals(sbomSha256(component))) throw new IllegalArgumentException("jar differs from SBOM " + coordinate);

            List<Map<String, String>> retainedNotices = extractNotices(jar, noticeDirectory.resolve(jarName));
            List<String> declared = new ArrayList<>();
            for (JsonNode licence : component.path("licenses")) {
                JsonNode value = licence.path("license");
                String name = value.path("id").asText(value.path("name").asText(""));
                if (!name.isBlank()) declared.add(name);
            }
            if (declared.isEmpty()) throw new IllegalArgumentException("SBOM licence names are empty " + coordinate);
            Set<String> acceptedSbomLicences = requireStringSet(evidence, "acceptedSbomLicenses");
            if (!acceptedSbomLicences.containsAll(declared)) {
                throw new IllegalArgumentException("SBOM licence declaration differs from reviewed evidence " + coordinate);
            }

            Map<String, Object> reportComponent = new LinkedHashMap<>();
            reportComponent.put("coordinate", coordinate);
            reportComponent.put("declaredSbomLicences", declared);
            reportComponent.put("acceptedSbomLicences", acceptedSbomLicences);
            reportComponent.put("evidenceFile", "licenses/" + requireText(evidence, "file"));
            reportComponent.put("evidenceSha256", requireHash(evidence, "sha256"));
            reportComponent.put("evidenceSourceUrl", requireText(evidence, "sourceUrl"));
            reportComponent.put("jar", jarName);
            reportComponent.put("jarSha256", jarHash);
            reportComponent.put("retainedJarNotices", retainedNotices);
            reportComponents.add(reportComponent);
        }
        if (!mapped.equals(sbomComponents.keySet())) {
            Set<String> missing = new TreeSet<>(sbomComponents.keySet());
            missing.removeAll(mapped);
            throw new IllegalArgumentException("SBOM components without licence evidence: " + missing);
        }

        Map<String, Object> report = new LinkedHashMap<>();
        report.put("schemaVersion", 1);
        report.put("componentCount", reportComponents.size());
        report.put("allComponentsHaveDeclaredAndHashedEvidence", true);
        report.put("correspondingSourceCount", reportSources.size());
        report.put("allRequiredCorrespondingSourcesPresentAndHashed", true);
        report.put("correspondingSources", reportSources);
        report.put("legalApproval", false);
        report.put("components", reportComponents);
        Files.createDirectories(reportFile.toAbsolutePath().normalize().getParent());
        String serialized = JSON.writeValueAsString(report).replace("\r\n", "\n");
        Files.writeString(reportFile, serialized, StandardCharsets.UTF_8);
    }

    private static List<Map<String, String>> extractNotices(Path jar, Path outputDirectory) throws IOException {
        List<Map<String, String>> retained = new ArrayList<>();
        Set<String> retainedNames = new TreeSet<>();
        try (ZipFile zip = new ZipFile(jar.toFile())) {
            for (ZipEntry entry : java.util.Collections.list(zip.entries())) {
                if (entry.isDirectory()) continue;
                String upper = entry.getName().toUpperCase(Locale.ROOT);
                if (!upper.startsWith("META-INF/LICENSE") && !upper.startsWith("META-INF/NOTICE")
                    && !upper.startsWith("META-INF/DEPENDENCIES")) continue;
                String safeName = entry.getName().replace('/', '_').replace('\\', '_');
                if (!retainedNames.add(safeName)) {
                    throw new IllegalArgumentException("dependency jar contains colliding retained notice names");
                }
                Path output = safeChild(outputDirectory, safeName);
                Files.createDirectories(output.getParent());
                try (InputStream input = zip.getInputStream(entry)) {
                    Files.copy(input, output, StandardCopyOption.REPLACE_EXISTING);
                }
                Map<String, String> notice = new LinkedHashMap<>();
                notice.put("path", jar.getFileName() + "/" + safeName);
                notice.put("sha256", sha256(output));
                retained.add(notice);
            }
        }
        return retained;
    }

    private static Path safeChild(Path root, String relative) {
        safePortablePath(relative);
        Path normalizedRoot = root.toAbsolutePath().normalize();
        Path child = normalizedRoot.resolve(relative).normalize();
        if (!child.startsWith(normalizedRoot)) throw new IllegalArgumentException("evidence path traversal");
        rejectSymlinkParents(normalizedRoot, child);
        return child;
    }

    private static void rejectSymlinkParents(Path root, Path child) {
        Path current = root;
        if (Files.isSymbolicLink(current)) throw new IllegalArgumentException("evidence root must not be a symlink");
        Path relative = root.relativize(child);
        for (int index = 0; index < Math.max(0, relative.getNameCount() - 1); index++) {
            current = current.resolve(relative.getName(index));
            if (Files.isSymbolicLink(current)) {
                throw new IllegalArgumentException("evidence path contains a symlink parent");
            }
        }
    }

    private static String safePortablePath(String value) {
        if (value.isBlank() || value.contains("\\") || value.startsWith("/") || value.endsWith("/")) {
            throw new IllegalArgumentException("unsafe evidence path");
        }
        for (String segment : value.split("/")) {
            if (segment.isBlank() || segment.equals(".") || segment.equals("..")) {
                throw new IllegalArgumentException("unsafe evidence path");
            }
        }
        return value;
    }

    private static long requirePositiveLong(JsonNode node, String field) {
        if (!node.path(field).canConvertToLong() || node.path(field).longValue() <= 0) {
            throw new IllegalArgumentException("invalid " + field);
        }
        return node.path(field).longValue();
    }

    private static Set<String> requireStringSet(JsonNode node, String field) {
        JsonNode value = node.path(field);
        if (!value.isArray() || value.isEmpty()) throw new IllegalArgumentException("missing " + field);
        Set<String> result = new TreeSet<>();
        for (JsonNode item : value) {
            if (!item.isTextual() || item.textValue().isBlank() || !result.add(item.textValue())) {
                throw new IllegalArgumentException("invalid " + field);
            }
        }
        return result;
    }

    private static String sbomSha256(JsonNode component) {
        for (JsonNode hash : component.path("hashes")) {
            if (hash.path("alg").asText().equals("SHA-256")) return requireHash(hash, "content");
        }
        throw new IllegalArgumentException("SBOM component lacks SHA-256");
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
