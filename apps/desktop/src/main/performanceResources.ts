import { cpus, type CpuInfo } from 'node:os';
import type { ProcessMetric } from 'electron';
import type { DesktopPerformanceResourcesSnapshot } from '../shared/protocol';

interface SystemMemorySample {
  readonly total: number;
  readonly free: number;
  readonly fileBacked?: number;
  readonly purgeable?: number;
}

interface CpuTimes {
  readonly active: number;
  readonly total: number;
}

export class DesktopPerformanceResources {
  private previousCpuTimes: CpuTimes | null = null;

  sample(
    processMetrics: readonly ProcessMetric[],
    memory: SystemMemorySample,
    options: {
      readonly cpuInfo?: readonly CpuInfo[];
      readonly capturedAt?: number;
      readonly displayRefreshHz?: number | null;
    } = {},
  ): DesktopPerformanceResourcesSnapshot {
    const cpuInfo = options.cpuInfo ?? cpus();
    const capturedAt = options.capturedAt ?? Date.now();
    const cpuTimes = aggregateCpuTimes(cpuInfo);
    const systemCpuUsagePercent = resolveSystemCpuUsagePercent(this.previousCpuTimes, cpuTimes);
    this.previousCpuTimes = cpuTimes;

    return {
      capturedAt,
      totalMemoryKiB: Math.max(0, memory.total),
      reclaimableMemoryKiB: Math.max(0, memory.free)
        + Math.max(0, memory.fileBacked ?? 0)
        + Math.max(0, memory.purgeable ?? 0),
      appWorkingSetKiB: processMetrics.reduce((total, metric) => total + Math.max(0, metric.memory.workingSetSize), 0),
      appCpuPercent: processMetrics.reduce((total, metric) => total + Math.max(0, metric.cpu.percentCPUUsage), 0),
      systemCpuUsagePercent,
      displayRefreshHz: options.displayRefreshHz && options.displayRefreshHz > 0
        ? options.displayRefreshHz
        : null,
    };
  }
}

export function aggregateCpuTimes(cpuInfo: readonly CpuInfo[]): CpuTimes {
  let idle = 0;
  let total = 0;
  for (const cpu of cpuInfo) {
    const times = cpu.times;
    idle += times.idle;
    total += times.user + times.nice + times.sys + times.idle + times.irq;
  }
  return { active: Math.max(0, total - idle), total };
}

export function resolveSystemCpuUsagePercent(previous: CpuTimes | null, current: CpuTimes): number | null {
  if (!previous) {
    return null;
  }
  const totalDelta = current.total - previous.total;
  const activeDelta = current.active - previous.active;
  if (totalDelta <= 0 || activeDelta < 0) {
    return null;
  }
  return Math.max(0, Math.min(100, activeDelta / totalDelta * 100));
}

export const desktopPerformanceResources = new DesktopPerformanceResources();
