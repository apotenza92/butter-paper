package com.butterpaper.signaturecore;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.apache.pdfbox.Loader;
import org.apache.pdfbox.pdmodel.PDDocument;
import org.apache.pdfbox.pdmodel.common.PDRectangle;
import org.apache.pdfbox.pdmodel.interactive.form.PDSignatureField;
import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SignatureFieldServiceTest {
    private static final ObjectMapper JSON = new ObjectMapper();

    @Test
    void createsVisibleFieldIncrementallyInNonzeroCropBoxWithRotationAndUserUnit() throws Exception {
        byte[] source = SigningTestFixtures.pdfWithGeometry(90, new PDRectangle(40, 50, 500, 400), 2f);
        SignatureFieldSpec spec = SignatureFieldSpec.parse(newField(
            "Signature_Π_测试",
            widget(0, 80.25, 90.5, 180.75, 60.125, 90),
            lock("exclude", List.of("UnprotectedField"))
        ));
        byte[] output = new SignatureFieldService().addField(source, spec);
        SafePdfMutation.assertAppendOnly(source, output);
        try (PDDocument document = Loader.loadPDF(output)) {
            PDSignatureField field = document.getSignatureFields().getFirst();
            assertEquals("Signature_Π_测试", field.getFullyQualifiedName());
            assertEquals(80.25f, field.getWidgets().getFirst().getRectangle().getLowerLeftX());
            assertEquals(90.5f, field.getWidgets().getFirst().getRectangle().getLowerLeftY());
            assertEquals(180.75f, field.getWidgets().getFirst().getRectangle().getWidth());
            assertEquals(60.125f, field.getWidgets().getFirst().getRectangle().getHeight());
            assertTrue(field.getCOSObject().containsKey(org.apache.pdfbox.cos.COSName.getPDFName("Lock")));
        }
    }

    @Test
    void rejectsCropBoxAndRotationMismatchesBeforeWriting() throws Exception {
        byte[] source = SigningTestFixtures.pdfWithGeometry(270, new PDRectangle(40, 50, 500, 400), 1.5f);
        SignatureFieldService service = new SignatureFieldService();
        SignatureFieldService.FieldException crop = assertThrows(
            SignatureFieldService.FieldException.class,
            () -> service.addField(source, SignatureFieldSpec.parse(newField(
                "Outside", widget(0, 20, 90, 180, 60, 270), null
            )))
        );
        assertEquals("FIELD_OUTSIDE_CROP_BOX", crop.code());
        SignatureFieldService.FieldException rotation = assertThrows(
            SignatureFieldService.FieldException.class,
            () -> service.addField(source, SignatureFieldSpec.parse(newField(
                "WrongRotation", widget(0, 80, 90, 180, 60, 90), null
            )))
        );
        assertEquals("PAGE_ROTATION_MISMATCH", rotation.code());
    }

    @Test
    void rejectsDuplicateFieldNames() throws Exception {
        byte[] source = SigningTestFixtures.pdf();
        SignatureFieldService service = new SignatureFieldService();
        SignatureFieldSpec spec = SignatureFieldSpec.parse(newField("Duplicate", widget(0, 72, 72, 180, 60, 0), null));
        byte[] first = service.addField(source, spec);
        SignatureFieldService.FieldException duplicate = assertThrows(
            SignatureFieldService.FieldException.class,
            () -> service.addField(first, spec)
        );
        assertEquals("DUPLICATE_FIELD_NAME", duplicate.code());
    }

    private static ObjectNode newField(String name, ObjectNode widget, ObjectNode lock) {
        ObjectNode field = JSON.createObjectNode();
        field.put("kind", "new");
        field.put("name", name);
        if (widget == null) field.putNull("widget"); else field.set("widget", widget);
        if (lock != null) field.set("lock", lock);
        return field;
    }

    private static ObjectNode widget(
        int pageIndex,
        double x,
        double y,
        double width,
        double height,
        int pageRotation
    ) {
        ObjectNode widget = JSON.createObjectNode();
        widget.put("pageIndex", pageIndex);
        widget.put("x", x);
        widget.put("y", y);
        widget.put("width", width);
        widget.put("height", height);
        widget.put("pageRotation", pageRotation);
        widget.put("coordinateSpace", "unrotated-pdf-default-user-space");
        return widget;
    }

    private static ObjectNode lock(String action, List<String> fields) {
        ObjectNode lock = JSON.createObjectNode();
        lock.put("action", action);
        var names = lock.putArray("fieldNames");
        fields.forEach(names::add);
        return lock;
    }
}
