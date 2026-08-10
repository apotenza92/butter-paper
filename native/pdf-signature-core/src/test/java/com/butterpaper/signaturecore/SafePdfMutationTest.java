package com.butterpaper.signaturecore;

import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.io.InterruptedIOException;
import java.nio.file.FileSystemException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.attribute.PosixFilePermission;
import java.util.Set;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class SafePdfMutationTest {
    @Test
    void writesOnlyAnAppendOnlyResultToTheBoundPrivateInode() throws Exception {
        byte[] source = SigningTestFixtures.pdf();
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(source);
        SafePdfMutation service = new SafePdfMutation();
        SafePdfMutation.Input input = service.readInput(workspace.source().toString(), workspace.sourceSha256());
        SafePdfMutation.Output output = service.validateOutput(input, workspace.output().toString());
        byte[] incremental = new byte[source.length + 8];
        System.arraycopy(source, 0, incremental, 0, source.length);
        System.arraycopy("revision".getBytes(java.nio.charset.StandardCharsets.US_ASCII), 0, incremental, source.length, 8);
        service.write(output, input, incremental);
        byte[] reopened = service.readOutput(output, SafePdfMutation.sha256(incremental));
        assertArrayEquals(incremental, reopened);
        assertArrayEquals(incremental, Files.readAllBytes(workspace.output()));
        assertArrayEquals(source, Files.readAllBytes(workspace.source()));
    }

    @Test
    void rejectsChangedPrefixesAndTruncatesOnlyItsValidatedOutput() throws Exception {
        byte[] source = SigningTestFixtures.pdf();
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(source);
        SafePdfMutation service = new SafePdfMutation();
        SafePdfMutation.Input input = service.readInput(workspace.source().toString(), workspace.sourceSha256());
        SafePdfMutation.Output output = service.validateOutput(input, workspace.output().toString());
        byte[] rewritten = source.clone();
        rewritten[0] ^= 1;
        SafePdfMutation.MutationException exception = assertThrows(
            SafePdfMutation.MutationException.class,
            () -> service.write(output, input, rewritten)
        );
        assertEquals("NOT_INCREMENTAL", exception.code());
        assertEquals(0, Files.size(workspace.output()));
        assertArrayEquals(source, Files.readAllBytes(workspace.source()));
    }

    @Test
    void rejectsSymlinkAndNonPrivateOutputTargets() throws Exception {
        byte[] source = SigningTestFixtures.pdf();
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(source);
        SafePdfMutation service = new SafePdfMutation();
        SafePdfMutation.Input input = service.readInput(workspace.source().toString(), workspace.sourceSha256());
        Path linked = workspace.directory().resolve("linked.pdf");
        try {
            Files.createSymbolicLink(linked, workspace.output());
            SafePdfMutation.MutationException symlink = assertThrows(
                SafePdfMutation.MutationException.class,
                () -> service.validateOutput(input, linked.toString())
            );
            assertEquals("UNSAFE_OUTPUT", symlink.code());
        } catch (UnsupportedOperationException | FileSystemException ignored) {
            // Platform has no symlink test support.
        }

        try {
            Files.setPosixFilePermissions(workspace.output(), Set.of(
                PosixFilePermission.OWNER_READ,
                PosixFilePermission.OWNER_WRITE,
                PosixFilePermission.GROUP_READ
            ));
            SafePdfMutation.MutationException permissions = assertThrows(
                SafePdfMutation.MutationException.class,
                () -> service.validateOutput(input, workspace.output().toString())
            );
            assertEquals("UNSAFE_OUTPUT", permissions.code());
        } catch (UnsupportedOperationException ignored) {
            // Windows ACL behavior is covered by the platform harness.
        }
    }

    @Test
    void failsClosedOnAZeroProgressShortWrite() throws Exception {
        byte[] source = SigningTestFixtures.pdf();
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(source);
        SafePdfMutation service = new SafePdfMutation((channel, buffer) -> 0);
        SafePdfMutation.Input input = service.readInput(workspace.source().toString(), workspace.sourceSha256());
        SafePdfMutation.Output output = service.validateOutput(input, workspace.output().toString());
        SafePdfMutation.MutationException exception = assertThrows(
            SafePdfMutation.MutationException.class,
            () -> service.write(output, input, incremental(source))
        );
        assertEquals("OUTPUT_WRITE_FAILED", exception.code());
        assertEquals(0, Files.size(workspace.output()));
        assertArrayEquals(source, Files.readAllBytes(workspace.source()));
    }

    @Test
    void truncatesTheBoundOutputAfterDeterministicDiskFullFailure() throws Exception {
        byte[] source = SigningTestFixtures.pdf();
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(source);
        AtomicInteger writes = new AtomicInteger();
        SafePdfMutation service = new SafePdfMutation((channel, buffer) -> {
            if (writes.getAndIncrement() != 0) throw new IOException("TEST_ONLY_DISK_FULL");
            int originalLimit = buffer.limit();
            buffer.limit(buffer.position() + Math.min(32, buffer.remaining()));
            try {
                return channel.write(buffer);
            } finally {
                buffer.limit(originalLimit);
            }
        });
        SafePdfMutation.Input input = service.readInput(workspace.source().toString(), workspace.sourceSha256());
        SafePdfMutation.Output output = service.validateOutput(input, workspace.output().toString());
        SafePdfMutation.MutationException exception = assertThrows(
            SafePdfMutation.MutationException.class,
            () -> service.write(output, input, incremental(source))
        );
        assertEquals("OUTPUT_WRITE_FAILED", exception.code());
        assertEquals(2, writes.get());
        assertEquals(0, Files.size(workspace.output()));
        assertArrayEquals(source, Files.readAllBytes(workspace.source()));
    }

    @Test
    void truncatesTheBoundOutputAfterAnInterruptedWrite() throws Exception {
        byte[] source = SigningTestFixtures.pdf();
        SigningTestFixtures.Workspace workspace = SigningTestFixtures.workspace(source);
        AtomicInteger writes = new AtomicInteger();
        SafePdfMutation service = new SafePdfMutation((channel, buffer) -> {
            if (writes.getAndIncrement() != 0) {
                throw new InterruptedIOException("TEST_ONLY_INTERRUPTED_WRITE");
            }
            int originalLimit = buffer.limit();
            buffer.limit(buffer.position() + Math.min(16, buffer.remaining()));
            try {
                return channel.write(buffer);
            } finally {
                buffer.limit(originalLimit);
            }
        });
        SafePdfMutation.Input input = service.readInput(workspace.source().toString(), workspace.sourceSha256());
        SafePdfMutation.Output output = service.validateOutput(input, workspace.output().toString());
        SafePdfMutation.MutationException exception = assertThrows(
            SafePdfMutation.MutationException.class,
            () -> service.write(output, input, incremental(source))
        );
        assertEquals("OUTPUT_WRITE_FAILED", exception.code());
        assertEquals(2, writes.get());
        assertEquals(0, Files.size(workspace.output()));
        assertArrayEquals(source, Files.readAllBytes(workspace.source()));
    }

    private static byte[] incremental(byte[] source) {
        byte[] result = java.util.Arrays.copyOf(source, source.length + 8);
        System.arraycopy(
            "revision".getBytes(java.nio.charset.StandardCharsets.US_ASCII),
            0,
            result,
            source.length,
            8
        );
        return result;
    }
}
