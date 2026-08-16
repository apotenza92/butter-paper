import { appendFileSync } from 'node:fs';
import { isAbsolute } from 'node:path';
import type { DesktopStartupMilestone } from '../shared/protocol';

const milestones: DesktopStartupMilestone[] = [];

export function recordTestStartupMilestone(name: string, error?: unknown): void {
  if (process.env.BP_TEST_MODE !== '1') {
    return;
  }

  const capturedAtEpochMs = Date.now();
  milestones.push({
    name,
    capturedAtEpochMs,
    processUptimeMs: Math.round(process.uptime() * 1_000_000) / 1_000,
  });

  const logPath = process.env.BP_TEST_STARTUP_LOG_PATH?.trim();
  if (!logPath || !isAbsolute(logPath)) {
    return;
  }
  const detail = error instanceof Error
    ? `${error.name}: ${error.message}\n${error.stack ?? ''}`
    : error == null ? '' : String(error);
  try {
    appendFileSync(logPath, `${new Date(capturedAtEpochMs).toISOString()} ${name}${detail ? ` ${detail}` : ''}\n`);
  } catch {
    // Startup diagnostics must never affect application startup.
  }
}

export function getTestStartupMilestones(): readonly DesktopStartupMilestone[] {
  return milestones.map((milestone) => ({ ...milestone }));
}
