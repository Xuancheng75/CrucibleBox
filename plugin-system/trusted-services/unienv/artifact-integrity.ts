// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import { SUPPORTED_TOOL_VERSIONS, type ToolId } from './protocol'

export interface ToolArtifactIntegrity {
  readonly filename: string
  readonly sha256: string
  readonly releaseTag?: string
}

type ToolArtifactCatalog = {
  readonly [Tool in ToolId]: {
    readonly [Version in (typeof SUPPORTED_TOOL_VERSIONS)[Tool][number]]: ToolArtifactIntegrity
  }
}

const TOOL_ARTIFACTS = {
  python: {
    '3.8.10': {
      filename: 'python-3.8.10-amd64.exe',
      sha256: '7628244cb53408b50639d2c1287c659f4e29d3dfdb9084b11aed5870c0c6a48a'
    },
    '3.9.13': {
      filename: 'python-3.9.13-amd64.exe',
      sha256: 'fb3d0466f3754752ca7fd839a09ffe53375ff2c981279fd4bc23a005458f7f5d'
    },
    '3.10.11': {
      filename: 'python-3.10.11-amd64.exe',
      sha256: 'd8dede5005564b408ba50317108b765ed9c3c510342a598f9fd42681cbe0648b'
    },
    '3.11.9': {
      filename: 'python-3.11.9-amd64.exe',
      sha256: '5ee42c4eee1e6b4464bb23722f90b45303f79442df63083f05322f1785f5fdde'
    },
    '3.12.5': {
      filename: 'python-3.12.5-amd64.exe',
      sha256: '44810512af577ca70b3269b8570b10825ec2ace2b86e4297e767a0f4c0ee8bfd'
    },
    '3.14.7': {
      filename: 'python-3.14.7-amd64.exe',
      sha256: '9d9eb2709ef81bf5cd30db3c2096bdbc4ea10087c22e62f27d356b36f6ae9649'
    }
  },
  node: {
    '16.20.2': {
      filename: 'node-v16.20.2-win-x64.zip',
      sha256: 'f8bb35f6c08dc7bf14ac753509c06ed1a7ebf5b390cd3fbdc8f8c1aedd020ec3'
    },
    '18.20.4': {
      filename: 'node-v18.20.4-win-x64.zip',
      sha256: 'a2864d9048fb83cc85e3b2c3d18f5731b69cae8964bb029f5cdecbb0820eccd7'
    },
    '20.15.1': {
      filename: 'node-v20.15.1-win-x64.zip',
      sha256: 'ba6c3711e2c3d0638c5f7cea3c234553808a73c52a5962a6cdb47b5210b70b04'
    },
    '22.5.1': {
      filename: 'node-v22.5.1-win-x64.zip',
      sha256: '71b74712aa5c6587c428b39d9ec9aa013bfcfa38a2a0ed8e68b3922dda1b69f4'
    },
    '24.18.1': {
      filename: 'node-v24.18.1-win-x64.zip',
      sha256: 'ec56b84a7551893ab2324ebdfdc4ab974a63b4781162600b68a1293cc3e53765'
    }
  },
  git: {
    '2.43.0': {
      filename: 'Git-2.43.0-64-bit.exe',
      releaseTag: 'v2.43.0.windows.1',
      sha256: 'a6058d7c4c16bfa5bcd6fde051a92de8c68535fd7ebade55fc0ab1c41be3c8d5'
    },
    '2.44.0': {
      filename: 'Git-2.44.0-64-bit.exe',
      releaseTag: 'v2.44.0.windows.1',
      sha256: '914ffc96cee0631d09049b9d87d4cd8ac9c98ead9a9f9a094d3341348324a9ec'
    },
    '2.45.2': {
      filename: 'Git-2.45.2-64-bit.exe',
      releaseTag: 'v2.45.2.windows.1',
      sha256: 'ce022a6a19e58bbbd4823f51cf798b006b4a683b93b0616a7bb5beeee901da98'
    },
    '2.46.0': {
      filename: 'Git-2.46.0-64-bit.exe',
      releaseTag: 'v2.46.0.windows.1',
      sha256: 'e6337d172590cea1f673acfeef218733e9352adeb863a3a9e8fa20ee0719a40f'
    },
    '2.54.0': {
      filename: 'Git-2.54.0-64-bit.exe',
      releaseTag: 'v2.54.0.windows.1',
      sha256: '2b96e7854f0520f0f6b709c21041d9801b1be44d5e1a0d9fa621b2fbc40f1983'
    }
  },
  go: {
    '1.21.6': {
      filename: 'go1.21.6.windows-amd64.zip',
      sha256: '27ac9dd6e66fb3fd0acfa6792ff053c86e7d2c055b022f4b5d53bfddec9e3301'
    },
    '1.22.4': {
      filename: 'go1.22.4.windows-amd64.zip',
      sha256: '26321c4d945a0035d8a5bc4a1965b0df401ff8ceac66ce2daadabf9030419a98'
    },
    '1.23.0': {
      filename: 'go1.23.0.windows-amd64.zip',
      sha256: 'd4be481ef73079ee0ad46081d278923aa3fd78db1b3cf147172592f73e14c1ac'
    },
    '1.26.5': {
      filename: 'go1.26.5.windows-amd64.zip',
      sha256: '97e6b2a833b6d89f9ff17d25419ac0a7e3b482a044e9ab18cdef834bd834fd38'
    }
  },
  java: {
    '17.0.11': {
      filename: 'OpenJDK17U-jdk_x64_windows_hotspot_17.0.11_9.zip',
      releaseTag: 'jdk-17.0.11+9',
      sha256: 'fdd6664d4131370398fbc8bfbb7b46dbfec4a22a090a511fe5c379dae188c390'
    },
    '17.0.12': {
      filename: 'OpenJDK17U-jdk_x64_windows_hotspot_17.0.12_7.zip',
      releaseTag: 'jdk-17.0.12+7',
      sha256: '052049d687ebfda6a4032d54afcd0da6549a23bc2ed04cfaa509746eeacbae71'
    },
    '17.0.20': {
      filename: 'OpenJDK17U-jdk_x64_windows_hotspot_17.0.20_8.zip',
      releaseTag: 'jdk-17.0.20+8',
      sha256: '418497be5cf585bdd2203d6486a565d66d3f5e992d5630d45104cb873fab8122'
    },
    '21.0.3': {
      filename: 'OpenJDK21U-jdk_x64_windows_hotspot_21.0.3_9.zip',
      releaseTag: 'jdk-21.0.3+9',
      sha256: 'c43a66cff7a403d56c5c5e1ff10d3d5f95961abf80f97f0e35380594909f0e4d'
    },
    '21.0.5': {
      filename: 'OpenJDK21U-jdk_x64_windows_hotspot_21.0.5_11.zip',
      releaseTag: 'jdk-21.0.5+11',
      sha256: '6f09d4a3598542313cca1540106d537c7092a54e415d569f7b928160a90d3128'
    },
    '21.0.12': {
      filename: 'OpenJDK21U-jdk_x64_windows_hotspot_21.0.12_8.zip',
      releaseTag: 'jdk-21.0.12+8',
      sha256: '9ba963ee2371874a74185d18bc7bb2ab9407df7683300855ed7606e0662321d0'
    },
    '22.0.1': {
      filename: 'OpenJDK22U-jdk_x64_windows_hotspot_22.0.1_8.zip',
      releaseTag: 'jdk-22.0.1+8',
      sha256: '4cf9d3c7ed8ec72a8adcca290d90fdd775100a38670410e674b05233a0c8288e'
    },
    '25.0.4': {
      filename: 'OpenJDK25U-jdk_x64_windows_hotspot_25.0.4_7.zip',
      releaseTag: 'jdk-25.0.4+7',
      sha256: '7caab7db43bf4b94a2e6252c699e70d90084f9aa7c943cd3414761fd540937ae'
    }
  }
} as const satisfies ToolArtifactCatalog

