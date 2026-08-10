package com.butterpaper.signaturecore;

import com.sun.nio.file.ExtendedOpenOption;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.channels.FileLock;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.OpenOption;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.nio.file.attribute.BasicFileAttributes;
import java.nio.file.attribute.PosixFilePermission;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.Arrays;
import java.util.HashSet;
import java.util.HexFormat;
import java.util.Set;

/**
 * Binds a signature mutation to an immutable source snapshot and a main-owned,
 * already-created private output inode. The signed source is never opened for
 * writing and a failed operation only truncates the same validated output.
 */
final class SafePdfMutation {
    static final long MAX_SIGNING_INPUT_BYTES = 128L * 1024L * 1024L;
    static final long MAX_SIGNING_OUTPUT_BYTES = 192L * 1024L * 1024L;

    static final class MutationException extends Exception {
        private final String code;

        MutationException(String code) {
            super(code);
            this.code = code;
        }

        String code() { return code; }
    }

    record Input(Path path, byte[] bytes, String sha256, BasicFileAttributes identity) {}
    record Output(Path path, Path parent, BasicFileAttributes identity) {}

    @FunctionalInterface
    interface ChannelWriter {
        int write(FileChannel channel, ByteBuffer buffer) throws IOException;
    }

    private final ChannelWriter channelWriter;

    SafePdfMutation() {
        this((channel, buffer) -> channel.write(buffer));
    }

    SafePdfMutation(ChannelWriter channelWriter) {
        this.channelWriter = java.util.Objects.requireNonNull(channelWriter);
    }

    Input readInput(String encodedPath, String expectedSha256) throws MutationException {
        Path path = safeAbsolutePath(encodedPath, "INVALID_INPUT_PATH");
        if (!isSha256(expectedSha256)) throw new MutationException("INVALID_INPUT_HASH");
        try {
            BasicFileAttributes before = Files.readAttributes(
                path, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
            );
            if (!before.isRegularFile() || before.isSymbolicLink()
                || before.size() < 8 || before.size() > MAX_SIGNING_INPUT_BYTES) {
                throw new MutationException("UNSAFE_INPUT");
            }
            byte[] bytes = Files.readAllBytes(path);
            BasicFileAttributes after = Files.readAttributes(
                path, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
            );
            if (!sameIdentity(before, after) || bytes.length != before.size()) {
                Arrays.fill(bytes, (byte) 0);
                throw new MutationException("INPUT_CHANGED");
            }
            String actual = sha256(bytes);
            if (!actual.equals(expectedSha256)) {
                Arrays.fill(bytes, (byte) 0);
                throw new MutationException("INPUT_CHANGED");
            }
            return new Input(path, bytes, actual, before);
        } catch (MutationException exception) {
            throw exception;
        } catch (IOException | RuntimeException exception) {
            throw new MutationException("UNSAFE_INPUT");
        }
    }

