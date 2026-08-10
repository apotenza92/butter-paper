package com.butterpaper.signaturecore;

import com.fasterxml.jackson.databind.JsonNode;
import eu.europa.esig.dss.enumerations.VisualSignatureRotation;
import eu.europa.esig.dss.pades.SignatureFieldParameters;

import java.util.ArrayList;
import java.util.List;
import java.util.regex.Pattern;

record SignatureFieldSpec(
    String name,
    boolean existing,
    Integer pageIndex,
    Float x,
    Float y,
    Float width,
    Float height,
    int pageRotation,
    FieldLock lock
) {
    private static final Pattern NEW_FIELD_NAME = Pattern.compile("[^.\\p{Cntrl}]{1,128}");
    private static final Pattern EXISTING_FIELD_NAME = Pattern.compile("[^\\p{Cntrl}]{1,512}");

    record FieldLock(String action, List<String> fieldNames) {}

    boolean hasVisibleWidget() {
        return pageIndex != null;
    }

    static SignatureFieldSpec parse(JsonNode node) throws IllegalArgumentException {
        if (node == null || !node.isObject()) throw new IllegalArgumentException("field is required");
        String kind = text(node.get("kind"));
        String name = text(node.get("name"));
        boolean existing = "existing".equals(kind);
        if ((!existing && !"new".equals(kind)) || name == null
            || !(existing ? EXISTING_FIELD_NAME : NEW_FIELD_NAME).matcher(name).matches()) {
            throw new IllegalArgumentException("invalid field identity");
        }
        FieldLock lock = parseLock(node.get("lock"));
        if (existing) {
            if (node.size() != 2 || lock != null) throw new IllegalArgumentException("existing field shape is invalid");
            return new SignatureFieldSpec(name, true, null, null, null, null, null, 0, null);
        }
        JsonNode widget = node.get("widget");
        if (widget == null || widget.isNull()) {
            if (node.size() != (lock == null ? 3 : 4)) throw new IllegalArgumentException("invisible field shape is invalid");
            return new SignatureFieldSpec(name, false, null, null, null, null, null, 0, lock);
        }
        if (!widget.isObject() || widget.size() != 7) throw new IllegalArgumentException("widget shape is invalid");
        int pageIndex = integer(widget.get("pageIndex"), 0, 100_000);
        float x = finite(widget.get("x"));
        float y = finite(widget.get("y"));
        float width = finite(widget.get("width"));
        float height = finite(widget.get("height"));
        int pageRotation = integer(widget.get("pageRotation"), 0, 270);
        String coordinateSpace = text(widget.get("coordinateSpace"));
        if (width <= 0 || height <= 0 || x < -1_000_000 || y < -1_000_000
            || x + width > 1_000_000 || y + height > 1_000_000
            || !List.of(0, 90, 180, 270).contains(pageRotation)
            || !"unrotated-pdf-default-user-space".equals(coordinateSpace)) {
            throw new IllegalArgumentException("widget geometry is invalid");
        }
        return new SignatureFieldSpec(name, false, pageIndex, x, y, width, height, pageRotation, lock);
    }

    SignatureFieldParameters toDssParameters() {
        SignatureFieldParameters parameters = new SignatureFieldParameters();
        parameters.setFieldId(name);
        if (pageIndex != null) {
            parameters.setPage(pageIndex + 1);
            parameters.setOriginX(x);
            parameters.setOriginY(y);
            parameters.setWidth(width);
            parameters.setHeight(height);
            parameters.setRotation(switch (pageRotation) {
                case 90 -> VisualSignatureRotation.ROTATE_90;
                case 180 -> VisualSignatureRotation.ROTATE_180;
                case 270 -> VisualSignatureRotation.ROTATE_270;
                default -> VisualSignatureRotation.NONE;
            });
        }
        return parameters;
    }

    private static FieldLock parseLock(JsonNode node) {
        if (node == null || node.isNull()) return null;
        if (!node.isObject() || node.size() != 2) throw new IllegalArgumentException("field lock is invalid");
        String action = text(node.get("action"));
        if (!List.of("all", "include", "exclude").contains(action)) {
            throw new IllegalArgumentException("field lock action is invalid");
        }
        JsonNode names = node.get("fieldNames");
        if (!names.isArray() || names.size() > 256) throw new IllegalArgumentException("field lock names are invalid");
        List<String> values = new ArrayList<>();
        for (JsonNode value : names) {
            String name = text(value);
            if (name == null || !EXISTING_FIELD_NAME.matcher(name).matches() || values.contains(name)) {
                throw new IllegalArgumentException("field lock name is invalid");
            }
            values.add(name);
        }
        if ("all".equals(action) != values.isEmpty()) {
            throw new IllegalArgumentException("field lock names do not match the action");
        }
        return new FieldLock(action, List.copyOf(values));
    }

    private static String text(JsonNode node) {
        return node != null && node.isTextual() ? node.textValue() : null;
    }

    private static int integer(JsonNode node, int min, int max) {
        if (node == null || !node.isIntegralNumber() || !node.canConvertToInt()) {
            throw new IllegalArgumentException("integer is invalid");
        }
        int value = node.intValue();
        if (value < min || value > max) throw new IllegalArgumentException("integer is outside bounds");
        return value;
    }

    private static float finite(JsonNode node) {
        if (node == null || !node.isNumber()) {
            throw new IllegalArgumentException("number is invalid");
        }
        double value = node.doubleValue();
        if (!Double.isFinite(value) || value < -1_000_000d || value > 1_000_000d) {
            throw new IllegalArgumentException("number is invalid");
        }
        float narrowed = (float) value;
        if (!Float.isFinite(narrowed)) throw new IllegalArgumentException("number is invalid");
        return narrowed;
    }
}
