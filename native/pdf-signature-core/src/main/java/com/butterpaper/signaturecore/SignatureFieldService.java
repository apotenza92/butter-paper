package com.butterpaper.signaturecore;

import org.apache.pdfbox.Loader;
import org.apache.pdfbox.cos.COSArray;
import org.apache.pdfbox.cos.COSBase;
import org.apache.pdfbox.cos.COSDictionary;
import org.apache.pdfbox.cos.COSName;
import org.apache.pdfbox.cos.COSString;
import org.apache.pdfbox.pdmodel.PDDocument;
import org.apache.pdfbox.pdmodel.PDPage;
import org.apache.pdfbox.pdmodel.common.PDRectangle;
import org.apache.pdfbox.pdmodel.interactive.annotation.PDAnnotationWidget;
import org.apache.pdfbox.pdmodel.interactive.form.PDAcroForm;
import org.apache.pdfbox.pdmodel.interactive.form.PDField;
import org.apache.pdfbox.pdmodel.interactive.form.PDSignatureField;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

final class SignatureFieldService {
    static final class FieldException extends Exception {
        private final String code;

        FieldException(String code) {
            super(code);
            this.code = code;
        }

        String code() { return code; }
    }

    byte[] addField(byte[] source, SignatureFieldSpec spec) throws FieldException {
        if (spec.existing()) throw new FieldException("FIELD_ALREADY_EXISTS");
        try (PDDocument document = Loader.loadPDF(source)) {
            if (!document.getSignatureDictionaries().isEmpty()) {
                throw new FieldException("SIGNED_SOURCE_FIELD_CREATION_BLOCKED");
            }
            PDAcroForm form = document.getDocumentCatalog().getAcroForm();
            if (form == null) {
                form = new PDAcroForm(document);
                document.getDocumentCatalog().setAcroForm(form);
            }
            for (PDField field : form.getFieldTree()) {
                if (spec.name().equals(field.getFullyQualifiedName())) throw new FieldException("DUPLICATE_FIELD_NAME");
            }
            PDSignatureField field = new PDSignatureField(form);
            field.setPartialName(spec.name());
            if (spec.lock() != null) field.getCOSObject().setItem(COSName.getPDFName("Lock"), lockDictionary(spec.lock()));
            form.getFields().add(field);
            form.setSignaturesExist(true);
            form.setAppendOnly(true);
            if (spec.pageIndex() != null) {
                if (spec.pageIndex() >= document.getNumberOfPages()) throw new FieldException("PAGE_OUT_OF_RANGE");
                PDPage page = document.getPage(spec.pageIndex());
                assertGeometry(
                    page,
                    new PDRectangle(spec.x(), spec.y(), spec.width(), spec.height()),
                    spec.pageRotation()
                );
                PDAnnotationWidget widget = field.getWidgets().getFirst();
                widget.setRectangle(new PDRectangle(spec.x(), spec.y(), spec.width(), spec.height()));
                widget.setPage(page);
                page.getAnnotations().add(widget);
            }
            ByteArrayOutputStream output = new ByteArrayOutputStream(source.length + 16_384);
            document.saveIncremental(output);
            byte[] bytes = output.toByteArray();
            try {
                SafePdfMutation.assertAppendOnly(source, bytes);
            } catch (SafePdfMutation.MutationException exception) {
                throw new FieldException(exception.code());
            }
            return bytes;
        } catch (FieldException exception) {
            throw exception;
        } catch (IOException | RuntimeException exception) {
            throw new FieldException("FIELD_CREATION_FAILED");
        }
    }

    static void assertGeometry(PDPage page, PDRectangle rectangle, Integer expectedRotation)
        throws FieldException {
        PDRectangle crop = page.getCropBox();
        float x = rectangle.getLowerLeftX();
        float y = rectangle.getLowerLeftY();
        float width = rectangle.getWidth();
        float height = rectangle.getHeight();
        float right = x + width;
        float top = y + height;
        if (!finite(x, y, width, height, right, top,
            crop.getLowerLeftX(), crop.getLowerLeftY(), crop.getUpperRightX(), crop.getUpperRightY())
            || width <= 0 || height <= 0
            || crop.getWidth() <= 0 || crop.getHeight() <= 0
            || x < crop.getLowerLeftX() || y < crop.getLowerLeftY()
            || right > crop.getUpperRightX() || top > crop.getUpperRightY()) {
            throw new FieldException("FIELD_OUTSIDE_CROP_BOX");
        }
        int rotation = ((page.getRotation() % 360) + 360) % 360;
        if (!List.of(0, 90, 180, 270).contains(rotation)
            || (expectedRotation != null && rotation != expectedRotation)) {
            throw new FieldException("PAGE_ROTATION_MISMATCH");
        }
        float userUnit = page.getUserUnit();
        if (!Float.isFinite(userUnit) || userUnit <= 0 || userUnit > 75_000) {
            throw new FieldException("UNSUPPORTED_USER_UNIT");
        }
    }

