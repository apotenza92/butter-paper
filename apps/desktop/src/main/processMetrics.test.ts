import { describe, expect, it } from 'vitest';
import { createDesktopProcessMetricsSnapshot } from './processMetrics';

describe('createDesktopProcessMetricsSnapshot', () => {
  it('normalises Electron process metrics and totals working sets', () => {
    const snapshot = createDesktopProcessMetricsSnapshot([
      {
        pid: 10,
        type: 'Browser',
        creationTime: 1,
        cpu: { percentCPUUsage: 2.5, cumulativeCPUUsage: 0.4, idleWakeupsPerSecond: 3 },
        memory: { workingSetSize: 100, peakWorkingSetSize: 150 },
      },
      {
        pid: 11,
        type: 'Tab',
        name: 'Renderer',
        creationTime: 2,
        cpu: { percentCPUUsage: 4, idleWakeupsPerSecond: 1 },
        memory: { workingSetSize: 200, peakWorkingSetSize: 250 },
      },
    ], 1234);

    expect(snapshot).toEqual({
      capturedAt: 1234,
      totalWorkingSetKiB: 300,
      totalPeakWorkingSetKiB: 400,
      processes: [
        {
          pid: 10,
          type: 'Browser',
          name: null,
          serviceName: null,
          creationTime: 1,
          cpuPercent: 2.5,
          cumulativeCpuSeconds: 0.4,
          idleWakeupsPerSecond: 3,
          workingSetKiB: 100,
          peakWorkingSetKiB: 150,
        },
        {
          pid: 11,
          type: 'Tab',
          name: 'Renderer',
          serviceName: null,
          creationTime: 2,
          cpuPercent: 4,
          cumulativeCpuSeconds: null,
          idleWakeupsPerSecond: 1,
          workingSetKiB: 200,
          peakWorkingSetKiB: 250,
        },
      ],
    });
  });
});