    Input readPostvalidationOutput(Input source, String encodedPath, String expectedSha256)
        throws MutationException {
        Path path = safeAbsolutePath(encodedPath, "INVALID_OUTPUT_PATH");
        if (path.equals(source.path())) throw new MutationException("OUTPUT_EQUALS_INPUT");
        if (!isSha256(expectedSha256)) throw new MutationException("INVALID_OUTPUT_HASH");
        try {
            BasicFileAttributes before = Files.readAttributes(
                path, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
            );
            if (!before.isRegularFile() || before.isSymbolicLink()
                || before.size() < 8 || before.size() > MAX_SIGNING_OUTPUT_BYTES) {
                throw new MutationException("UNSAFE_OUTPUT");
            }
            try {
                Object links = Files.getAttribute(path, "unix:nlink", LinkOption.NOFOLLOW_LINKS);
                if (links instanceof Number count && count.longValue() != 1L) {
                    throw new MutationException("UNSAFE_OUTPUT");
                }
            } catch (UnsupportedOperationException ignored) {
                // Windows link/ACL checks are independently enforced by Electron main.
            }
            assertPrivatePermissions(path);
            Path parent = path.getParent();
            if (parent == null) throw new MutationException("UNSAFE_OUTPUT");
            BasicFileAttributes parentAttributes = Files.readAttributes(
                parent, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
            );
            if (!parentAttributes.isDirectory() || parentAttributes.isSymbolicLink()) {
                throw new MutationException("UNSAFE_OUTPUT");
            }
            assertPrivateDirectory(parent);
            byte[] bytes = Files.readAllBytes(path);
            BasicFileAttributes after = Files.readAttributes(
                path, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
            );
            if (!sameIdentity(before, after) || bytes.length != before.size()
                || !sha256(bytes).equals(expectedSha256)) {
                Arrays.fill(bytes, (byte) 0);
                throw new MutationException("OUTPUT_CHANGED");
            }
            return new Input(path, bytes, expectedSha256, before);
        } catch (MutationException exception) {
            throw exception;
        } catch (IOException | RuntimeException exception) {
            throw new MutationException("UNSAFE_OUTPUT");
        }
    }

    Output validateOutput(Input input, String encodedPath) throws MutationException {
        Path path = safeAbsolutePath(encodedPath, "INVALID_OUTPUT_PATH");
        if (path.equals(input.path())) throw new MutationException("OUTPUT_EQUALS_INPUT");
        try {
            BasicFileAttributes attributes = Files.readAttributes(
                path, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
            );
            if (!attributes.isRegularFile() || attributes.isSymbolicLink() || attributes.size() != 0) {
                throw new MutationException("UNSAFE_OUTPUT");
            }
            Object links = Files.getAttribute(path, "unix:nlink", LinkOption.NOFOLLOW_LINKS);
            if (links instanceof Number count && count.longValue() != 1L) {
                throw new MutationException("UNSAFE_OUTPUT");
            }
            assertPrivatePermissions(path);
            Path parent = path.getParent();
            if (parent == null) throw new MutationException("UNSAFE_OUTPUT");
            BasicFileAttributes parentAttributes = Files.readAttributes(
                parent, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
            );
            if (!parentAttributes.isDirectory() || parentAttributes.isSymbolicLink()) {
                throw new MutationException("UNSAFE_OUTPUT");
            }
            assertPrivateDirectory(parent);
            return new Output(path, parent, attributes);
        } catch (UnsupportedOperationException ignored) {
            try {
                BasicFileAttributes attributes = Files.readAttributes(
                    path, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
                );
                Path parent = path.getParent();
                if (!attributes.isRegularFile() || attributes.isSymbolicLink()
                    || attributes.size() != 0 || parent == null) {
                    throw new MutationException("UNSAFE_OUTPUT");
                }
                BasicFileAttributes parentAttributes = Files.readAttributes(
                    parent, BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
                );
                if (!parentAttributes.isDirectory() || parentAttributes.isSymbolicLink()) {
                    throw new MutationException("UNSAFE_OUTPUT");
                }
                return new Output(path, parent, attributes);
            } catch (MutationException exception) {
                throw exception;
            } catch (IOException exception) {
                throw new MutationException("UNSAFE_OUTPUT");
            }
        } catch (MutationException exception) {
            throw exception;
        } catch (IOException | RuntimeException exception) {
            throw new MutationException("UNSAFE_OUTPUT");
        }
    }

