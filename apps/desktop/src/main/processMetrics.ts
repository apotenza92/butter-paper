import type { ProcessMetric } from 'electron';
import type { DesktopProcessMetricsSnapshot } from '../shared/protocol';

export function createDesktopProcessMetricsSnapshot(
  metrics: readonly ProcessMetric[],
  capturedAt = Date.now(),
): DesktopProcessMetricsSnapshot {
  const processes = metrics.map((metric) => ({
    pid: metric.pid,
    type: metric.type,
    name: metric.name ?? null,
    serviceName: metric.serviceName ?? null,
    creationTime: metric.creationTime,
    cpuPercent: metric.cpu.percentCPUUsage,
    cumulativeCpuSeconds: metric.cpu.cumulativeCPUUsage ?? null,
    idleWakeupsPerSecond: metric.cpu.idleWakeupsPerSecond,
    workingSetKiB: metric.memory.workingSetSize,
    peakWorkingSetKiB: metric.memory.peakWorkingSetSize,
  }));

  return {
    capturedAt,
    totalWorkingSetKiB: processes.reduce((total, process) => total + process.workingSetKiB, 0),
    totalPeakWorkingSetKiB: processes.reduce((total, process) => total + process.peakWorkingSetKiB, 0),
    processes,
  };
}
