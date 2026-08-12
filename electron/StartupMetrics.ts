export interface ProcessMetricSnapshot {
  memory: {
    workingSetSize: number
  }
  name?: string
  pid: number
  serviceName?: string
  type: string
}

export interface StartupPerformanceReport {
  schemaVersion: 1
  name: 'openbox.startup'
  phase: string
  durationMs: number
  milestones: Record<string, number>
  processes: {
    count: number
    pluginUtilityCount: number
    totalWorkingSetKiB: number
  }
}

export type StartupClock = () => number

export class StartupMetrics {
  private readonly clock: StartupClock
  private readonly milestones = new Map<string, number>()
  private readonly startedAt: number

  constructor(clock: StartupClock = () => performance.now()) {
    this.clock = clock
    this.startedAt = clock()
  }

  mark(name: string): number {
    const elapsed = Math.max(0, this.clock() - this.startedAt)
    if (!this.milestones.has(name)) this.milestones.set(name, elapsed)
    return this.milestones.get(name)!
  }

  report(
    phase: string,
    processMetrics: readonly ProcessMetricSnapshot[]
  ): StartupPerformanceReport {
    const durationMs = this.mark(phase)
    let pluginUtilityCount = 0
    let totalWorkingSetKiB = 0
    for (const metric of processMetrics) {
      totalWorkingSetKiB += Math.max(0, metric.memory.workingSetSize)
      if (
        metric.type === 'Utility' &&
        (metric.serviceName?.startsWith('openbox-plugin-') ||
          metric.name?.startsWith('openbox-plugin-'))
      ) {
        pluginUtilityCount += 1
      }
    }
    return {
      schemaVersion: 1,
      name: 'openbox.startup',
      phase,
      durationMs: Math.round(durationMs),
      milestones: Object.fromEntries(
        Array.from(this.milestones, ([name, elapsed]) => [name, Math.round(elapsed)])
      ),
      processes: {
        count: processMetrics.length,
        pluginUtilityCount,
        totalWorkingSetKiB: Math.round(totalWorkingSetKiB)
      }
    }
  }
}
