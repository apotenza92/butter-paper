import { useEffect, useRef, useState } from 'react';
import type { DesktopPerformanceResourcesSnapshot } from '../../../shared/protocol';
import type { LocalPdfSession } from './documentSession';
import { AdaptivePerformanceController, normalizeAdaptiveInputTimestamp, type AdaptivePerformanceSnapshot } from '../utils/adaptivePerformance';

const RESOURCE_SAMPLE_INTERVAL_MS = 1000;
const CONTROLLER_EVALUATION_INTERVAL_MS = 250;

export function useAdaptivePerformance(
  session: LocalPdfSession | null,
  enabled: boolean,
): AdaptivePerformanceSnapshot {
  const controllerRef = useRef(new AdaptivePerformanceController());
  const resourcesRef = useRef<DesktopPerformanceResourcesSnapshot | null>(null);
  const [snapshot, setSnapshot] = useState(() => controllerRef.current.current());

  useEffect(() => {
    if (!enabled) {
      resourcesRef.current = null;
      return;
    }

    let cancelled = false;
    const sample = async () => {
      try {
        const resources = await window.butterPaper.application.getPerformanceResources();
        if (!cancelled) resourcesRef.current = resources;
      } catch {
        if (!cancelled) resourcesRef.current = null;
      }
    };
    void sample();
    const interval = window.setInterval(() => void sample(), RESOURCE_SAMPLE_INTERVAL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [enabled]);

  useEffect(() => {
    const controller = controllerRef.current;
    if (!enabled || !session) {
      controller.resetFrames();
      return;
    }

    let frameId = 0;
    let lastEvaluationAt = 0;
    const observe = (frameAt: number) => {
      if (document.visibilityState !== 'visible') {
        controller.resetFrames();
        frameId = window.requestAnimationFrame(observe);
        return;
      }

      controller.observeFrame(frameAt);
      if (frameAt - lastEvaluationAt >= CONTROLLER_EVALUATION_INTERVAL_MS) {
        lastEvaluationAt = frameAt;
        const nextSnapshot = controller.evaluate(session.diagnostics(), resourcesRef.current);
        setSnapshot((current) => current.level === nextSnapshot.level
          && current.detectedRefreshHz === nextSnapshot.detectedRefreshHz
          ? current
          : nextSnapshot);
      }
      frameId = window.requestAnimationFrame(observe);
    };
    const observeInput = (event: Event) => {
      const currentTime = performance.now();
      controller.observeInput(normalizeAdaptiveInputTimestamp(event.timeStamp, currentTime));
    };
    const inputEvents: Array<keyof WindowEventMap> = ['wheel', 'pointermove', 'pointerdown', 'keydown'];
    for (const eventName of inputEvents) {
      window.addEventListener(eventName, observeInput, { capture: true, passive: true });
    }
    frameId = window.requestAnimationFrame(observe);
    return () => {
      window.cancelAnimationFrame(frameId);
      for (const eventName of inputEvents) {
        window.removeEventListener(eventName, observeInput, { capture: true });
      }
    };
  }, [enabled, session]);

  useEffect(() => {
    session?.setAdaptivePerformanceLevel(snapshot.level);
  }, [session, snapshot.level]);

  return snapshot;
}
