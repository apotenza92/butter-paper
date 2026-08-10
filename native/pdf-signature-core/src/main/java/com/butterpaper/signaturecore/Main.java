package com.butterpaper.signaturecore;
import java.io.PushbackInputStream;
import java.util.Arrays;
public final class Main {
    private Main() {}
    public static void main(String[] args) throws Exception {
        if (args.length != 0) {
            System.err.println("pdf-signature-core accepts protocol data on stdin only; process arguments are rejected");
            System.exit(64);
        }
        PushbackInputStream input = new PushbackInputStream(System.in, FramedProtocolServer.MAGIC.length);
        byte[] prefix = input.readNBytes(FramedProtocolServer.MAGIC.length);
        if (prefix.length == FramedProtocolServer.MAGIC.length
            && Arrays.equals(prefix, FramedProtocolServer.MAGIC)) {
            System.exit(new FramedProtocolServer(new Pkcs12PasswordPrompt.SwingPrompt())
                .runAfterMagic(input, System.out));
        }
        input.unread(prefix);
        System.exit(new ProtocolServer(System.err).run(input, System.out));
    }
}