    void write(Output target, Input input, byte[] signedBytes) throws MutationException {
        if (signedBytes.length > MAX_SIGNING_OUTPUT_BYTES) throw new MutationException("OUTPUT_TOO_LARGE");
        assertAppendOnly(input.bytes(), signedBytes);
        boolean touched = false;
        try (FileChannel channel = FileChannel.open(target.path(), outputOptions());
             FileLock ignored = channel.lock(0L, Long.MAX_VALUE, false)) {
            verifyOutputIdentity(target);
            if (channel.size() != 0) throw new MutationException("OUTPUT_NOT_EMPTY");
            touched = true;
            ByteBuffer buffer = ByteBuffer.wrap(signedBytes);
            while (buffer.hasRemaining()) {
                int before = buffer.position();
                int written = channelWriter.write(channel, buffer);
                if (written <= 0 || buffer.position() - before != written) {
                    throw new MutationException("OUTPUT_WRITE_FAILED");
                }
            }
            channel.force(true);
            verifyOutputIdentity(target);
            verifyInputIdentity(input);
            if (channel.size() != signedBytes.length) throw new MutationException("OUTPUT_WRITE_FAILED");
            channel.position(0L);
            byte[] written = new byte[signedBytes.length];
            try {
                ByteBuffer verification = ByteBuffer.wrap(written);
                while (verification.hasRemaining()) {
                    if (channel.read(verification) < 0) throw new MutationException("OUTPUT_WRITE_FAILED");
                }
                if (!MessageDigest.isEqual(written, signedBytes)) {
                    throw new MutationException("OUTPUT_WRITE_FAILED");
                }
            } finally {
                Arrays.fill(written, (byte) 0);
            }
        } catch (MutationException exception) {
            if (touched) truncateIfSame(target);
            throw exception;
        } catch (IOException | RuntimeException exception) {
            if (touched) truncateIfSame(target);
            throw new MutationException("OUTPUT_WRITE_FAILED");
        }
    }

    byte[] readOutput(Output target, String expectedSha256) throws MutationException {
        if (!isSha256(expectedSha256)) throw new MutationException("INVALID_OUTPUT_HASH");
        try {
            verifyOutputIdentity(target);
            BasicFileAttributes before = Files.readAttributes(
                target.path(), BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
            );
            if (before.size() < 1 || before.size() > MAX_SIGNING_OUTPUT_BYTES) {
                throw new MutationException("OUTPUT_WRITE_FAILED");
            }
            byte[] bytes = Files.readAllBytes(target.path());
            BasicFileAttributes after = Files.readAttributes(
                target.path(), BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
            );
            if (!sameIdentity(before, after) || !sameFileKey(target.identity(), after)
                || bytes.length != before.size() || !sha256(bytes).equals(expectedSha256)) {
                Arrays.fill(bytes, (byte) 0);
                throw new MutationException("OUTPUT_WRITE_FAILED");
            }
            return bytes;
        } catch (MutationException exception) {
            throw exception;
        } catch (IOException | RuntimeException exception) {
            throw new MutationException("OUTPUT_WRITE_FAILED");
        }
    }

    void discard(Output target) {
        truncateIfSame(target);
    }

    void verifyInputIdentity(Input input) throws MutationException {
        verifyReadIdentity(input, "INPUT_CHANGED");
    }

    void verifyPostvalidationOutputIdentity(Input output) throws MutationException {
        verifyReadIdentity(output, "OUTPUT_CHANGED");
    }

    private static void verifyReadIdentity(Input input, String code) throws MutationException {
        try {
            BasicFileAttributes current = Files.readAttributes(
                input.path(), BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
            );
            if (!sameIdentity(input.identity(), current)
                || !sha256(Files.readAllBytes(input.path())).equals(input.sha256())) {
                throw new MutationException(code);
            }
        } catch (MutationException exception) {
            throw exception;
        } catch (IOException | RuntimeException exception) {
            throw new MutationException(code);
        }
    }

    static void assertAppendOnly(byte[] input, byte[] output) throws MutationException {
        if (output.length <= input.length) throw new MutationException("NOT_INCREMENTAL");
        for (int index = 0; index < input.length; index++) {
            if (input[index] != output[index]) throw new MutationException("NOT_INCREMENTAL");
        }
    }

