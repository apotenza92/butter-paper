import assert from "node:assert/strict";
import test from "node:test";

import {
  abortX11DamageObserverCollection,
  beginX11DamageObserverCollection,
  x11DamageObserverIntegrationV6,
  commonX11DamageTimingBoundaryPassedV6,
  finishX11DamageObserverCollection,
  parseX11DamageObserverSample,
  damageObserverCompileArguments,
  damageObserverDependencyBlocker,
  summarizeX11DamageObserverSamples,
  retainX11DamageObserverSample,
  x11DamageObserverActionContext,
} from "./x11-damage-observer.mjs";

function sample(overrides = {}) {
  return {
    schema_version: 3,
    observer: "native-x11-damage-observer-v1",
    observer_pid: 220,
    window_id: "1234",
    input_window_id: "1234",
    verified_input_window_id: "1234",
    damage_drawable_id: "1234",
    input_target_relation: "same-window",
    input_api: "XTEST",
    action: "click",
    action_token: "annotation-create:rectangle",
    action_sequence: 0,
    action_event_count: 1,
    action_position: "terminal",
    correlation_method:
      "observer-owned-terminal-XTEST-action-to-first-target-DamageNotify-after-damage-reset",
    input_clock: "CLOCK_MONOTONIC",
    completion_clock: "CLOCK_MONOTONIC",
    completion_signal: "X11-DamageNotify",
    observation_scope: "x11-server-drawable-damage-not-presentation-completion",
    server_observed_drawable_damage: true,
    presentation_completion_observed: false,
    physical_scanout_observed: false,
    target_viewable_before_action: true,
    target_width: 1200,
    target_height: 800,
    damage_extension_major: 1,
    damage_extension_minor: 1,
    damage_report_level: "XDamageReportNonEmpty",
    input_monotonic_ms: 1000,
    action_completed_monotonic_ms: 1000.25,
    damage_notify_received_monotonic_ms: 1014.5,
    input_to_damage_notify_ms: 14.5,
    damage_handle_id: "88",
    damage_server_timestamp: 10,
    damage_area: { x: 1, y: 2, width: 300, height: 200 },
    damage_geometry: { x: 0, y: 0, width: 1200, height: 800 },
    damage_more: false,
    injected_samples: [],
    ...overrides,
  };
}

test("compile plan links the implementation-neutral XDamage, X11, and XTEST clients", () => {
  const args = damageObserverCompileArguments({
    source: "/src/observer.c",
    output: "/out/observer",
  });
  assert(args.includes("-lXdamage"));
  assert(!args.includes("-lXpresent"));
  assert(args.includes("-lX11"));
  assert(args.includes("-l:libXtst.so.6"));
  assert(args.includes("-Werror"));
});

test("missing XDamage development files produce an explicit package blocker", () => {
  assert.equal(
    damageObserverDependencyBlocker({ header: true, pkgConfig: true }),
    null,
  );
  const blocker = damageObserverDependencyBlocker({
    header: false,
    pkgConfig: false,
  });
  assert.equal(blocker.code, "xdamage-development-files-missing");
  assert.equal(blocker.dependency.debian_ubuntu_package, "libxdamage-dev");
});

test("parses only an honest independent XTEST to server drawable damage sample", () => {
  assert.deepEqual(
    parseX11DamageObserverSample(`${JSON.stringify(sample())}\n`, {
      candidatePid: 100,
    }),
    sample(),
  );
  assert.throws(
    () =>
      parseX11DamageObserverSample(
        JSON.stringify(sample({ observer_pid: 100 })),
        { candidatePid: 100 },
      ),
    /identity or XDamage/,
  );
  assert.throws(
    () =>
      parseX11DamageObserverSample(
        JSON.stringify(sample({ presentation_completion_observed: true })),
      ),
    /identity or XDamage/,
  );
  assert.throws(
    () =>
      parseX11DamageObserverSample(
        JSON.stringify(sample({ physical_scanout_observed: true })),
      ),
    /identity or XDamage/,
  );
  assert.throws(
    () =>
      parseX11DamageObserverSample(
        JSON.stringify(sample({ input_to_damage_notify_ms: 3 })),
      ),
    /does not match/,
  );
  assert.throws(
    () =>
      parseX11DamageObserverSample(
        JSON.stringify(sample({ action_token: "bad token" })),
      ),
    /identity or XDamage/,
  );
  assert.throws(
    () =>
      parseX11DamageObserverSample(
        JSON.stringify(sample({ action_completed_monotonic_ms: 1015 })),
      ),
    /does not match/,
  );
});

