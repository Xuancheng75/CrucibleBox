import { describe, expect, it } from 'vitest'
import { StartupMetrics, type ProcessMetricSnapshot } from '../electron/StartupMetrics'

describe('StartupMetrics', () => {
  it('records each milestone once relative to process startup', () => {
    let now = 100
    const metrics = new StartupMetrics(() => now)
    now = 135.4
    expect(metrics.mark('app.ready')).toBeCloseTo(35.4)
    now = 190
    expect(metrics.mark('app.ready')).toBeCloseTo(35.4)
    expect(metrics.mark('renderer.ready')).toBe(90)

    expect(metrics.report('renderer.ready', []).milestones).toEqual({
      'app.ready': 35,
      'renderer.ready': 90
    })
  })

  it('summarizes working sets and only OpenBox plugin utility processes', () => {
    let now = 0
    const metrics = new StartupMetrics(() => now)
    now = 250
    const processes: ProcessMetricSnapshot[] = [
      { pid: 1, type: 'Browser', memory: { workingSetSize: 120 } },
      {
        pid: 2,
        type: 'Utility',
        serviceName: 'openbox-plugin-diary',
        memory: { workingSetSize: 80 }
      },
      {
        pid: 3,
        type: 'Utility',
        serviceName: 'Network Service',
        memory: { workingSetSize: 30 }
      }
    ]

    expect(metrics.report('plugins.restored', processes)).toMatchObject({
      durationMs: 250,
      processes: {
        count: 3,
        pluginUtilityCount: 1,
        totalWorkingSetKiB: 230
      }
    })
  })

  it('clamps negative clocks and working-set values', () => {
    let now = 10
    const metrics = new StartupMetrics(() => now)
    now = 5
    expect(
      metrics.report('clock.adjusted', [
        { pid: 1, type: 'Browser', memory: { workingSetSize: -10 } }
      ])
    ).toMatchObject({ durationMs: 0, processes: { totalWorkingSetKiB: 0 } })
  })
})
