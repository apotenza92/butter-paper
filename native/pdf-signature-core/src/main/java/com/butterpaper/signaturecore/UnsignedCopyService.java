package com.butterpaper.signaturecore;
import com.sun.nio.file.ExtendedOpenOption;
import org.apache.pdfbox.Loader;
import org.apache.pdfbox.cos.COSArray;
import org.apache.pdfbox.cos.COSBase;
import org.apache.pdfbox.cos.COSDictionary;
import org.apache.pdfbox.cos.COSName;
import org.apache.pdfbox.cos.COSObject;
import org.apache.pdfbox.pdmodel.PDDocument;
import org.apache.pdfbox.pdmodel.PDPage;
import org.apache.pdfbox.pdmodel.interactive.annotation.PDAnnotation;
import org.apache.pdfbox.pdmodel.interactive.annotation.PDAnnotationWidget;
import org.apache.pdfbox.pdmodel.interactive.form.PDAcroForm;
import org.apache.pdfbox.pdmodel.interactive.form.PDSignatureField;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.nio.ByteBuffer;
import java.nio.channels.Channels;
import java.nio.channels.FileChannel;
import java.nio.channels.FileLock;
import java.nio.channels.SeekableByteChannel;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.OpenOption;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.nio.file.attribute.BasicFileAttributes;
import java.nio.file.attribute.PosixFilePermission;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.ArrayDeque;
import java.util.Collections;
import java.util.Deque;
import java.util.EnumSet;
import java.util.HexFormat;
import java.util.HashSet;
import java.util.IdentityHashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
/** Creates a new structurally unsigned PDF without mutating the signed source. */
final class UnsignedCopyService {
    static final String REMOVAL_POLICY_ID = "butter-paper-structurally-unsigned-copy";
    static final int REMOVAL_POLICY_VERSION = 1;
    static final class CopyException extends Exception {
        private final String code;
        CopyException(String code) {
            super(code);
            this.code = code;
        }
        String code() { return code; }
    }
    Map<String, Object> create(String inputPath, String outputPath) throws CopyException {
        InputSnapshot input = readInput(inputPath);
        OutputTarget target = validateOutputTarget(input.path(), outputPath);
        boolean outputWritten = false;
        try {
            CopyMetrics metrics;
            StructuralPostcheck postcheck;
            String outputSha256;
            BasicFileAttributes outputAttributes;
            try (PDDocument document = Loader.loadPDF(input.bytes())) {
                metrics = removeSignatures(document);
                try (BoundOutput bound = openPrivateOutput(target)) {
                    outputWritten = true;
                    document.save(bound.output());
                    bound.force();
                    byte[] outputBytes = readBounded(bound.channel());
                    postcheck = validateUnsignedOutput(outputBytes, metrics);
                    outputSha256 = sha256(outputBytes);
                    ensureSourceUnchanged(input);
                    outputAttributes = verifyBoundOutput(target, bound, outputSha256);
                }
            }
            syncDirectory(target.parent());
            Map<String, Object> result = new LinkedHashMap<>();
            result.put("inputSha256", input.sha256());
            result.put("outputSha256", outputSha256);
            result.put("outputBytes", outputAttributes.size());
            result.put("pageCount", metrics.pageCount());
            result.put("removalPolicyId", REMOVAL_POLICY_ID);
            result.put("removalPolicyVersion", REMOVAL_POLICY_VERSION);
            Map<String, Object> removed = new LinkedHashMap<>();
            removed.put("signatureValues", metrics.removedSignatureValues());
            removed.put("signatureFields", metrics.removedSignatureFields());
            removed.put("signatureWidgets", metrics.removedSignatureWidgets());
            removed.put("certificationReferences", metrics.removedCertificationReferences());
            removed.put("fieldMdpReferences", metrics.removedFieldMdpReferences());
            removed.put("validationEvidenceEntries", metrics.removedValidationEvidenceEntries());
            result.put("removed", removed);
            Map<String, Object> structural = new LinkedHashMap<>();
            structural.put("byteRangeMarkerCount", postcheck.byteRangeMarkerCount());
            structural.put("signatureDictionaryCount", postcheck.signatureDictionaryCount());
            structural.put("signedSignatureFieldCount", postcheck.signedSignatureFieldCount());
            structural.put("docMdpReferenceCount", postcheck.docMdpReferenceCount());
            structural.put("fieldMdpReferenceCount", postcheck.fieldMdpReferenceCount());
            structural.put("dssOrVriEntryCount", postcheck.dssOrVriEntryCount());
            result.put("structuralPostcheck", structural);
            result.put("warnings", List.of(
                "Digital signatures, certification controls, and embedded validation evidence were removed from this new copy."
            ));
            result.put("sourcePreserved", true);
            result.put("validatedUnsigned", true);
            result.put("engineVersion", Protocol.ENGINE_VERSION);
            return result;
        } catch (CopyException exception) {
            if (outputWritten) truncatePrivateOutputIfSame(target);
            throw exception;
        } catch (IOException | RuntimeException exception) {
            if (outputWritten) truncatePrivateOutputIfSame(target);
            throw new CopyException("UNSIGNED_COPY_FAILED");
        }
    }
    Map<String, Object> inspect(String inputPath) throws CopyException {
        InputSnapshot input = readInput(inputPath);
        try (PDDocument document = Loader.loadPDF(input.bytes())) {
            StructuralPostcheck postcheck = structuralPostcheck(document);
            Map<String, Object> result = new LinkedHashMap<>();
            result.put("inputSha256", input.sha256());
            result.put("structurallyReadable", true);
            result.put("byteRangeMarkerCount", postcheck.byteRangeMarkerCount());
            result.put("signatureDictionaryCount", postcheck.signatureDictionaryCount());
            result.put("signedSignatureFieldCount", postcheck.signedSignatureFieldCount());
            result.put("docMdpReferenceCount", postcheck.docMdpReferenceCount());
            result.put("fieldMdpReferenceCount", postcheck.fieldMdpReferenceCount());
            result.put("dssOrVriEntryCount", postcheck.dssOrVriEntryCount());
            return result;
        } catch (IOException | RuntimeException exception) {
            throw new CopyException("MALFORMED_PDF");
        }
    }
    private static CopyMetrics removeSignatures(PDDocument document) throws IOException, CopyException {
        int pageCount = document.getNumberOfPages();
        PDAcroForm form = document.getDocumentCatalog().getAcroForm();
        List<PDSignatureField> signatureFields = new ArrayList<>(document.getSignatureFields());
        Set<COSDictionary> signedFields = Collections.newSetFromMap(new IdentityHashMap<>());
        Set<COSDictionary> signedWidgets = Collections.newSetFromMap(new IdentityHashMap<>());
        int removedFieldMdpReferences = 0;
        for (PDSignatureField field : signatureFields) {
            if (field.getCOSObject().containsKey(COSName.getPDFName("Lock"))) {
                removedFieldMdpReferences++;
                field.getCOSObject().removeItem(COSName.getPDFName("Lock"));
            }
            if (field.getSignature() == null) continue;
            signedFields.add(field.getCOSObject());
            for (PDAnnotationWidget widget : field.getWidgets()) signedWidgets.add(widget.getCOSObject());
        }
        if (signedFields.isEmpty() && document.getSignatureDictionaries().isEmpty()) {
            throw new CopyException("INPUT_NOT_SIGNED");
        }
        Set<COSDictionary> signatureValues = Collections.newSetFromMap(new IdentityHashMap<>());
        signatureValues.addAll(document.getSignatureDictionaries().stream()
            .map(signature -> signature.getCOSObject())
            .toList());
        int removedCertificationReferences = 0;
        for (COSDictionary signatureValue : signatureValues) {
            COSArray references = asArray(signatureValue.getDictionaryObject(COSName.getPDFName("Reference")));
            if (references == null) continue;
            for (COSBase referenceBase : references) {
                COSDictionary reference = asDictionary(referenceBase);
                String method = reference == null ? null
                    : reference.getNameAsString(COSName.getPDFName("TransformMethod"));
                if ("DocMDP".equals(method)) removedCertificationReferences++;
                if ("FieldMDP".equals(method)) removedFieldMdpReferences++;
            }
        }
        int removedWidgets = 0;
        for (PDPage page : document.getPages()) {
            List<PDAnnotation> annotations = page.getAnnotations();
            int before = annotations.size();
            annotations.removeIf(annotation -> signedWidgets.contains(annotation.getCOSObject())
                || signedFields.contains(annotation.getCOSObject()));
            removedWidgets += before - annotations.size();
        }
        if (form != null) {
            COSArray roots = asArray(form.getCOSObject().getDictionaryObject(COSName.FIELDS));
            if (roots != null) pruneFields(roots, signedFields);
            form.getCOSObject().removeItem(COSName.SIG_FLAGS);
        }
        COSDictionary catalog = document.getDocumentCatalog().getCOSObject();
        COSDictionary dss = asDictionary(catalog.getDictionaryObject(COSName.getPDFName("DSS")));
        int removedValidationEvidenceEntries = validationEvidenceEntryCount(dss);
        catalog.removeItem(COSName.getPDFName("DSS"));
        COSDictionary permissions = asDictionary(catalog.getDictionaryObject(COSName.getPDFName("Perms")));
        if (permissions != null && permissions.containsKey(COSName.getPDFName("DocMDP"))) {
            permissions.removeItem(COSName.getPDFName("DocMDP"));
            removedCertificationReferences++;
            if (permissions.keySet().isEmpty()) catalog.removeItem(COSName.getPDFName("Perms"));
        }
        return new CopyMetrics(
            pageCount,
            signatureValues.size(),
            signedFields.size(),
            removedWidgets,
            removedCertificationReferences,
            removedFieldMdpReferences,
            removedValidationEvidenceEntries
        );
    }
    private static void pruneFields(COSArray fields, Set<COSDictionary> signedFields) {
        for (int index = fields.size() - 1; index >= 0; index--) {
            COSDictionary field = asDictionary(fields.get(index));
            if (field == null) continue;
            if (signedFields.contains(field)) {
                field.removeItem(COSName.V);
                field.removeItem(COSName.AP);
                field.removeItem(COSName.getPDFName("Lock"));
                fields.remove(index);
                continue;
            }
            COSArray children = asArray(field.getDictionaryObject(COSName.KIDS));
            if (children != null) pruneFields(children, signedFields);
        }
    }
    private static BoundOutput openPrivateOutput(OutputTarget target) throws IOException, CopyException {
        Set<OpenOption> options = privateOutputOpenOptions(isWindows());
        FileChannel channel = FileChannel.open(target.path(), options);
        try {
            FileLock lock = channel.lock(0L, Long.MAX_VALUE, false);
            verifyPathStillBound(target);
            if (channel.size() != 0) throw new CopyException("OUTPUT_NOT_EMPTY");
            channel.truncate(0);
            channel.position(0);
            verifyPathStillBound(target);
            return new BoundOutput(channel, Channels.newOutputStream(channel), lock);
        } catch (IOException | RuntimeException | CopyException exception) {
            channel.close();
            throw exception;
        }
    }
    static Set<OpenOption> privateOutputOpenOptions(boolean windows) {
        Set<OpenOption> options = new HashSet<>();
        options.add(StandardOpenOption.READ);
        options.add(StandardOpenOption.WRITE);
        options.add(LinkOption.NOFOLLOW_LINKS);
        if (windows) options.add(ExtendedOpenOption.NOSHARE_DELETE);
        return Set.copyOf(options);
    }
    private static BasicFileAttributes verifyBoundOutput(
        OutputTarget target,
        BoundOutput bound,
        String expectedSha256
    ) throws IOException, CopyException {
        verifyPathStillBound(target);
        BasicFileAttributes attributes = Files.readAttributes(
            target.path(), BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
        );
        if (bound.channel().size() != attributes.size()
            || !expectedSha256.equals(sha256(readBounded(bound.channel())))) {
            throw new CopyException("UNSIGNED_COPY_VALIDATION_FAILED");
        }
        return attributes;
    }
    private static void verifyPathStillBound(OutputTarget target) throws IOException, CopyException {
        BasicFileAttributes attributes = Files.readAttributes(
            target.path(), BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
        );
        if (!attributes.isRegularFile() || attributes.isSymbolicLink()) {
            throw new CopyException("OUTPUT_IDENTITY_CHANGED");
        }
        FileIdentity current = readFileIdentity(target.path());
        if (target.identity().available() && !target.identity().sameFile(current)) {
            throw new CopyException("OUTPUT_IDENTITY_CHANGED");
        }
        if (!target.identity().available() && !isWindows()) {
            throw new CopyException("OUTPUT_IDENTITY_UNAVAILABLE");
        }
    }
    private static StructuralPostcheck validateUnsignedOutput(byte[] bytes, CopyMetrics metrics) throws CopyException {
        try {
            try (PDDocument document = Loader.loadPDF(bytes)) {
                StructuralPostcheck postcheck = structuralPostcheck(document);
                if (document.getNumberOfPages() != metrics.pageCount() || !postcheck.allZero()) {
                    throw new CopyException("UNSIGNED_COPY_VALIDATION_FAILED");
                }
                return postcheck;
            }
        } catch (IOException | RuntimeException exception) {
            throw new CopyException("UNSIGNED_COPY_VALIDATION_FAILED");
        }
    }
    private static StructuralPostcheck structuralPostcheck(PDDocument document) throws CopyException {
        Set<COSDictionary> signatureDictionaries = Collections.newSetFromMap(new IdentityHashMap<>());
        Set<COSDictionary> byteRangeDictionaries = Collections.newSetFromMap(new IdentityHashMap<>());
        int docMdpReferences = 0;
        int fieldMdpReferences = 0;
        int dssOrVriEntries = 0;
        for (COSDictionary dictionary : reachableDictionaries(document)) {
            if (dictionary.containsKey(COSName.BYTERANGE)) byteRangeDictionaries.add(dictionary);
            if ("Sig".equals(dictionary.getNameAsString(COSName.TYPE))
                || dictionary.containsKey(COSName.BYTERANGE)) {
                signatureDictionaries.add(dictionary);
            }
            COSArray references = asArray(dictionary.getDictionaryObject(COSName.getPDFName("Reference")));
            if (references != null) {
                for (COSBase referenceBase : references) {
                    COSDictionary reference = asDictionary(referenceBase);
                    String method = reference == null ? null
                        : reference.getNameAsString(COSName.getPDFName("TransformMethod"));
                    if ("DocMDP".equals(method)) docMdpReferences++;
                    if ("FieldMDP".equals(method)) fieldMdpReferences++;
                }
            }
            if (dictionary.containsKey(COSName.getPDFName("VRI"))) dssOrVriEntries++;
        }
        COSDictionary catalog = document.getDocumentCatalog().getCOSObject();
        COSDictionary permissions = asDictionary(catalog.getDictionaryObject(COSName.getPDFName("Perms")));
        if (permissions != null && permissions.containsKey(COSName.getPDFName("DocMDP"))) docMdpReferences++;
        COSDictionary dss = asDictionary(catalog.getDictionaryObject(COSName.getPDFName("DSS")));
        dssOrVriEntries += validationEvidenceEntryCount(dss);
        for (PDSignatureField field : document.getSignatureFields()) {
            if (field.getCOSObject().containsKey(COSName.getPDFName("Lock"))) fieldMdpReferences++;
        }
        return new StructuralPostcheck(
            boundedCount(byteRangeDictionaries.size()),
            boundedCount(signatureDictionaries.size()),
            boundedCount((int) document.getSignatureFields().stream().filter(field -> field.getSignature() != null).count()),
            boundedCount(docMdpReferences),
            boundedCount(fieldMdpReferences),
            boundedCount(dssOrVriEntries)
        );
    }
    private static Set<COSDictionary> reachableDictionaries(PDDocument document) throws CopyException {
        Set<COSBase> visited = Collections.newSetFromMap(new IdentityHashMap<>());
        Set<COSDictionary> dictionaries = Collections.newSetFromMap(new IdentityHashMap<>());
        Deque<COSBase> pending = new ArrayDeque<>();
        pending.add(document.getDocument().getTrailer());
        while (!pending.isEmpty() && visited.size() <= Protocol.MAX_STRUCTURAL_MARKERS) {
            COSBase current = pending.removeFirst();
            if (current == null || !visited.add(current)) continue;
            if (current instanceof COSObject object) {
                COSBase referenced = object.getObject();
                if (referenced != null) pending.addLast(referenced);
            } else if (current instanceof COSDictionary dictionary) {
                dictionaries.add(dictionary);
                for (COSBase value : dictionary.getValues()) {
                    if (value != null) pending.addLast(value);
                }
            } else if (current instanceof COSArray array) {
                for (COSBase value : array) {
                    if (value != null) pending.addLast(value);
                }
            }
        }
        if (!pending.isEmpty()) throw new CopyException("STRUCTURAL_LIMIT_EXCEEDED");
        return dictionaries;
    }
    private static int validationEvidenceEntryCount(COSDictionary dss) {
        if (dss == null) return 0;
        int count = 1;
        for (String name : List.of("Certs", "CRLs", "OCSPs")) {
            COSArray values = asArray(dss.getDictionaryObject(COSName.getPDFName(name)));
            if (values != null) count += values.size();
        }
        COSDictionary vri = asDictionary(dss.getDictionaryObject(COSName.getPDFName("VRI")));
        if (vri != null) count += 1 + vri.size();
        return count;
    }
    private static int boundedCount(int count) {
        return Math.min(count, Protocol.MAX_STRUCTURAL_MARKERS + 1);
    }
    private static InputSnapshot readInput(String inputPath) throws CopyException {
        if (inputPath == null || inputPath.isBlank() || inputPath.length() > Protocol.MAX_PATH_LENGTH) {
            throw new CopyException("INVALID_INPUT_PATH");
        }
        Path path;
        try {
            path = Path.of(inputPath);
        } catch (RuntimeException exception) {
            throw new CopyException("INVALID_INPUT_PATH");
        }
        if (!path.isAbsolute()) throw new CopyException("INVALID_INPUT_PATH");
        BasicFileAttributes attributes;
        try {
            attributes = Files.readAttributes(path, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS);
        } catch (IOException exception) {
            throw new CopyException("INPUT_UNAVAILABLE");
        }
        if (attributes.isSymbolicLink()) throw new CopyException("INPUT_SYMLINK_REJECTED");
        if (!attributes.isRegularFile()) throw new CopyException("INPUT_NOT_REGULAR_FILE");
        if (attributes.size() > Protocol.MAX_INPUT_BYTES) throw new CopyException("INPUT_TOO_LARGE");
        byte[] bytes = readBounded(path, attributes.size());
        return new InputSnapshot(path, bytes, sha256(bytes), attributes.size(), attributes.lastModifiedTime().toMillis());
    }
    private static byte[] readBounded(Path path, long expectedSize) throws CopyException {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream((int) expectedSize);
        try (SeekableByteChannel channel = Files.newByteChannel(
            path, Set.<OpenOption>of(StandardOpenOption.READ, LinkOption.NOFOLLOW_LINKS)
        )) {
            ByteBuffer buffer = ByteBuffer.allocate(64 * 1024);
            long total = 0;
            while (channel.read(buffer) != -1) {
                buffer.flip();
                byte[] chunk = new byte[buffer.remaining()];
                buffer.get(chunk);
                buffer.clear();
                total += chunk.length;
                if (total > Protocol.MAX_INPUT_BYTES) throw new CopyException("INPUT_TOO_LARGE");
                bytes.write(chunk);
            }
        } catch (CopyException exception) {
            throw exception;
        } catch (IOException exception) {
            throw new CopyException("INPUT_READ_FAILED");
        }
        return bytes.toByteArray();
    }
    private static byte[] readBounded(FileChannel channel) throws CopyException {
        try {
            long size = channel.size();
            if (size > Protocol.MAX_INPUT_BYTES) throw new CopyException("UNSIGNED_COPY_VALIDATION_FAILED");
            ByteArrayOutputStream bytes = new ByteArrayOutputStream((int) size);
            ByteBuffer buffer = ByteBuffer.allocate(64 * 1024);
            channel.position(0);
            while (channel.read(buffer) != -1) {
                buffer.flip();
                byte[] chunk = new byte[buffer.remaining()];
                buffer.get(chunk);
                buffer.clear();
                if ((long) bytes.size() + chunk.length > Protocol.MAX_INPUT_BYTES) {
                    throw new CopyException("UNSIGNED_COPY_VALIDATION_FAILED");
                }
                bytes.write(chunk);
            }
            return bytes.toByteArray();
        } catch (CopyException exception) {
            throw exception;
        } catch (IOException exception) {
            throw new CopyException("UNSIGNED_COPY_VALIDATION_FAILED");
        }
    }
    private static OutputTarget validateOutputTarget(Path input, String outputPath) throws CopyException {
        if (outputPath == null || outputPath.isBlank() || outputPath.length() > Protocol.MAX_PATH_LENGTH) {
            throw new CopyException("INVALID_OUTPUT_PATH");
        }
        Path output;
        try {
            output = Path.of(outputPath).normalize();
        } catch (RuntimeException exception) {
            throw new CopyException("INVALID_OUTPUT_PATH");
        }
        if (!output.isAbsolute() || output.equals(input.normalize())) throw new CopyException("INVALID_OUTPUT_PATH");
        if (!Files.exists(output, LinkOption.NOFOLLOW_LINKS)) throw new CopyException("OUTPUT_REQUIRED");
        if (Files.isSymbolicLink(output)) throw new CopyException("OUTPUT_SYMLINK_REJECTED");
        Path parent = output.getParent();
        if (parent == null) throw new CopyException("INVALID_OUTPUT_PATH");
        try {
            Path realParent = parent.toRealPath(LinkOption.NOFOLLOW_LINKS);
            BasicFileAttributes parentAttributes = Files.readAttributes(
                realParent, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
            );
            if (!parentAttributes.isDirectory() || parentAttributes.isSymbolicLink()) {
                throw new CopyException("OUTPUT_PARENT_UNSAFE");
            }
            Path realInput = input.toRealPath(LinkOption.NOFOLLOW_LINKS);
            Path resolvedOutput = realParent.resolve(output.getFileName());
            if (resolvedOutput.equals(realInput)) throw new CopyException("INVALID_OUTPUT_PATH");
            BasicFileAttributes outputAttributes = Files.readAttributes(
                resolvedOutput, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
            );
            if (!outputAttributes.isRegularFile() || outputAttributes.isSymbolicLink()) {
                throw new CopyException("OUTPUT_NOT_REGULAR_FILE");
            }
            if (outputAttributes.size() != 0) throw new CopyException("OUTPUT_NOT_EMPTY");
            assertPrivatePermissions(realParent, resolvedOutput);
            FileIdentity identity = readFileIdentity(resolvedOutput);
            if (identity.linkCount() != null && identity.linkCount() != 1) {
                throw new CopyException("OUTPUT_HARDLINK_REJECTED");
            }
            return new OutputTarget(resolvedOutput, realParent, identity);
        } catch (CopyException exception) {
            throw exception;
        } catch (IOException exception) {
            throw new CopyException("OUTPUT_PARENT_UNSAFE");
        }
    }
    private static void ensureSourceUnchanged(InputSnapshot input) throws CopyException {
        try {
            BasicFileAttributes attributes = Files.readAttributes(
                input.path(), BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
            );
            if (!attributes.isRegularFile() || attributes.isSymbolicLink()
                || attributes.size() != input.size()
                || attributes.lastModifiedTime().toMillis() != input.lastModifiedMillis()
                || !input.sha256().equals(sha256(input.path()))) {
                throw new CopyException("SOURCE_CHANGED");
            }
        } catch (CopyException exception) {
            throw exception;
        } catch (IOException exception) {
            throw new CopyException("SOURCE_CHANGED");
        }
    }
    private static void assertPrivatePermissions(Path parent, Path output) throws IOException, CopyException {
        try {
            Set<PosixFilePermission> expectedFile = EnumSet.of(
                PosixFilePermission.OWNER_READ,
                PosixFilePermission.OWNER_WRITE
            );
            Set<PosixFilePermission> expectedParent = EnumSet.of(
                PosixFilePermission.OWNER_READ,
                PosixFilePermission.OWNER_WRITE,
                PosixFilePermission.OWNER_EXECUTE
            );
            if (!Files.getPosixFilePermissions(output, LinkOption.NOFOLLOW_LINKS).equals(expectedFile)
                || !Files.getPosixFilePermissions(parent, LinkOption.NOFOLLOW_LINKS).equals(expectedParent)) {
                throw new CopyException("OUTPUT_PERMISSIONS_UNSAFE");
            }
        } catch (UnsupportedOperationException ignored) {
            // Windows ACLs are owned by the private main-process temporary directory.
        }
    }
    private static void syncDirectory(Path directory) {
        try (FileChannel channel = FileChannel.open(directory, StandardOpenOption.READ)) {
            channel.force(true);
        } catch (IOException | UnsupportedOperationException ignored) {
            // The completed file itself was forced; directory fsync is not portable.
        }
    }
    private static void truncatePrivateOutputIfSame(OutputTarget target) {
        try {
            if (!target.identity().available()) return;
            if (!target.identity().sameFile(readFileIdentity(target.path()))) return;
            try (FileChannel channel = FileChannel.open(
                target.path(), StandardOpenOption.WRITE, LinkOption.NOFOLLOW_LINKS
            )) {
                channel.truncate(0);
                channel.force(true);
            }
        } catch (IOException | RuntimeException | CopyException ignored) {
            // The main process owns cleanup/quarantine of its private temporary workspace.
        }
    }
    private static FileIdentity readFileIdentity(Path path) throws IOException, CopyException {
        BasicFileAttributes attributes = Files.readAttributes(
            path, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
        );
        Object fileKey = attributes.fileKey();
        Long device = null;
        Long inode = null;
        Integer linkCount = null;
        try {
            Map<String, Object> unix = Files.readAttributes(
                path, "unix:dev,ino,nlink", LinkOption.NOFOLLOW_LINKS
            );
            device = ((Number) unix.get("dev")).longValue();
            inode = ((Number) unix.get("ino")).longValue();
            linkCount = ((Number) unix.get("nlink")).intValue();
        } catch (UnsupportedOperationException ignored) {
            // Windows uses BasicFileAttributes.fileKey for the identity check.
        }
        if ((device == null || inode == null) && fileKey == null && !isWindows()) {
            throw new CopyException("OUTPUT_IDENTITY_UNAVAILABLE");
        }
        return new FileIdentity(fileKey, device, inode, linkCount);
    }
    private static boolean isWindows() {
        return System.getProperty("os.name", "").startsWith("Windows");
    }
    private static String sha256(Path path) throws IOException {
        MessageDigest digest = sha256Digest();
        try (SeekableByteChannel channel = Files.newByteChannel(
            path, Set.<OpenOption>of(StandardOpenOption.READ, LinkOption.NOFOLLOW_LINKS)
        )) {
            ByteBuffer buffer = ByteBuffer.allocate(64 * 1024);
            while (channel.read(buffer) != -1) {
                buffer.flip();
                digest.update(buffer);
                buffer.clear();
            }
        }
        return HexFormat.of().formatHex(digest.digest());
    }
    private static String sha256(byte[] bytes) {
        return HexFormat.of().formatHex(sha256Digest().digest(bytes));
    }
    private static MessageDigest sha256Digest() {
        try {
            return MessageDigest.getInstance("SHA-256");
        } catch (NoSuchAlgorithmException impossible) {
            throw new IllegalStateException("SHA-256 unavailable", impossible);
        }
    }
    private static COSBase dereference(COSBase value) {
        while (value instanceof COSObject object) value = object.getObject();
        return value;
    }
    private static COSArray asArray(COSBase value) {
        COSBase direct = dereference(value);
        return direct instanceof COSArray array ? array : null;
    }
    private static COSDictionary asDictionary(COSBase value) {
        COSBase direct = dereference(value);
        return direct instanceof COSDictionary dictionary ? dictionary : null;
    }
    private record InputSnapshot(Path path, byte[] bytes, String sha256, long size, long lastModifiedMillis) {}
    private record OutputTarget(Path path, Path parent, FileIdentity identity) {}
    private record FileIdentity(Object fileKey, Long device, Long inode, Integer linkCount) {
        boolean available() {
            return (device != null && inode != null) || fileKey != null;
        }
        boolean sameFile(FileIdentity other) {
            if (device != null && inode != null && other.device != null && other.inode != null) {
                return device.equals(other.device) && inode.equals(other.inode);
            }
            return fileKey != null && fileKey.equals(other.fileKey);
        }
    }
    private record BoundOutput(
        FileChannel channel,
        OutputStream output,
        FileLock lock
    ) implements AutoCloseable {
        void force() throws IOException {
            channel.force(true);
        }
        @Override
        public void close() throws IOException {
            IOException failure = null;
            try {
                lock.release();
            } catch (IOException exception) {
                failure = exception;
            }
            try {
                output.close();
            } catch (IOException exception) {
                if (failure == null) failure = exception;
                else failure.addSuppressed(exception);
            }
            if (failure != null) throw failure;
        }
    }
    private record CopyMetrics(
        int pageCount,
        int removedSignatureValues,
        int removedSignatureFields,
        int removedSignatureWidgets,
        int removedCertificationReferences,
        int removedFieldMdpReferences,
        int removedValidationEvidenceEntries
    ) {}
    private record StructuralPostcheck(
        int byteRangeMarkerCount,
        int signatureDictionaryCount,
        int signedSignatureFieldCount,
        int docMdpReferenceCount,
        int fieldMdpReferenceCount,
        int dssOrVriEntryCount
    ) {
        boolean allZero() {
            return byteRangeMarkerCount == 0
                && signatureDictionaryCount == 0
                && signedSignatureFieldCount == 0
                && docMdpReferenceCount == 0
                && fieldMdpReferenceCount == 0
                && dssOrVriEntryCount == 0;
        }
    }
}
