import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import test from 'node:test';

const sourceDirectory = new URL('../src/', import.meta.url);

// A deliberately bounded source check, not a Rust parser or a rendered-colour test.
// Domain colour conversions remain valid; literals belong in a reviewed token layer.
function literalColourCalls(source) {
  return [...source.matchAll(/\b(?:rgb|rgba|hsla)\s*\(\s*(?:0x[\da-fA-F]+|\d+(?:\.\d*)?)/g)]
    .map((match) => match[0]);
}

function embeddedMeasurementSections(appearance, measurement) {
  return /\.content_only\(self\.embedded && !snapshot\.show_offset\)/.test(appearance)
    && /\.content_only\(self\.embedded\)/.test(measurement);
}

test('native UI colour check detects literal constructors while allowing theme tokens and document colours', () => {
  assert.equal(literalColourCalls('div().bg(gpui::rgb(0x000000))').length, 1);
  assert.equal(literalColourCalls('div().border_color(rgba(0xff0000ff))').length, 1);
  assert.equal(literalColourCalls('div().bg(hsla(0., 0., 0., 1.))').length, 1);
  assert.deepEqual(literalColourCalls('div().bg(cx.theme().background); gpui::Rgba::from(annotation_colour); rgb(document_colour)'), []);
});

test('native inspectors, defaults panels, viewer toolbar and system theme use tokens or domain colour values', async () => {
  const files = (await readdir(sourceDirectory)).filter((file) =>
    /(?:inspector|panel|toolbar_strip|system_theme)\.rs$/.test(file));
  assert.ok(files.includes('dimension_property_inspector.rs'));
  assert.ok(files.includes('viewer_toolbar_strip.rs'));
  for (const file of files) {
    const source = await readFile(new URL(file, sourceDirectory), 'utf8');
    assert.deepEqual(literalColourCalls(source), [], `${file}: use semantic theme tokens for control chrome; keep annotation colours as domain values`);
  }
});

test('the shared interaction chrome is the single authored canvas colour source', async () => {
  const source = await readFile(new URL('interaction_chrome.rs', sourceDirectory), 'utf8');
  const allowed = [
    'rgb(0x2563eb', 'rgb(0xfacc15', 'rgb(0x111827', 'rgb(0xffffff', 'rgb(0x94a3b8', 'rgb(0x22c55e',
  ];
  const used = literalColourCalls(source);
  assert.deepEqual(used, allowed);
  for (const colour of allowed) {
    assert.equal(used.filter(value => value === colour).length, 1, `each interaction colour owns one semantic role: ${colour}`);
  }
});

test('embedded appearance and measurement sections retain one scroll owner', async () => {
  const appearance = await readFile(new URL('dimension_property_inspector.rs', sourceDirectory), 'utf8');
  const measurement = await readFile(new URL('measurement_property_inspector.rs', sourceDirectory), 'utf8');
  assert.ok(embeddedMeasurementSections(appearance, measurement), 'embedded measurement sections must contribute content to the outer inspector');
  assert.equal(embeddedMeasurementSections(appearance.replace('.content_only(self.embedded && !snapshot.show_offset)', '.content_only(false)'), measurement), false);
  assert.equal(embeddedMeasurementSections(appearance, measurement.replace('.content_only(self.embedded)', '.content_only(false)')), false);
});
