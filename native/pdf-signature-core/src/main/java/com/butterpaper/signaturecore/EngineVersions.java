package com.butterpaper.signaturecore;
import com.fasterxml.jackson.databind.ObjectMapper;
import eu.europa.esig.dss.pades.signature.PAdESService;
import org.apache.pdfbox.pdmodel.PDDocument;
import java.util.LinkedHashMap;
import java.util.Map;
final class EngineVersions {
    private EngineVersions() {}
    static Map<String, Object> describe() {
        Map<String, Object> versions = new LinkedHashMap<>();
        versions.put("engine", Protocol.ENGINE_VERSION);
        versions.put("protocol", Protocol.VERSION);
        versions.put("java", Runtime.version().toString());
        versions.put("javaFeature", Runtime.version().feature());
        versions.put("dss", implementationVersion(PAdESService.class, "6.4"));
        versions.put("pdfBox", implementationVersion(PDDocument.class, "3.0.6"));
        versions.put("jackson", implementationVersion(ObjectMapper.class, "2.21.5"));
        return versions;
    }
    private static String implementationVersion(Class<?> type, String pinnedFallback) {
        String implementationVersion = type.getPackage().getImplementationVersion();
        return implementationVersion == null ? pinnedFallback : implementationVersion;
    }
}
