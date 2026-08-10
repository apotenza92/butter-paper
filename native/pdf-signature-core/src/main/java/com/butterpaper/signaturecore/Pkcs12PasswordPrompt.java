package com.butterpaper.signaturecore;

import javax.swing.BorderFactory;
import javax.swing.JButton;
import javax.swing.JDialog;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JPasswordField;
import javax.swing.SwingUtilities;
import javax.swing.WindowConstants;
import java.awt.BorderLayout;
import java.awt.Dialog;
import java.awt.FlowLayout;
import java.awt.GraphicsEnvironment;
import java.awt.event.WindowAdapter;
import java.awt.event.WindowEvent;
import java.lang.reflect.InvocationTargetException;
import java.util.Arrays;
import java.util.concurrent.atomic.AtomicReference;

interface Pkcs12PasswordPrompt {
    final class PromptException extends Exception {
        private final String code;

        PromptException(String code) {
            super(code);
            this.code = code;
        }

        String code() { return code; }
    }

    char[] requestPassword() throws PromptException;

    /**
     * Production password boundary. The password remains a mutable char array
     * in the sidecar and never crosses Electron IPC, stdin/stdout, argv or env.
     */
    final class SwingPrompt implements Pkcs12PasswordPrompt {
        private static final int MAX_PASSWORD_CHARACTERS = 4_096;

        @Override
        public char[] requestPassword() throws PromptException {
            if (GraphicsEnvironment.isHeadless()) throw new PromptException("PROVIDER_UI_UNAVAILABLE");
            AtomicReference<char[]> result = new AtomicReference<>();
            AtomicReference<PromptException> failure = new AtomicReference<>();
            Runnable show = () -> {
                JPasswordField field = new JPasswordField(28);
                field.getAccessibleContext().setAccessibleName("PKCS number 12 password");
                JLabel label = new JLabel("Enter the password for the selected PKCS#12 identity:");
                label.setLabelFor(field);
                JButton unlock = new JButton("Unlock");
                JButton cancel = new JButton("Cancel");
                JDialog dialog = new JDialog((java.awt.Frame) null, "Unlock signing identity", Dialog.ModalityType.APPLICATION_MODAL);
                dialog.setDefaultCloseOperation(WindowConstants.DO_NOTHING_ON_CLOSE);
                JPanel content = new JPanel(new BorderLayout(0, 12));
                content.setBorder(BorderFactory.createEmptyBorder(16, 16, 16, 16));
                content.add(label, BorderLayout.NORTH);
                content.add(field, BorderLayout.CENTER);
                JPanel actions = new JPanel(new FlowLayout(FlowLayout.TRAILING));
                actions.add(cancel);
                actions.add(unlock);
                content.add(actions, BorderLayout.SOUTH);
                dialog.setContentPane(content);
                dialog.getRootPane().setDefaultButton(unlock);

                Runnable cancelAction = () -> {
                    char[] discarded = field.getPassword();
                    Arrays.fill(discarded, '\0');
                    field.setText("");
                    failure.set(new PromptException("PROVIDER_CANCELLED"));
                    dialog.dispose();
                };
                cancel.addActionListener(event -> cancelAction.run());
                dialog.addWindowListener(new WindowAdapter() {
                    @Override public void windowClosing(WindowEvent event) { cancelAction.run(); }
                });
                unlock.addActionListener(event -> {
                    char[] password = field.getPassword();
                    field.setText("");
                    if (password.length > MAX_PASSWORD_CHARACTERS) {
                        Arrays.fill(password, '\0');
                        failure.set(new PromptException("PASSWORD_TOO_LARGE"));
                    } else {
                        result.set(password);
                    }
                    dialog.dispose();
                });
                dialog.pack();
                dialog.setResizable(false);
                dialog.setLocationByPlatform(true);
                dialog.setAlwaysOnTop(true);
                SwingUtilities.invokeLater(field::requestFocusInWindow);
                dialog.setVisible(true);
            };
            try {
                if (SwingUtilities.isEventDispatchThread()) show.run();
                else SwingUtilities.invokeAndWait(show);
            } catch (InterruptedException exception) {
                Thread.currentThread().interrupt();
                throw new PromptException("PROVIDER_CANCELLED");
            } catch (InvocationTargetException | RuntimeException exception) {
                throw new PromptException("PROVIDER_UI_UNAVAILABLE");
            }
            if (failure.get() != null) throw failure.get();
            char[] password = result.get();
            if (password == null) throw new PromptException("PROVIDER_CANCELLED");
            return password;
        }
    }
}