    static SignatureFieldSpec.FieldLock fieldLock(PDSignatureField field) throws FieldException {
        COSBase raw = field.getCOSObject().getDictionaryObject(COSName.getPDFName("Lock"));
        if (raw == null) return null;
        if (!(raw instanceof COSDictionary dictionary)
            || !"SigFieldLock".equals(dictionary.getNameAsString(COSName.TYPE))) {
            throw new FieldException("SIGNATURE_FIELD_LOCK_INVALID");
        }
        String action = switch (dictionary.getNameAsString(COSName.getPDFName("Action"))) {
            case "All" -> "all";
            case "Include" -> "include";
            case "Exclude" -> "exclude";
            default -> null;
        };
        if (action == null || dictionary.size() != ("all".equals(action) ? 2 : 3)) {
            throw new FieldException("SIGNATURE_FIELD_LOCK_INVALID");
        }
        COSBase rawFields = dictionary.getDictionaryObject(COSName.getPDFName("Fields"));
        if ("all".equals(action)) {
            if (rawFields != null) throw new FieldException("SIGNATURE_FIELD_LOCK_INVALID");
            return new SignatureFieldSpec.FieldLock(action, List.of());
        }
        if (!(rawFields instanceof COSArray fields) || fields.size() == 0 || fields.size() > 256) {
            throw new FieldException("SIGNATURE_FIELD_LOCK_INVALID");
        }
        List<String> names = new ArrayList<>();
        for (int index = 0; index < fields.size(); index++) {
            COSBase value = fields.getObject(index);
            if (!(value instanceof COSString string)) throw new FieldException("SIGNATURE_FIELD_LOCK_INVALID");
            String name = string.getString();
            if (name.isEmpty() || name.length() > 512 || name.codePoints().anyMatch(Character::isISOControl)
                || names.contains(name)) {
                throw new FieldException("SIGNATURE_FIELD_LOCK_INVALID");
            }
            names.add(name);
        }
        return new SignatureFieldSpec.FieldLock(action, List.copyOf(names));
    }

    private static boolean finite(float... values) {
        for (float value : values) if (!Float.isFinite(value)) return false;
        return true;
    }

    Map<String, Object> inspect(byte[] output, String fieldName) throws FieldException {
        try (PDDocument document = Loader.loadPDF(output)) {
            PDSignatureField field = document.getSignatureFields().stream()
                .filter(candidate -> fieldName.equals(candidate.getFullyQualifiedName()))
                .findFirst()
                .orElseThrow(() -> new FieldException("FIELD_MISSING"));
            Map<String, Object> result = new LinkedHashMap<>();
            result.put("fieldName", fieldName);
            result.put("signed", field.getSignature() != null);
            result.put("widgetCount", field.getWidgets().size());
            result.put("fieldLockPresent", field.getCOSObject().containsKey(COSName.getPDFName("Lock")));
            result.put("structurallyReadable", true);
            return result;
        } catch (FieldException exception) {
            throw exception;
        } catch (IOException | RuntimeException exception) {
            throw new FieldException("FIELD_VALIDATION_FAILED");
        }
    }

    private static COSDictionary lockDictionary(SignatureFieldSpec.FieldLock lock) {
        COSDictionary dictionary = new COSDictionary();
        dictionary.setName(COSName.TYPE, "SigFieldLock");
        dictionary.setName(COSName.getPDFName("Action"), switch (lock.action()) {
            case "include" -> "Include";
            case "exclude" -> "Exclude";
            default -> "All";
        });
        if (!lock.fieldNames().isEmpty()) {
            COSArray fields = new COSArray();
            lock.fieldNames().forEach(name -> fields.add(new COSString(name)));
            dictionary.setItem(COSName.getPDFName("Fields"), fields);
        }
        return dictionary;
    }
}