test("accepts an exact pointer trace bound to the terminal real replay action", () => {
  const observed = sample({
    action: "pointer",
    action_event_count: 4,
    injected_samples: [
      { sample_index: 0, observed_monotonic_ms: 990, action: "down" },
      { sample_index: 1, observed_monotonic_ms: 995, action: "move" },
      { sample_index: 2, observed_monotonic_ms: 1000, action: "move" },
    ],
  });
  assert.deepEqual(
    parseX11DamageObserverSample(JSON.stringify(observed)),
    observed,
  );
});

test("accepts a verified chooser input window distinct from the damaged target", () => {
  const observed = sample({
    action: "key",
    input_window_id: "5679",
    verified_input_window_id: "5678",
    input_target_relation: "verified-distinct-window",
  });
  assert.deepEqual(
    parseX11DamageObserverSample(JSON.stringify(observed)),
    observed,
  );
  assert.throws(
    () =>
      parseX11DamageObserverSample(
        JSON.stringify({ ...observed, input_target_relation: "same-window" }),
      ),
    /identity or XDamage/,
  );
});

test("accepts a verified chooser click distinct from the damaged target", () => {
  const observed = sample({
    action: "click",
    input_window_id: "5679",
    verified_input_window_id: "5678",
    input_target_relation: "verified-distinct-window",
  });
  assert.deepEqual(
    parseX11DamageObserverSample(JSON.stringify(observed)),
    observed,
  );
});

test("builds the exact frozen boundary receipt with nearest-rank p95", () => {
  const samples = Array.from({ length: 20 }, (_, index) =>
    sample({
      observer_pid: 220 + index,
      action_token: `annotation-create:rectangle-${index}`,
      action_sequence: index,
      input_monotonic_ms: 1000 + index * 100,
      action_completed_monotonic_ms: 1000.25 + index * 100,
      damage_notify_received_monotonic_ms: 1001 + index * 100 + index,
      input_to_damage_notify_ms: 1 + index,
    }),
  );
  const receipt = summarizeX11DamageObserverSamples(samples, {
    candidatePid: 100,
  });
  assert.equal(receipt.schema_version, 2);
  assert.equal(receipt.boundary_id, "x11-damage-notify-after-xtest-v1");
  assert.equal(receipt.completion_signal, "X11-DamageNotify");
  assert.equal(receipt.server_observed_drawable_damage, true);
  assert.equal(receipt.presentation_completion_observed, false);
  assert.equal(receipt.observer_process_independent, true);
  assert.equal(receipt.physical_scanout_observed, false);
  assert.equal(receipt.sample_count, 20);
  assert.equal(receipt.input_to_damage_notify_p95_ms, 19);
  assert.equal(receipt.passed, true);
  assert.equal(receipt.decision_timing_eligible, true);
  assert.equal(receipt.temporal_action_binding, true);
  assert.equal(commonX11DamageTimingBoundaryPassedV6(receipt), true);
  const changed = structuredClone(receipt);
  changed.samples[0].presentation_completion_observed = true;
  assert.equal(commonX11DamageTimingBoundaryPassedV6(changed), false);
});

test("summary fails closed on duplicate action correlation identities", () => {
  assert.throws(
    () => summarizeX11DamageObserverSamples([sample(), sample()]),
    /correlation is duplicated/,
  );
});

test("v6 collection issues unique action identities and projects the shared raw receipt", () => {
  abortX11DamageObserverCollection();
  beginX11DamageObserverCollection({ candidatePid: 100 });
  const first = x11DamageObserverActionContext("annotation create");
  const second = x11DamageObserverActionContext("annotation create");
  assert.equal(first.actionToken, "annotation-create:0");
  assert.equal(second.actionToken, "annotation-create:1");
  retainX11DamageObserverSample(
    sample({
      action_token: first.actionToken,
      action_sequence: first.actionSequence,
    }),
  );
  const receipt = finishX11DamageObserverCollection();
  assert.equal(receipt.sample_count, 1);
  assert.equal(receipt.temporal_action_binding, true);
  assert.equal(x11DamageObserverIntegrationV6.ready, true);
  assert.deepEqual(x11DamageObserverIntegrationV6.implementations, [
    "electron",
    "gpui",
  ]);
  abortX11DamageObserverCollection();
});