export function getToolArtifactIntegrity(tool: ToolId, version: string): ToolArtifactIntegrity {
  const artifact = (TOOL_ARTIFACTS[tool] as Record<string, ToolArtifactIntegrity>)[version]
  if (!artifact) throw new Error(`${tool} ${version} 的制品完整性信息未维护`)
  return artifact
}

export function getOfficialToolArtifactUrl(tool: ToolId, version: string): string {
  const artifact = getToolArtifactIntegrity(tool, version)
  switch (tool) {
    case 'python':
      return `https://www.python.org/ftp/python/${version}/${artifact.filename}`
    case 'node':
      return `https://nodejs.org/dist/v${version}/${artifact.filename}`
    case 'go':
      return `https://go.dev/dl/${artifact.filename}`
    case 'git':
      if (!artifact.releaseTag) throw new Error(`Git ${version} 的发布标签未维护`)
      return `https://github.com/git-for-windows/git/releases/download/${artifact.releaseTag}/${artifact.filename}`
    case 'java': {
      if (!artifact.releaseTag) throw new Error(`JDK ${version} 的发布标签未维护`)
      const major = version.split('.')[0]
      return `https://github.com/adoptium/temurin${major}-binaries/releases/download/${encodeURIComponent(artifact.releaseTag)}/${artifact.filename}`
    }
  }
}