    static String sha256(byte[] bytes) throws MutationException {
        try {
            return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));
        } catch (NoSuchAlgorithmException exception) {
            throw new MutationException("INTERNAL_ERROR");
        }
    }

    private static void verifyOutputIdentity(Output target) throws IOException, MutationException {
        BasicFileAttributes current = Files.readAttributes(
            target.path(), BasicFileAttributes.class, LinkOption.NOFOLLOW_LINKS
        );
        if (!sameFileKey(target.identity(), current) || !current.isRegularFile() || current.isSymbolicLink()) {
            throw new MutationException("OUTPUT_REPLACED");
        }
    }

    private static void truncateIfSame(Output target) {
        try {
            verifyOutputIdentity(target);
            try (FileChannel channel = FileChannel.open(target.path(), StandardOpenOption.WRITE, LinkOption.NOFOLLOW_LINKS)) {
                channel.truncate(0);
                channel.force(true);
            }
        } catch (IOException | RuntimeException | MutationException exception) {
            // Never truncate through a path whose identity cannot be proved.
        }
    }

    private static Set<OpenOption> outputOptions() {
        Set<OpenOption> options = new HashSet<>();
        options.add(StandardOpenOption.READ);
        options.add(StandardOpenOption.WRITE);
        options.add(LinkOption.NOFOLLOW_LINKS);
        if (isWindows()) options.add(ExtendedOpenOption.NOSHARE_DELETE);
        return Set.copyOf(options);
    }

    private static void assertPrivatePermissions(Path path) throws IOException, MutationException {
        try {
            Set<PosixFilePermission> permissions = Files.getPosixFilePermissions(path, LinkOption.NOFOLLOW_LINKS);
            if (!permissions.equals(Set.of(
                PosixFilePermission.OWNER_READ,
                PosixFilePermission.OWNER_WRITE
            ))) throw new MutationException("UNSAFE_OUTPUT");
        } catch (UnsupportedOperationException ignored) {
            // Windows ACLs are enforced independently by Electron main.
        }
    }

    private static void assertPrivateDirectory(Path path) throws IOException, MutationException {
        try {
            Set<PosixFilePermission> permissions = Files.getPosixFilePermissions(path, LinkOption.NOFOLLOW_LINKS);
            if (!permissions.equals(Set.of(
                PosixFilePermission.OWNER_READ,
                PosixFilePermission.OWNER_WRITE,
                PosixFilePermission.OWNER_EXECUTE
            ))) throw new MutationException("UNSAFE_OUTPUT");
        } catch (UnsupportedOperationException ignored) {
            // Windows ACLs are enforced independently by Electron main.
        }
    }

    private static Path safeAbsolutePath(String encoded, String code) throws MutationException {
        if (encoded == null || encoded.isBlank() || encoded.length() > Protocol.MAX_PATH_LENGTH) {
            throw new MutationException(code);
        }
        try {
            Path path = Path.of(encoded).normalize();
            if (!path.isAbsolute() || !path.toString().equals(encoded)) throw new MutationException(code);
            return path;
        } catch (RuntimeException exception) {
            throw new MutationException(code);
        }
    }

    private static boolean sameIdentity(BasicFileAttributes left, BasicFileAttributes right) {
        return sameFileKey(left, right)
            && left.size() == right.size()
            && left.lastModifiedTime().equals(right.lastModifiedTime())
            && left.creationTime().equals(right.creationTime());
    }

    private static boolean sameFileKey(BasicFileAttributes left, BasicFileAttributes right) {
        return left.fileKey() != null && right.fileKey() != null
            ? left.fileKey().equals(right.fileKey())
            : left.isRegularFile() == right.isRegularFile()
                && left.creationTime().equals(right.creationTime());
    }

    private static boolean isSha256(String value) {
        return value != null && value.matches("[a-f0-9]{64}");
    }

    private static boolean isWindows() {
        return System.getProperty("os.name", "").startsWith("Windows");
    }
}
