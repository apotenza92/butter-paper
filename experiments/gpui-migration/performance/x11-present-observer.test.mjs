import assert from "node:assert/strict";
import test from "node:test";

import {
  abortX11PresentObserverCollection,
  beginX11PresentObserverCollection,
  causalX11PresentObserverIntegrationV6,
  finishX11PresentObserverCollection,
  parseX11PresentObserverSample,
  presentObserverCompileArguments,
  presentObserverDependencyBlocker,
  summarizeX11PresentObserverSamples,
  retainX11PresentObserverSample,
  x11PresentObserverActionContext,
} from "./x11-present-observer.mjs";

function sample(overrides = {}) {
  return {
    schema_version: 2,
    observer: "native-x11-present-observer-v1",
    observer_pid: 220,
    window_id: "1234",
    input_window_id: "1234",
    verified_input_window_id: "1234",
    present_window_id: "1234",
    input_target_relation: "same-window",
    input_api: "XTEST",
    action: "click",
    action_token: "annotation-create:rectangle",
    action_sequence: 0,
    action_event_count: 1,
    action_position: "terminal",
    correlation_method:
      "observer-owned-terminal-XTEST-action-after-event-drain",
    input_clock: "CLOCK_MONOTONIC",
    completion_clock: "CLOCK_MONOTONIC",
    completion_signal: "X11-PresentCompleteNotify",
    physical_scanout_observed: false,
    target_viewable_before_action: true,
    target_width: 1200,
    target_height: 800,
    present_extension_major: 1,
    present_extension_minor: 4,
    input_monotonic_ms: 1000,
    action_completed_monotonic_ms: 1000.25,
    present_complete_received_monotonic_ms: 1014.5,
    input_to_present_complete_ms: 14.5,
    present_event_id: "88",
    present_serial: 9,
    present_ust: 10,
    present_msc: 11,
    present_kind: 0,
    present_mode: 1,
    injected_samples: [],
    ...overrides,
  };
}

test("compile plan links the real XPresent, X11, and XTEST clients", () => {
  const args = presentObserverCompileArguments({
    source: "/src/observer.c",
    output: "/out/observer",
  });
  assert(args.includes("-lXpresent"));
  assert(args.includes("-lX11"));
  assert(args.includes("-l:libXtst.so.6"));
  assert(args.includes("-Werror"));
});

test("missing XPresent development files produce an explicit package blocker", () => {
  assert.equal(
    presentObserverDependencyBlocker({ header: true, pkgConfig: true }),
    null,
  );
  const blocker = presentObserverDependencyBlocker({
    header: false,
    pkgConfig: false,
  });
  assert.equal(blocker.code, "xpresent-development-files-missing");
  assert.equal(blocker.dependency.debian_ubuntu_package, "libxpresent-dev");
});

test("parses only a truthful independent XTEST to PresentComplete sample", () => {
  assert.deepEqual(
    parseX11PresentObserverSample(`${JSON.stringify(sample())}\n`, {
      candidatePid: 100,
    }),
    sample(),
  );
  assert.throws(
    () =>
      parseX11PresentObserverSample(
        JSON.stringify(sample({ observer_pid: 100 })),
        { candidatePid: 100 },
      ),
    /identity or XPresent/,
  );
  assert.throws(
    () =>
      parseX11PresentObserverSample(
        JSON.stringify(sample({ present_kind: 1 })),
      ),
    /identity or XPresent/,
  );
  assert.throws(
    () =>
      parseX11PresentObserverSample(
        JSON.stringify(sample({ physical_scanout_observed: true })),
      ),
    /identity or XPresent/,
  );
  assert.throws(
    () =>
      parseX11PresentObserverSample(
        JSON.stringify(sample({ input_to_present_complete_ms: 3 })),
      ),
    /does not match/,
  );
  assert.throws(
    () =>
      parseX11PresentObserverSample(
        JSON.stringify(sample({ action_token: "bad token" })),
      ),
    /identity or XPresent/,
  );
  assert.throws(
    () =>
      parseX11PresentObserverSample(
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
    parseX11PresentObserverSample(JSON.stringify(observed)),
    observed,
  );
});

test("accepts a verified chooser input window distinct from the main Present target", () => {
  const observed = sample({
    action: "key",
    input_window_id: "5679",
    verified_input_window_id: "5678",
    input_target_relation: "verified-distinct-window",
  });
  assert.deepEqual(
    parseX11PresentObserverSample(JSON.stringify(observed)),
    observed,
  );
  assert.throws(
    () =>
      parseX11PresentObserverSample(
        JSON.stringify({ ...observed, input_target_relation: "same-window" }),
      ),
    /identity or XPresent/,
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
      present_complete_received_monotonic_ms: 1001 + index * 100 + index,
      input_to_present_complete_ms: 1 + index,
    }),
  );
  const receipt = summarizeX11PresentObserverSamples(samples, {
    candidatePid: 100,
  });
  assert.equal(receipt.boundary_id, "x11-present-complete-after-xtest-v1");
  assert.equal(receipt.observer_process_independent, true);
  assert.equal(receipt.physical_scanout_observed, false);
  assert.equal(receipt.sample_count, 20);
  assert.equal(receipt.input_to_present_complete_p95_ms, 19);
  assert.equal(receipt.passed, true);
  assert.equal(receipt.decision_timing_eligible, true);
  assert.equal(receipt.causal_action_binding, true);
});

test("summary fails closed on duplicate action correlation identities", () => {
  assert.throws(
    () => summarizeX11PresentObserverSamples([sample(), sample()]),
    /correlation is duplicated/,
  );
});

test("v6 collection issues unique action identities and projects the shared raw receipt", () => {
  abortX11PresentObserverCollection();
  beginX11PresentObserverCollection({ candidatePid: 100 });
  const first = x11PresentObserverActionContext("annotation create");
  const second = x11PresentObserverActionContext("annotation create");
  assert.equal(first.actionToken, "annotation-create:0");
  assert.equal(second.actionToken, "annotation-create:1");
  retainX11PresentObserverSample(
    sample({
      action_token: first.actionToken,
      action_sequence: first.actionSequence,
    }),
  );
  const receipt = finishX11PresentObserverCollection();
  assert.equal(receipt.sample_count, 1);
  assert.equal(receipt.causal_action_binding, true);
  assert.equal(causalX11PresentObserverIntegrationV6.ready, true);
  assert.deepEqual(causalX11PresentObserverIntegrationV6.implementations, [
    "electron",
    "gpui",
  ]);
  abortX11PresentObserverCollection();
});
