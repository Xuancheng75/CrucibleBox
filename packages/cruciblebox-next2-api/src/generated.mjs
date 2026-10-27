// Generated from contracts/next2/contract.json.
export const contract = {
  status: 'candidate',
  versions: {
    manifestVersion: 6,
    sdkApiVersion: 6,
    sdkPackageVersion: '6.0.0-alpha.1',
    wireVersion: 4,
    dataSchemaVersion: 1,
    resourceManifestVersion: 1,
    catalogVersion: 1
  },
  budget: {
    bytes: 65536,
    depth: 32,
    nodes: 16384,
    inflight: 32,
    leaseMs: 300000,
    requestTimeoutMs: 30000,
    interactiveTimeoutMs: 300000,
    handshakeTimeoutMs: 10000,
    stopTimeoutMs: 5000,
    packageFiles: 5000,
    packageBytes: 629145600,
    resourceFiles: 20000,
    resourceBytes: 17179869184,
    manifestBytes: 2097152,
    resourceManifestBytes: 8388608,
    manifestNodes: 200000,
    sessionRequests: 4096,
    storageValueBytes: 4194304,
    storageTransactionBytes: 8388608,
    storageChunkBytes: 24576,
    storageTransactionOps: 32
  },
  $defs: {
    Json: {},
    Empty: { type: 'object', additionalProperties: false, required: [], properties: {} },
    VersionRange: {
      type: 'object',
      additionalProperties: false,
      required: ['min', 'maxExclusive'],
      properties: {
        min: { type: 'string', minLength: 1, maxLength: 100, format: 'semver-without-build' },
        maxExclusive: {
          type: 'string',
          minLength: 1,
          maxLength: 100,
          format: 'semver-without-build'
        }
      }
    },
    ApiRange: {
      type: 'object',
      additionalProperties: false,
      required: ['min', 'maxExclusive'],
      properties: {
        min: { type: 'integer', const: 6 },
        maxExclusive: { type: 'integer', const: 7 }
      }
    },
    Publisher: {
      type: 'object',
      additionalProperties: false,
      required: ['id', 'displayName'],
      properties: {
        id: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' },
        displayName: { type: 'string', minLength: 1, maxLength: 200 }
      }
    },
    FileEntry: {
      type: 'object',
      additionalProperties: false,
      required: ['path', 'bytes', 'sha256'],
      properties: {
        path: { type: 'string', minLength: 1, maxLength: 240, format: 'package-path' },
        bytes: { type: 'integer', minimum: 0, maximum: 17179869184 },
        sha256: { type: 'string', minLength: 1, maxLength: 64, pattern: '^[a-f0-9]{64}$' }
      }
    },
    JsBackend: {
      type: 'object',
      additionalProperties: false,
      required: ['kind', 'entry'],
      properties: {
        kind: { const: 'js' },
        entry: { type: 'string', minLength: 1, maxLength: 240, format: 'package-path' }
      }
    },
    NativeBackend: {
      type: 'object',
      additionalProperties: false,
      required: ['kind', 'entry', 'os', 'arch', 'trust'],
      properties: {
        kind: { const: 'native' },
        entry: { type: 'string', minLength: 1, maxLength: 240, format: 'package-path' },
        os: { const: 'windows' },
        arch: { const: 'x86_64' },
        trust: { const: 'trusted-local-code' }
      }
    },
    Backend: { oneOf: [{ $ref: '#/$defs/JsBackend' }, { $ref: '#/$defs/NativeBackend' }] },
    Navigation: {
      type: 'object',
      additionalProperties: false,
      required: ['title'],
      properties: {
        title: { type: 'string', minLength: 1, maxLength: 100 },
        icon: { type: 'string', minLength: 1, maxLength: 240, format: 'package-path' }
      }
    },
    Command: {
      type: 'object',
      additionalProperties: false,
      required: ['id', 'title', 'method'],
      properties: {
        id: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' },
        title: { type: 'string', minLength: 1, maxLength: 100 },
        method: { type: 'string', minLength: 1, maxLength: 128 }
      }
    },
    Contributions: {
      type: 'object',
      additionalProperties: false,
      required: ['commands'],
      properties: {
        navigation: { $ref: '#/$defs/Navigation' },
        commands: { type: 'array', items: { $ref: '#/$defs/Command' }, maxItems: 64 },
        fileHandlers: { type: 'array', maxItems: 16, items: { $ref: '#/$defs/FileHandler' } }
      }
    },
    ServiceExport: {
      type: 'object',
      additionalProperties: false,
      required: ['id', 'version', 'methods'],
      properties: {
        id: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' },
        version: { type: 'string', minLength: 1, maxLength: 100, format: 'semver-without-build' },
        methods: {
          type: 'array',
          items: { type: 'string', minLength: 1, maxLength: 128 },
          maxItems: 128,
          minItems: 1
        }
      }
    },
    ServiceDependency: {
      type: 'object',
      additionalProperties: false,
      required: ['id', 'providerId', 'version'],
      properties: {
        id: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' },
        providerId: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' },
        version: { $ref: '#/$defs/VersionRange' }
      }
    },
    ResourceDependency: {
      type: 'object',
      additionalProperties: false,
      required: ['id', 'version'],
      properties: {
        id: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' },
        version: { $ref: '#/$defs/VersionRange' },
        catalog: { type: 'string', minLength: 1, maxLength: 240, format: 'package-path' },
        optional: { type: 'boolean' }
      }
    },
    ResourceDownload: {
      type: 'object',
      additionalProperties: false,
      required: ['path', 'url'],
      properties: {
        path: { type: 'string', minLength: 1, maxLength: 240, format: 'package-path' },
        url: { type: 'string', minLength: 9, maxLength: 4096, pattern: '^https://' }
      }
    },
    ResourceRelease: {
      type: 'object',
      additionalProperties: false,
      required: ['id', 'title', 'description', 'manifest', 'downloads'],
      properties: {
        id: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' },
        title: { type: 'string', minLength: 1, maxLength: 200 },
        description: { type: 'string', maxLength: 2000 },
        manifest: { $ref: '#/$defs/ResourceManifest' },
        downloads: {
          type: 'array',
          minItems: 1,
          maxItems: 20000,
          items: { $ref: '#/$defs/ResourceDownload' }
        }
      }
    },
    ResourceCatalog: {
      type: 'object',
      additionalProperties: false,
      required: ['resourceCatalogVersion', 'releases'],
      properties: {
        resourceCatalogVersion: { const: 1 },
        releases: { type: 'array', maxItems: 32, items: { $ref: '#/$defs/ResourceRelease' } }
      }
    },
    PlatformTarget: {
      type: 'object',
      additionalProperties: false,
      required: ['kind'],
      properties: { kind: { const: 'platform' } }
    },
    BackendTarget: {
      type: 'object',
      additionalProperties: false,
      required: ['kind'],
      properties: { kind: { const: 'backend' } }
    },
    ServiceTarget: {
      type: 'object',
      additionalProperties: false,
      required: ['kind', 'id'],
      properties: {
        kind: { const: 'service' },
        id: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' }
      }
    },
    Target: {
      oneOf: [
        { $ref: '#/$defs/PlatformTarget' },
        { $ref: '#/$defs/BackendTarget' },
        { $ref: '#/$defs/ServiceTarget' }
      ]
    },
    Request: {
      type: 'object',
      additionalProperties: false,
      required: ['wireVersion', 'requestId', 'session', 'target', 'method', 'params'],
      properties: {
        wireVersion: { const: 4 },
        requestId: { type: 'string', minLength: 1, maxLength: 128 },
        session: { type: 'string', minLength: 1, maxLength: 64, pattern: '^[a-f0-9]{64}$' },
        target: { $ref: '#/$defs/Target' },
        method: { type: 'string', minLength: 1, maxLength: 128 },
        params: {}
      }
    },
    RpcError: {
      type: 'object',
      additionalProperties: false,
      required: ['code', 'message'],
      properties: {
        code: { type: 'string', minLength: 1, maxLength: 100 },
        message: { type: 'string', minLength: 1, maxLength: 2048 }
      }
    },
    Success: {
      type: 'object',
      additionalProperties: false,
      required: ['wireVersion', 'requestId', 'ok', 'result'],
      properties: {
        wireVersion: { const: 4 },
        requestId: { type: 'string', minLength: 1, maxLength: 128 },
        ok: { const: true },
        result: {}
      }
    },
    Failure: {
      type: 'object',
      additionalProperties: false,
      required: ['wireVersion', 'requestId', 'ok', 'error'],
      properties: {
        wireVersion: { const: 4 },
        requestId: { type: 'string', minLength: 1, maxLength: 128 },
        ok: { const: false },
        error: { $ref: '#/$defs/RpcError' }
      }
    },
    Response: { oneOf: [{ $ref: '#/$defs/Success' }, { $ref: '#/$defs/Failure' }] },
    FileReference: {
      type: 'object',
      additionalProperties: false,
      required: ['ref', 'name', 'bytes'],
      properties: {
        ref: { type: 'string', minLength: 1, maxLength: 128 },
        name: { type: 'string', minLength: 1, maxLength: 240 },
        bytes: { type: 'integer', minimum: 0, maximum: 17179869184 }
      }
    },
    FileReadParams: {
      type: 'object',
      additionalProperties: false,
      required: ['ref', 'offset', 'length'],
      properties: {
        ref: { type: 'string', minLength: 1, maxLength: 128 },
        offset: { type: 'integer', minimum: 0, maximum: 17179869184 },
        length: { type: 'integer', minimum: 1, maximum: 24576 }
      }
    },
    FileResolveParams: {
      type: 'object',
      additionalProperties: false,
      required: ['ref'],
      properties: { ref: { type: 'string', minLength: 1, maxLength: 128 } }
    },
    FilePublishParams: {
      type: 'object',
      additionalProperties: false,
      required: ['taskId', 'path', 'name'],
      properties: {
        taskId: { type: 'string', minLength: 1, maxLength: 128 },
        path: { type: 'string', minLength: 1, maxLength: 32767 },
        name: { type: 'string', minLength: 1, maxLength: 240 }
      }
    },
    TaskStartParams: {
      type: 'object',
      additionalProperties: false,
      required: ['target', 'method', 'params'],
      properties: {
        target: { $ref: '#/$defs/Target' },
        method: { type: 'string', minLength: 1, maxLength: 128 },
        params: {}
      }
    },
    TaskProgressParams: {
      type: 'object',
      additionalProperties: false,
      required: ['taskId', 'sequence', 'stage', 'percent', 'message'],
      properties: {
        taskId: { type: 'string', minLength: 1, maxLength: 128 },
        sequence: { type: 'integer', minimum: 1, maximum: 9007199254740991 },
        stage: { type: 'string', minLength: 1, maxLength: 100 },
        percent: { type: 'integer', minimum: 0, maximum: 100 },
        message: { type: 'string', minLength: 1, maxLength: 2048 }
      }
    },
    BackendHello: {
      type: 'object',
      additionalProperties: false,
      required: ['kind', 'wireVersion', 'sdkApiVersion'],
      properties: {
        kind: { const: 'hello' },
        wireVersion: { const: 4 },
        sdkApiVersion: { const: 6 }
      }
    },
    BackendInit: {
      type: 'object',
      additionalProperties: false,
      required: ['kind', 'wireVersion', 'session', 'pluginId', 'dataDirectory', 'resources'],
      properties: {
        kind: { const: 'initialize' },
        wireVersion: { const: 4 },
        session: { type: 'string', minLength: 1, maxLength: 64, pattern: '^[a-f0-9]{64}$' },
        pluginId: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' },
        dataDirectory: { type: 'string', minLength: 1, maxLength: 32767 },
        resources: { type: 'array', items: { $ref: '#/$defs/ResourceMount' }, maxItems: 64 }
      }
    },
    ResourceMount: {
      type: 'object',
      additionalProperties: false,
      required: ['id', 'version', 'directory'],
      properties: {
        id: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' },
        version: { type: 'string', minLength: 1, maxLength: 100, format: 'semver-without-build' },
        directory: { type: 'string', minLength: 1, maxLength: 32767 }
      }
    },
    BackendReady: {
      type: 'object',
      additionalProperties: false,
      required: ['kind', 'wireVersion'],
      properties: { kind: { const: 'ready' }, wireVersion: { const: 4 } }
    },
    BackendExecute: {
      type: 'object',
      additionalProperties: false,
      required: ['kind', 'request', 'callerId'],
      properties: {
        kind: { const: 'execute' },
        request: { $ref: '#/$defs/Request' },
        taskId: { type: 'string', minLength: 1, maxLength: 128 },
        callerId: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' }
      }
    },
    BackendReply: {
      type: 'object',
      additionalProperties: false,
      required: ['kind', 'response'],
      properties: { kind: { const: 'reply' }, response: { $ref: '#/$defs/Response' } }
    },
    BackendCapability: {
      type: 'object',
      additionalProperties: false,
      required: ['kind', 'request'],
      properties: { kind: { const: 'capability' }, request: { $ref: '#/$defs/Request' } }
    },
    BackendCapabilityReply: {
      type: 'object',
      additionalProperties: false,
      required: ['kind', 'response'],
      properties: { kind: { const: 'capability-result' }, response: { $ref: '#/$defs/Response' } }
    },
    BackendCancel: {
      type: 'object',
      additionalProperties: false,
      required: ['kind', 'wireVersion', 'taskId'],
      properties: {
        kind: { const: 'cancel' },
        wireVersion: { const: 4 },
        taskId: { type: 'string', minLength: 1, maxLength: 128 }
      }
    },
    BackendStop: {
      type: 'object',
      additionalProperties: false,
      required: ['kind', 'wireVersion'],
      properties: { kind: { const: 'stop' }, wireVersion: { const: 4 } }
    },
    BackendFrame: {
      oneOf: [
        { $ref: '#/$defs/BackendHello' },
        { $ref: '#/$defs/BackendInit' },
        { $ref: '#/$defs/BackendReady' },
        { $ref: '#/$defs/BackendExecute' },
        { $ref: '#/$defs/BackendReply' },
        { $ref: '#/$defs/BackendCapability' },
        { $ref: '#/$defs/BackendCapabilityReply' },
        { $ref: '#/$defs/BackendCancel' },
        { $ref: '#/$defs/BackendStop' }
      ]
    },
    RuntimePingParams: {
      type: 'object',
      properties: {},
      required: [],
      additionalProperties: false
    },
    RuntimePingResult: {},
    StorageGetParams: {
      type: 'object',
      properties: {
        key: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' }
      },
      required: ['key'],
      additionalProperties: false
    },
    StorageGetResult: {},
    StorageSetParams: {
      type: 'object',
      properties: {
        key: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' },
        value: {}
      },
      required: ['key', 'value'],
      additionalProperties: false
    },
    StorageSetResult: {},
    StorageDeleteParams: {
      type: 'object',
      properties: {
        key: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' }
      },
      required: ['key'],
      additionalProperties: false
    },
    StorageDeleteResult: {},
    StorageBatchParams: {
      type: 'object',
      properties: {
        operations: {
          type: 'array',
          minItems: 1,
          maxItems: 64,
          items: {
            oneOf: [
              {
                type: 'object',
                properties: {
                  type: { const: 'set' },
                  key: {
                    type: 'string',
                    minLength: 1,
                    maxLength: 128,
                    pattern: '^[A-Za-z0-9_.:-]+$'
                  },
                  value: {}
                },
                required: ['type', 'key', 'value'],
                additionalProperties: false
              },
              {
                type: 'object',
                properties: {
                  type: { const: 'delete' },
                  key: {
                    type: 'string',
                    minLength: 1,
                    maxLength: 128,
                    pattern: '^[A-Za-z0-9_.:-]+$'
                  }
                },
                required: ['type', 'key'],
                additionalProperties: false
              }
            ]
          }
        }
      },
      required: ['operations'],
      additionalProperties: false
    },
    StorageBatchResult: {},
    StorageListParams: {
      type: 'object',
      properties: {
        prefix: { type: 'string', minLength: 0, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]*$' },
        limit: { type: 'integer', minimum: 1, maximum: 100 },
        after: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' }
      },
      required: ['prefix', 'limit'],
      additionalProperties: false
    },
    StorageListResult: {
      type: 'object',
      properties: {
        items: {
          type: 'array',
          maxItems: 100,
          items: {
            type: 'object',
            properties: {
              key: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' },
              value: {}
            },
            required: ['key', 'value'],
            additionalProperties: false
          }
        },
        nextCursor: {
          oneOf: [
            { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' },
            { type: 'null' }
          ]
        }
      },
      required: ['items', 'nextCursor'],
      additionalProperties: false
    },
    StorageKeysParams: {
      type: 'object',
      properties: {
        prefix: { type: 'string', minLength: 0, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]*$' },
        limit: { type: 'integer', minimum: 1, maximum: 100 },
        after: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' }
      },
      required: ['prefix', 'limit'],
      additionalProperties: false
    },
    StorageKeysResult: {
      type: 'object',
      properties: {
        items: {
          type: 'array',
          maxItems: 100,
          items: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' }
        },
        nextCursor: {
          oneOf: [
            { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' },
            { type: 'null' }
          ]
        }
      },
      required: ['items', 'nextCursor'],
      additionalProperties: false
    },
    StorageReadBeginParams: {
      type: 'object',
      properties: {
        key: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' }
      },
      required: ['key'],
      additionalProperties: false
    },
    StorageReadBeginResult: {
      type: 'object',
      properties: {
        found: { type: 'boolean' },
        readId: {
          oneOf: [
            { type: 'string', minLength: 32, maxLength: 128, pattern: '^[A-Za-z0-9]+$' },
            { type: 'null' }
          ]
        },
        byteLength: { type: 'integer', minimum: 0, maximum: 4194304 }
      },
      required: ['found', 'readId', 'byteLength'],
      additionalProperties: false
    },
    StorageReadChunkParams: {
      type: 'object',
      properties: {
        readId: { type: 'string', minLength: 32, maxLength: 128, pattern: '^[A-Za-z0-9]+$' },
        offset: { type: 'integer', minimum: 0, maximum: 4194304 }
      },
      required: ['readId', 'offset'],
      additionalProperties: false
    },
    StorageReadChunkResult: {
      type: 'object',
      properties: {
        offset: { type: 'integer', minimum: 0, maximum: 4194304 },
        data: {
          type: 'string',
          minLength: 1,
          maxLength: 32768,
          pattern: '^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$'
        }
      },
      required: ['offset', 'data'],
      additionalProperties: false
    },
    StorageReadCloseParams: {
      type: 'object',
      properties: {
        readId: { type: 'string', minLength: 32, maxLength: 128, pattern: '^[A-Za-z0-9]+$' }
      },
      required: ['readId'],
      additionalProperties: false
    },
    StorageReadCloseResult: {},
    StorageWriteBeginParams: {
      type: 'object',
      properties: {
        writes: {
          type: 'array',
          maxItems: 32,
          items: {
            type: 'object',
            properties: {
              key: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' },
              byteLength: { type: 'integer', minimum: 1, maximum: 4194304 }
            },
            required: ['key', 'byteLength'],
            additionalProperties: false
          }
        },
        deletes: {
          type: 'array',
          maxItems: 32,
          items: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' }
        }
      },
      required: ['writes', 'deletes'],
      additionalProperties: false
    },
    StorageWriteBeginResult: {
      type: 'object',
      properties: {
        transactionId: { type: 'string', minLength: 32, maxLength: 128, pattern: '^[A-Za-z0-9]+$' }
      },
      required: ['transactionId'],
      additionalProperties: false
    },
    StorageWriteChunkParams: {
      type: 'object',
      properties: {
        transactionId: { type: 'string', minLength: 32, maxLength: 128, pattern: '^[A-Za-z0-9]+$' },
        key: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' },
        offset: { type: 'integer', minimum: 0, maximum: 4194304 },
        data: {
          type: 'string',
          minLength: 1,
          maxLength: 32768,
          pattern: '^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$'
        }
      },
      required: ['transactionId', 'key', 'offset', 'data'],
      additionalProperties: false
    },
    StorageWriteChunkResult: {
      type: 'object',
      properties: { receivedBytes: { type: 'integer', minimum: 1, maximum: 8388608 } },
      required: ['receivedBytes'],
      additionalProperties: false
    },
    StorageWriteCommitParams: {
      type: 'object',
      properties: {
        transactionId: { type: 'string', minLength: 32, maxLength: 128, pattern: '^[A-Za-z0-9]+$' }
      },
      required: ['transactionId'],
      additionalProperties: false
    },
    StorageWriteCommitResult: {},
    StorageWriteAbortParams: {
      type: 'object',
      properties: {
        transactionId: { type: 'string', minLength: 32, maxLength: 128, pattern: '^[A-Za-z0-9]+$' }
      },
      required: ['transactionId'],
      additionalProperties: false
    },
    StorageWriteAbortResult: {},
    ConfigGetParams: { type: 'object', properties: {}, required: [], additionalProperties: false },
    ConfigGetResult: {},
    ConfigPatchParams: {
      type: 'object',
      properties: { values: { type: 'object', additionalProperties: true } },
      required: ['values'],
      additionalProperties: false
    },
    ConfigPatchResult: {},
    TasksGetParams: {
      type: 'object',
      properties: {
        taskId: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' }
      },
      required: ['taskId'],
      additionalProperties: false
    },
    TasksGetResult: {},
    TasksControlStateParams: {
      type: 'object',
      properties: {
        taskId: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' },
        executionPaused: { type: 'boolean' }
      },
      required: ['taskId'],
      additionalProperties: false
    },
    TasksCancelParams: {
      type: 'object',
      properties: {
        taskId: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' }
      },
      required: ['taskId'],
      additionalProperties: false
    },
    TasksCancelResult: {},
    TasksListParams: {
      type: 'object',
      properties: {
        limit: { type: 'integer', minimum: 1, maximum: 20 },
        after: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[A-Za-z0-9_.:-]+$' }
      },
      required: ['limit'],
      additionalProperties: false
    },
    TasksListResult: {},
    ThemeGetParams: { type: 'object', properties: {}, required: [], additionalProperties: false },
    ThemeGetResult: {},
    ThemeListParams: { type: 'object', properties: {}, required: [], additionalProperties: false },
    ThemeListResult: {},
    ThemePreviewParams: {
      type: 'object',
      properties: {
        theme: {
          type: 'object',
          properties: {
            id: { type: 'string', minLength: 1, maxLength: 64 },
            name: { type: 'string', minLength: 1, maxLength: 80 },
            mode: { enum: ['light', 'dark'] },
            tokens: {
              type: 'object',
              properties: {
                colorBg: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorBgLayout: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorBgContainer: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorBgElevated: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorPrimary: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorPrimaryHover: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorPrimaryBg: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorText: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorTextSecondary: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorTextTertiary: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorBorder: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorBorderSecondary: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorSuccess: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorSuccessBg: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorWarning: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorWarningBg: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorError: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorErrorBg: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorLink: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                borderRadius: { type: 'number', minimum: 0, maximum: 32 },
                fontFamily: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 512,
                  pattern: '^[^;<>\\\\]+$'
                }
              },
              required: [
                'colorBg',
                'colorBgLayout',
                'colorBgContainer',
                'colorBgElevated',
                'colorPrimary',
                'colorPrimaryHover',
                'colorPrimaryBg',
                'colorText',
                'colorTextSecondary',
                'colorTextTertiary',
                'colorBorder',
                'colorBorderSecondary',
                'colorSuccess',
                'colorSuccessBg',
                'colorWarning',
                'colorWarningBg',
                'colorError',
                'colorErrorBg',
                'colorLink',
                'borderRadius',
                'fontFamily'
              ],
              additionalProperties: false
            }
          },
          required: ['id', 'name', 'mode', 'tokens'],
          additionalProperties: false
        }
      },
      required: ['theme'],
      additionalProperties: false
    },
    ThemePreviewResult: {},
    ThemeCommitParams: {
      type: 'object',
      properties: {},
      required: [],
      additionalProperties: false
    },
    ThemeCommitResult: {},
    ThemeRollbackParams: {
      type: 'object',
      properties: {},
      required: [],
      additionalProperties: false
    },
    ThemeRollbackResult: {},
    ThemeSetParams: {
      type: 'object',
      properties: {
        theme: {
          type: 'object',
          properties: {
            id: { type: 'string', minLength: 1, maxLength: 64 },
            name: { type: 'string', minLength: 1, maxLength: 80 },
            mode: { enum: ['light', 'dark'] },
            tokens: {
              type: 'object',
              properties: {
                colorBg: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorBgLayout: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorBgContainer: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorBgElevated: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorPrimary: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorPrimaryHover: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorPrimaryBg: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorText: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorTextSecondary: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorTextTertiary: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorBorder: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorBorderSecondary: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorSuccess: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorSuccessBg: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorWarning: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorWarningBg: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorError: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorErrorBg: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                colorLink: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 128,
                  pattern:
                    '^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$'
                },
                borderRadius: { type: 'number', minimum: 0, maximum: 32 },
                fontFamily: {
                  type: 'string',
                  minLength: 1,
                  maxLength: 512,
                  pattern: '^[^;<>\\\\]+$'
                }
              },
              required: [
                'colorBg',
                'colorBgLayout',
                'colorBgContainer',
                'colorBgElevated',
                'colorPrimary',
                'colorPrimaryHover',
                'colorPrimaryBg',
                'colorText',
                'colorTextSecondary',
                'colorTextTertiary',
                'colorBorder',
                'colorBorderSecondary',
                'colorSuccess',
                'colorSuccessBg',
                'colorWarning',
                'colorWarningBg',
                'colorError',
                'colorErrorBg',
                'colorLink',
                'borderRadius',
                'fontFamily'
              ],
              additionalProperties: false
            }
          },
          required: ['id', 'name', 'mode', 'tokens'],
          additionalProperties: false
        }
      },
      required: ['theme'],
      additionalProperties: false
    },
    ThemeSetResult: {},
    DialogOpenParams: {
      type: 'object',
      properties: {
        type: { enum: ['file', 'folder'] },
        multiple: { type: 'boolean' },
        extensions: {
          type: 'array',
          maxItems: 32,
          items: { type: 'string', minLength: 1, maxLength: 16, pattern: '^[A-Za-z0-9]+$' }
        }
      },
      required: ['type'],
      additionalProperties: false
    },
    DialogOpenResult: {},
    DialogConfirmParams: {
      type: 'object',
      properties: {
        title: { type: 'string', minLength: 1, maxLength: 200 },
        message: { type: 'string', minLength: 1, maxLength: 4000 },
        confirmLabel: { type: 'string', minLength: 1, maxLength: 80 },
        cancelLabel: { type: 'string', minLength: 1, maxLength: 80 }
      },
      required: ['title', 'message'],
      additionalProperties: false
    },
    DialogConfirmResult: {},
    NotificationShowParams: {
      type: 'object',
      properties: {
        title: { type: 'string', minLength: 1, maxLength: 200 },
        body: { type: 'string', maxLength: 4000 }
      },
      required: ['title', 'body'],
      additionalProperties: false
    },
    NotificationShowResult: {},
    Manifest: {
      type: 'object',
      additionalProperties: false,
      required: [
        'manifestVersion',
        'sdkApiVersion',
        'id',
        'version',
        'displayName',
        'description',
        'publisher',
        'minHostVersion',
        'sdk',
        'renderer',
        'permissions',
        'background',
        'contributes',
        'exports',
        'dependencies',
        'resources',
        'files'
      ],
      properties: {
        manifestVersion: { const: 6 },
        sdkApiVersion: { const: 6 },
        id: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' },
        version: { type: 'string', minLength: 1, maxLength: 100, format: 'semver-without-build' },
        displayName: { type: 'string', minLength: 1, maxLength: 100 },
        description: { type: 'string', maxLength: 2000 },
        publisher: { $ref: '#/$defs/Publisher' },
        minHostVersion: {
          type: 'string',
          minLength: 1,
          maxLength: 100,
          format: 'semver-without-build'
        },
        sdk: { $ref: '#/$defs/ApiRange' },
        renderer: { type: 'string', minLength: 1, maxLength: 240, format: 'package-path' },
        backend: { $ref: '#/$defs/Backend' },
        permissions: {
          type: 'array',
          items: {
            enum: [
              'storage:read',
              'storage:write',
              'tasks:read',
              'tasks:control',
              'theme:read',
              'theme:write',
              'dialog',
              'notification',
              'clipboard',
              'files:read',
              'files:publish',
              'tasks:execute',
              'browser:downloads',
              'network:fetch'
            ]
          },
          maxItems: 128,
          uniqueItems: true
        },
        background: { type: 'boolean' },
        contributes: { $ref: '#/$defs/Contributions' },
        exports: { type: 'array', items: { $ref: '#/$defs/ServiceExport' }, maxItems: 64 },
        dependencies: { type: 'array', items: { $ref: '#/$defs/ServiceDependency' }, maxItems: 64 },
        resources: { type: 'array', items: { $ref: '#/$defs/ResourceDependency' }, maxItems: 64 },
        files: { type: 'array', items: { $ref: '#/$defs/FileEntry' }, maxItems: 5000, minItems: 1 },
        config: { type: 'object', additionalProperties: true }
      }
    },
    ResourceManifest: {
      type: 'object',
      additionalProperties: false,
      required: [
        'resourceManifestVersion',
        'id',
        'pluginId',
        'version',
        'kind',
        'containsNativeCode',
        'source',
        'license',
        'pluginVersion',
        'os',
        'arch',
        'files',
        'publisher'
      ],
      properties: {
        resourceManifestVersion: { const: 1 },
        id: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' },
        pluginId: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' },
        version: { type: 'string', minLength: 1, maxLength: 100, format: 'semver-without-build' },
        kind: { enum: ['model', 'runtime', 'data'] },
        containsNativeCode: { type: 'boolean' },
        source: { type: 'string', minLength: 1, maxLength: 2048 },
        license: { type: 'string', minLength: 1, maxLength: 2000 },
        pluginVersion: { $ref: '#/$defs/VersionRange' },
        os: { const: 'windows' },
        arch: { const: 'x86_64' },
        files: {
          type: 'array',
          items: { $ref: '#/$defs/FileEntry' },
          maxItems: 20000,
          minItems: 1
        },
        publisher: { $ref: '#/$defs/Publisher' }
      }
    },
    RendererAppearance: {
      type: 'object',
      properties: {
        mode: { enum: ['light', 'dark'] },
        cssVars: {
          type: 'object',
          properties: {
            '--ob-mode': { type: 'string', maxLength: 512 },
            '--ob-theme-id': { type: 'string', maxLength: 512 },
            '--ob-color-primary-contrast': { type: 'string', maxLength: 512 },
            '--ob-color-success-border': { type: 'string', maxLength: 512 },
            '--ob-color-warning-border': { type: 'string', maxLength: 512 },
            '--ob-color-error-border': { type: 'string', maxLength: 512 },
            '--ob-color-bg': { type: 'string', maxLength: 512 },
            '--ob-color-bg-layout': { type: 'string', maxLength: 512 },
            '--ob-color-bg-container': { type: 'string', maxLength: 512 },
            '--ob-color-bg-elevated': { type: 'string', maxLength: 512 },
            '--ob-color-primary': { type: 'string', maxLength: 512 },
            '--ob-color-primary-hover': { type: 'string', maxLength: 512 },
            '--ob-color-primary-bg': { type: 'string', maxLength: 512 },
            '--ob-color-text': { type: 'string', maxLength: 512 },
            '--ob-color-text-secondary': { type: 'string', maxLength: 512 },
            '--ob-color-text-tertiary': { type: 'string', maxLength: 512 },
            '--ob-color-border': { type: 'string', maxLength: 512 },
            '--ob-color-border-secondary': { type: 'string', maxLength: 512 },
            '--ob-color-success': { type: 'string', maxLength: 512 },
            '--ob-color-success-bg': { type: 'string', maxLength: 512 },
            '--ob-color-warning': { type: 'string', maxLength: 512 },
            '--ob-color-warning-bg': { type: 'string', maxLength: 512 },
            '--ob-color-error': { type: 'string', maxLength: 512 },
            '--ob-color-error-bg': { type: 'string', maxLength: 512 },
            '--ob-color-link': { type: 'string', maxLength: 512 },
            '--ob-radius': { type: 'string', maxLength: 512 },
            '--ob-font-family': { type: 'string', maxLength: 512 },
            '--ob-colorBg': { type: 'string', maxLength: 512 },
            '--ob-colorBgLayout': { type: 'string', maxLength: 512 },
            '--ob-colorBgContainer': { type: 'string', maxLength: 512 },
            '--ob-colorBgElevated': { type: 'string', maxLength: 512 },
            '--ob-colorPrimary': { type: 'string', maxLength: 512 },
            '--ob-colorPrimaryHover': { type: 'string', maxLength: 512 },
            '--ob-colorPrimaryBg': { type: 'string', maxLength: 512 },
            '--ob-colorText': { type: 'string', maxLength: 512 },
            '--ob-colorTextSecondary': { type: 'string', maxLength: 512 },
            '--ob-colorTextTertiary': { type: 'string', maxLength: 512 },
            '--ob-colorBorder': { type: 'string', maxLength: 512 },
            '--ob-colorBorderSecondary': { type: 'string', maxLength: 512 },
            '--ob-colorSuccess': { type: 'string', maxLength: 512 },
            '--ob-colorSuccessBg': { type: 'string', maxLength: 512 },
            '--ob-colorWarning': { type: 'string', maxLength: 512 },
            '--ob-colorWarningBg': { type: 'string', maxLength: 512 },
            '--ob-colorError': { type: 'string', maxLength: 512 },
            '--ob-colorErrorBg': { type: 'string', maxLength: 512 },
            '--ob-colorLink': { type: 'string', maxLength: 512 },
            '--ob-borderRadius': { type: 'string', maxLength: 512 },
            '--ob-fontFamily': { type: 'string', maxLength: 512 }
          },
          required: [
            '--ob-mode',
            '--ob-theme-id',
            '--ob-color-primary-contrast',
            '--ob-color-success-border',
            '--ob-color-warning-border',
            '--ob-color-error-border',
            '--ob-color-bg',
            '--ob-color-bg-layout',
            '--ob-color-bg-container',
            '--ob-color-bg-elevated',
            '--ob-color-primary',
            '--ob-color-primary-hover',
            '--ob-color-primary-bg',
            '--ob-color-text',
            '--ob-color-text-secondary',
            '--ob-color-text-tertiary',
            '--ob-color-border',
            '--ob-color-border-secondary',
            '--ob-color-success',
            '--ob-color-success-bg',
            '--ob-color-warning',
            '--ob-color-warning-bg',
            '--ob-color-error',
            '--ob-color-error-bg',
            '--ob-color-link',
            '--ob-radius',
            '--ob-font-family',
            '--ob-colorBg',
            '--ob-colorBgLayout',
            '--ob-colorBgContainer',
            '--ob-colorBgElevated',
            '--ob-colorPrimary',
            '--ob-colorPrimaryHover',
            '--ob-colorPrimaryBg',
            '--ob-colorText',
            '--ob-colorTextSecondary',
            '--ob-colorTextTertiary',
            '--ob-colorBorder',
            '--ob-colorBorderSecondary',
            '--ob-colorSuccess',
            '--ob-colorSuccessBg',
            '--ob-colorWarning',
            '--ob-colorWarningBg',
            '--ob-colorError',
            '--ob-colorErrorBg',
            '--ob-colorLink',
            '--ob-borderRadius',
            '--ob-fontFamily'
          ],
          additionalProperties: false
        }
      },
      required: ['mode', 'cssVars'],
      additionalProperties: false
    },
    TaskSnapshot: {
      type: 'object',
      properties: {
        taskId: { type: 'string', minLength: 1, maxLength: 128 },
        status: {
          enum: ['queued', 'running', 'paused', 'succeeded', 'failed', 'cancelled', 'interrupted']
        },
        sequence: { type: 'integer', minimum: 0, maximum: 9007199254740991 },
        cancelRequested: { type: 'boolean' },
        resultRefs: { type: 'array', maxItems: 32, items: { type: 'string', maxLength: 2048 } },
        resourceKey: { type: 'string', maxLength: 256 },
        error: {},
        progress: {},
        publication: {},
        checkpoint: {},
        initiatorId: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' },
        providerId: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' }
      },
      required: [
        'taskId',
        'status',
        'sequence',
        'cancelRequested',
        'resultRefs',
        'resourceKey',
        'initiatorId',
        'providerId'
      ],
      additionalProperties: true
    },
    SignatureRecord: {
      type: 'object',
      additionalProperties: false,
      properties: {
        signatureVersion: { const: 1 },
        publisherId: { type: 'string', minLength: 1, maxLength: 64, format: 'plugin-id' },
        keyId: { type: 'string', minLength: 1, maxLength: 100 },
        signature: {
          type: 'string',
          minLength: 1,
          maxLength: 128,
          pattern: '^[A-Za-z0-9+/]{86}==$'
        }
      },
      required: ['signatureVersion', 'publisherId', 'keyId', 'signature']
    },
    NetworkFetchParams: {
      type: 'object',
      additionalProperties: false,
      properties: {
        url: { type: 'string', minLength: 1, maxLength: 2048 },
        method: { enum: ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD'] },
        headers: { type: 'object', additionalProperties: true },
        body: { type: 'string', maxLength: 32768 }
      },
      required: ['url', 'method']
    },
    NetworkFetchResult: {
      type: 'object',
      additionalProperties: false,
      properties: {
        status: { type: 'integer', minimum: 100, maximum: 599 },
        body: { type: 'string', maxLength: 32768 },
        headers: { type: 'object', additionalProperties: true }
      },
      required: ['status', 'body', 'headers']
    },
    FileHandler: {
      type: 'object',
      additionalProperties: false,
      required: ['extensions'],
      properties: {
        extensions: {
          type: 'array',
          minItems: 1,
          maxItems: 64,
          uniqueItems: true,
          items: { type: 'string', minLength: 1, maxLength: 20, pattern: '^[a-z0-9]+$' }
        }
      }
    }
  },
  methods: {
    'runtime.ping': {
      permission: null,
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/RuntimePingParams' },
      result: { $ref: '#/$defs/RuntimePingResult' }
    },
    'storage.get': {
      permission: 'storage:read',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/StorageGetParams' },
      result: { $ref: '#/$defs/StorageGetResult' }
    },
    'storage.set': {
      permission: 'storage:write',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/StorageSetParams' },
      result: { $ref: '#/$defs/StorageSetResult' }
    },
    'storage.delete': {
      permission: 'storage:write',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/StorageDeleteParams' },
      result: { $ref: '#/$defs/StorageDeleteResult' }
    },
    'storage.batch': {
      permission: 'storage:write',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/StorageBatchParams' },
      result: { $ref: '#/$defs/StorageBatchResult' }
    },
    'storage.list': {
      permission: 'storage:read',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/StorageListParams' },
      result: { $ref: '#/$defs/StorageListResult' }
    },
    'storage.keys': {
      permission: 'storage:read',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/StorageKeysParams' },
      result: { $ref: '#/$defs/StorageKeysResult' }
    },
    'storage.read.begin': {
      permission: 'storage:read',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/StorageReadBeginParams' },
      result: { $ref: '#/$defs/StorageReadBeginResult' }
    },
    'storage.read.chunk': {
      permission: 'storage:read',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/StorageReadChunkParams' },
      result: { $ref: '#/$defs/StorageReadChunkResult' }
    },
    'storage.read.close': {
      permission: 'storage:read',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/StorageReadCloseParams' },
      result: { $ref: '#/$defs/StorageReadCloseResult' }
    },
    'storage.write.begin': {
      permission: 'storage:write',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/StorageWriteBeginParams' },
      result: { $ref: '#/$defs/StorageWriteBeginResult' }
    },
    'storage.write.chunk': {
      permission: 'storage:write',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/StorageWriteChunkParams' },
      result: { $ref: '#/$defs/StorageWriteChunkResult' }
    },
    'storage.write.commit': {
      permission: 'storage:write',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/StorageWriteCommitParams' },
      result: { $ref: '#/$defs/StorageWriteCommitResult' }
    },
    'storage.write.abort': {
      permission: 'storage:write',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/StorageWriteAbortParams' },
      result: { $ref: '#/$defs/StorageWriteAbortResult' }
    },
    'config.get': {
      permission: 'storage:read',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/ConfigGetParams' },
      result: { $ref: '#/$defs/ConfigGetResult' }
    },
    'config.patch': {
      permission: 'storage:write',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/ConfigPatchParams' },
      result: { $ref: '#/$defs/ConfigPatchResult' }
    },
    'tasks.get': {
      permission: 'tasks:read',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/TasksGetParams' },
      result: { $ref: '#/$defs/TasksGetResult' }
    },
    'tasks.cancel': {
      permission: 'tasks:control',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/TasksCancelParams' },
      result: { $ref: '#/$defs/TasksCancelResult' }
    },
    'tasks.list': {
      permission: 'tasks:read',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/TasksListParams' },
      result: { $ref: '#/$defs/TasksListResult' }
    },
    'theme.get': {
      permission: 'theme:read',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/ThemeGetParams' },
      result: { $ref: '#/$defs/ThemeGetResult' }
    },
    'theme.list': {
      permission: 'theme:read',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/ThemeListParams' },
      result: { $ref: '#/$defs/ThemeListResult' }
    },
    'theme.preview': {
      permission: 'theme:write',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/ThemePreviewParams' },
      result: { $ref: '#/$defs/ThemePreviewResult' }
    },
    'theme.commit': {
      permission: 'theme:write',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/ThemeCommitParams' },
      result: { $ref: '#/$defs/ThemeCommitResult' }
    },
    'theme.rollback': {
      permission: 'theme:write',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/ThemeRollbackParams' },
      result: { $ref: '#/$defs/ThemeRollbackResult' }
    },
    'theme.set': {
      permission: 'theme:write',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/ThemeSetParams' },
      result: { $ref: '#/$defs/ThemeSetResult' }
    },
    'dialog.open': {
      permission: 'dialog',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/DialogOpenParams' },
      result: { $ref: '#/$defs/DialogOpenResult' }
    },
    'dialog.confirm': {
      permission: 'dialog',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/DialogConfirmParams' },
      result: { $ref: '#/$defs/DialogConfirmResult' }
    },
    'notification.show': {
      permission: 'notification',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/NotificationShowParams' },
      result: { $ref: '#/$defs/NotificationShowResult' }
    },
    'clipboard.read': {
      permission: 'clipboard',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/Empty' },
      result: { type: 'string' }
    },
    'clipboard.write': {
      permission: 'clipboard',
      contexts: ['renderer', 'backend'],
      params: {
        type: 'object',
        additionalProperties: false,
        required: ['text'],
        properties: { text: { type: 'string', maxLength: 32768 } }
      },
      result: { type: 'boolean' }
    },
    'data.directory': {
      permission: null,
      contexts: ['backend'],
      params: { $ref: '#/$defs/Empty' },
      result: { type: 'string', minLength: 1, maxLength: 32767 }
    },
    'resources.list': {
      permission: null,
      contexts: ['backend'],
      params: { $ref: '#/$defs/Empty' },
      result: { type: 'array', items: { $ref: '#/$defs/ResourceMount' }, maxItems: 64 }
    },
    'files.choose': {
      permission: 'dialog',
      contexts: ['renderer', 'backend'],
      params: {
        type: 'object',
        additionalProperties: false,
        required: ['type'],
        properties: {
          type: { enum: ['file', 'folder'] },
          multiple: { type: 'boolean' },
          extensions: {
            type: 'array',
            items: { type: 'string', minLength: 1, maxLength: 20 },
            maxItems: 64
          }
        }
      },
      result: {
        oneOf: [
          { type: 'null' },
          { type: 'array', items: { $ref: '#/$defs/FileReference' }, maxItems: 128 }
        ]
      }
    },
    'files.read': {
      permission: 'files:read',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/FileReadParams' },
      result: {
        type: 'object',
        additionalProperties: false,
        required: ['data', 'eof'],
        properties: { data: { type: 'string', maxLength: 32768 }, eof: { type: 'boolean' } }
      }
    },
    'files.resolve': {
      permission: 'files:read',
      contexts: ['backend'],
      params: { $ref: '#/$defs/FileResolveParams' },
      result: { type: 'string', minLength: 1, maxLength: 32767 }
    },
    'files.publish': {
      permission: 'files:publish',
      contexts: ['backend'],
      params: { $ref: '#/$defs/FilePublishParams' },
      result: { $ref: '#/$defs/FileReference' }
    },
    'tasks.start': {
      permission: 'tasks:control',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/TaskStartParams' },
      result: {
        type: 'object',
        additionalProperties: false,
        required: ['taskId'],
        properties: { taskId: { type: 'string', minLength: 1, maxLength: 128 } }
      }
    },
    'tasks.beginCommit': {
      permission: 'tasks:execute',
      contexts: ['backend'],
      params: {
        type: 'object',
        properties: { taskId: { type: 'string', minLength: 1, maxLength: 128 } },
        required: ['taskId'],
        additionalProperties: false
      },
      result: { type: 'boolean' }
    },
    'tasks.progress': {
      permission: 'tasks:execute',
      contexts: ['backend'],
      params: { $ref: '#/$defs/TaskProgressParams' },
      result: { type: 'boolean' }
    },
    'network.route': {
      permission: 'network:fetch',
      contexts: ['backend'],
      params: {
        type: 'object',
        properties: { url: { type: 'string', minLength: 1, maxLength: 8192 } },
        required: ['url'],
        additionalProperties: false
      },
      result: {
        type: 'object',
        properties: {
          kind: { enum: ['direct', 'proxy'] },
          url: { type: 'string', maxLength: 8192 }
        },
        required: ['kind'],
        additionalProperties: false
      }
    },
    'network.fetch': {
      permission: 'network:fetch',
      contexts: ['backend'],
      params: { $ref: '#/$defs/NetworkFetchParams' },
      result: { $ref: '#/$defs/NetworkFetchResult' }
    },
    'tasks.pause': {
      permission: 'tasks:control',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/TasksGetParams' },
      result: { type: 'boolean' }
    },
    'tasks.resume': {
      permission: 'tasks:control',
      contexts: ['renderer', 'backend'],
      params: { $ref: '#/$defs/TasksGetParams' },
      result: { type: 'boolean' }
    },
    'tasks.controlState': {
      permission: 'tasks:execute',
      contexts: ['backend'],
      params: { $ref: '#/$defs/TasksControlStateParams' },
      result: {
        type: 'object',
        properties: { paused: { type: 'boolean' }, cancelRequested: { type: 'boolean' } },
        required: ['paused', 'cancelRequested'],
        additionalProperties: false
      }
    }
  },
  lifecycle: {
    backend: 'on-demand',
    background: 'only-when-declared',
    pageClose: 'detach',
    disable: 'stop-tasks-and-process-tree-before-commit',
    upgrade: 'stop-old-before-swap',
    uninstall: 'retain-data-and-stop-before-removal',
    shutdown: 'stop-process-trees',
    restart: 'interrupt-without-replay',
    cancel: 'request-intent-then-confirm-executor-stopped'
  },
  trust: {
    native: 'trusted-local-code',
    sandbox: false,
    signature: 'ed25519-detached-raw-manifest',
    unsigned: 'explicit-local-install-decision',
    publisher: 'verified-key-policy-not-display-name'
  },
  methodsPolicy:
    'Only platform methods are enumerated. Backend and declared service methods are opaque to the host.',
  transport: {
    framing: 'uint32-big-endian-length-prefixed-utf8-json',
    stdout: 'protocol-only',
    stderr: 'diagnostics',
    handshake: ['hello', 'initialize', 'ready'],
    session: 'host-generated-256-bit-token',
    backendInvocation: 'host-replaces-session-and-derives-callerId',
    taskCompletion: 'correlated-reply-after-executor-stops',
    cancellation: 'cancel-frame-then-stop-process-tree-at-deadline',
    resourceMounts: 'validated-installed-versions-only'
  },
  install: {
    layout: 'versioned-code-and-resources-stable-data',
    codeDirectory: '.cruciblebox-code',
    reservedPaths: [
      'plugin.json',
      'resource.json',
      'signature.json',
      '.openbox-host-transaction.json',
      '.openbox-host-transaction.json.pending',
      '.cruciblebox-code'
    ],
    approval: 'immutable-prepared-package-token',
    publisherChange: 'explicit-approval',
    nativeCode: 'explicit-approval',
    permissionExpansion: 'explicit-approval',
    failure: 'preserve-old-install-and-user-data',
    resourceImport: 'user-initiated-local-offline-package'
  }
}
