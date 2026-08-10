package com.butterpaper.signaturecore;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;

/** Separate-process probe used only by the Windows mandatory-lock test. */
public final class WindowsCompetingWriterProbe {
    private WindowsCompetingWriterProbe() {}

    public static void main(String[] arguments) throws Exception {
        if (arguments.length != 0) System.exit(64);
        String path = new BufferedReader(new InputStreamReader(System.in)).readLine();
        if (path == null || path.isBlank()) System.exit(64);
        try (FileChannel channel = FileChannel.open(Path.of(path), StandardOpenOption.WRITE)) {
            channel.write(ByteBuffer.wrap(new byte[] { 1 }));
            System.out.println("WRITE_SUCCEEDED");
            System.exit(2);
        } catch (IOException expected) {
            System.out.println("WRITE_BLOCKED");
        }
    }
}
