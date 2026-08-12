import { describe, expect, it } from 'vitest'
import { getOfficialToolArtifactUrl, getToolArtifactIntegrity } from '../src/artifact-integrity'
import { SUPPORTED_TOOL_VERSIONS, type ToolId } from '../src/protocol'

describe('tool artifact integrity catalog', () => {
  it('pins exactly one valid artifact for every supported tool version', () => {
    for (const [tool, versions] of Object.entries(SUPPORTED_TOOL_VERSIONS)) {
      for (const version of versions) {
        const artifact = getToolArtifactIntegrity(tool as ToolId, version)
        expect(artifact.filename).toMatch(/^[A-Za-z0-9][A-Za-z0-9._+-]*$/)
        expect(artifact.sha256).toMatch(/^[a-f0-9]{64}$/)
        const officialUrl = new URL(getOfficialToolArtifactUrl(tool as ToolId, version))
        expect(officialUrl.protocol).toBe('https:')
        expect(officialUrl.username).toBe('')
        expect(officialUrl.password).toBe('')
        expect(decodeURIComponent(officialUrl.pathname).endsWith(`/${artifact.filename}`)).toBe(
          true
        )
      }
    }
  })

  it('uses the published Adoptium release tags for corrected JDK builds', () => {
    expect(getToolArtifactIntegrity('java', '17.0.12')).toMatchObject({
      filename: 'OpenJDK17U-jdk_x64_windows_hotspot_17.0.12_7.zip',
      releaseTag: 'jdk-17.0.12+7'
    })
    expect(getToolArtifactIntegrity('java', '21.0.5')).toMatchObject({
      filename: 'OpenJDK21U-jdk_x64_windows_hotspot_21.0.5_11.zip',
      releaseTag: 'jdk-21.0.5+11'
    })
    expect(decodeURIComponent(getOfficialToolArtifactUrl('java', '17.0.12'))).toContain(
      '/jdk-17.0.12+7/'
    )
    expect(decodeURIComponent(getOfficialToolArtifactUrl('java', '21.0.5'))).toContain(
      '/jdk-21.0.5+11/'
    )
  })

  it('pins the official current Windows artifacts and checksums', () => {
    expect(getToolArtifactIntegrity('python', '3.14.7')).toEqual({
      filename: 'python-3.14.7-amd64.exe',
      sha256: '9d9eb2709ef81bf5cd30db3c2096bdbc4ea10087c22e62f27d356b36f6ae9649'
    })
    expect(getToolArtifactIntegrity('node', '24.18.1')).toEqual({
      filename: 'node-v24.18.1-win-x64.zip',
      sha256: 'ec56b84a7551893ab2324ebdfdc4ab974a63b4781162600b68a1293cc3e53765'
    })
    expect(getToolArtifactIntegrity('git', '2.54.0')).toEqual({
      filename: 'Git-2.54.0-64-bit.exe',
      releaseTag: 'v2.54.0.windows.1',
      sha256: '2b96e7854f0520f0f6b709c21041d9801b1be44d5e1a0d9fa621b2fbc40f1983'
    })
    expect(getToolArtifactIntegrity('go', '1.26.5')).toEqual({
      filename: 'go1.26.5.windows-amd64.zip',
      sha256: '97e6b2a833b6d89f9ff17d25419ac0a7e3b482a044e9ab18cdef834bd834fd38'
    })
    expect(getToolArtifactIntegrity('java', '25.0.4')).toEqual({
      filename: 'OpenJDK25U-jdk_x64_windows_hotspot_25.0.4_7.zip',
      releaseTag: 'jdk-25.0.4+7',
      sha256: '7caab7db43bf4b94a2e6252c699e70d90084f9aa7c943cd3414761fd540937ae'
    })
  })

  it('fails closed for a version without a pinned artifact', () => {
    expect(() => getToolArtifactIntegrity('node', '999.0.0')).toThrow('完整性信息未维护')
  })
})
