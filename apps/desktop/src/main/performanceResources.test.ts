import { describe, expect, it } from 'vitest';
import type { CpuInfo } from 'node:os';
import type { ProcessMetric } from 'electron';
import { DesktopPerformanceResources, aggregateCpuTimes, resolveSystemCpuUsagePercent } from './performanceResources';

const cpu = (times: CpuInfo['times']): CpuInfo => ({ model: 'test', speed: 1, times });

describe('desktop performance resources', () => {
  it('computes system CPU use from consecutive cumulative samples', () => {
    const previous = aggregateCpuTimes([cpu({ user: 20, nice: 0, sys: 10, idle: 70, irq: 0 })]);
    const current = aggregateCpuTimes([cpu({ user: 50, nice: 0, sys: 20, idle: 130, irq: 0 })]);
    expect(resolveSystemCpuUsagePercent(previous, current)).toBe(40);
  });

  it('returns a bounded local resource snapshot', () => {
    const sampler = new DesktopPerformanceResources();
    const metrics: ProcessMetric[] = [{
      pid: 1,
      type: 'Browser',
      creationTime: 1,
      cpu: { percentCPUUsage: 12, idleWakeupsPerSecond: 0 },
      memory: { workingSetSize: 250, peakWorkingSetSize: 300 },
    }];
    const firstCpu = [cpu({ user: 20, nice: 0, sys: 10, idle: 70, irq: 0 })];
    const secondCpu = [cpu({ user: 50, nice: 0, sys: 20, idle: 130, irq: 0 })];

    expect(sampler.sample(metrics, { total: 1000, free: 100, fileBacked: 200, purgeable: 50 }, {
      cpuInfo: firstCpu,
      capturedAt: 10,
      displayRefreshHz: 120,
    })).toEqual({
      capturedAt: 10,
      totalMemoryKiB: 1000,
      reclaimableMemoryKiB: 350,
      appWorkingSetKiB: 250,
      appCpuPercent: 12,
      systemCpuUsagePercent: null,
      displayRefreshHz: 120,
    });
    expect(sampler.sample(metrics, { total: 1000, free: 90, purgeable: 40 }, {
      cpuInfo: secondCpu,
      capturedAt: 20,
    }).systemCpuUsagePercent).toBe(40);
  });
});
