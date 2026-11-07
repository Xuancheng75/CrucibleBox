# Next 3.0 公共 API 索引

由 contracts/next2/contract.json 生成；修改契约后运行 npm run generate:next2。

Manifest 7、API 7、wire 5；JS SDK 7.0.0-beta.2。基础最低宿主 2.2.0-beta.1，新增能力使用方按实际方法声明至少 2.2.0-beta.2。

完整字段、必需项与大小边界见 [契约 Schema](../contracts/next2/contract.json) 和 [TypeScript 类型](../packages/cruciblebox-next2-api/src/generated.d.mts)。本页不把业务 Json 载荷解释为宿主内建业务方法。

权限列为空仍需要合法会话、启用的安装身份及对应上下文。backend 并不自动具有任务权限；结果写入、下载和工具执行还要求有效任务归属。界面主题、对话框与通知由可信主窗口完成授权后的操作。

| 方法                       | 权限             | 上下文            | 参数类型                      | 返回类型                 |
| -------------------------- | ---------------- | ----------------- | ----------------------------- | ------------------------ |
| capabilities.resolve       | —                | renderer, backend | CapabilityResolveParams       | CapabilityBinding        |
| clipboard.read             | clipboard        | renderer, backend | Empty                         | string                   |
| clipboard.write            | clipboard        | renderer, backend | object                        | boolean                  |
| config.get                 | storage:read     | renderer, backend | ConfigGetParams               | ConfigGetResult          |
| config.patch               | storage:write    | renderer, backend | ConfigPatchParams             | ConfigPatchResult        |
| data.directory             | —                | backend           | Empty                         | string                   |
| dialog.confirm             | dialog           | renderer          | DialogConfirmParams           | DialogConfirmResult      |
| dialog.open                | dialog           | renderer          | DialogOpenParams              | DialogOpenResult         |
| events.poll                | events:subscribe | renderer, backend | EventSubscriptionParams       | EventPollResult          |
| events.publish             | events:publish   | backend           | EventPublishParams            | EventPublishResult       |
| events.subscribe           | events:subscribe | renderer, backend | EventTopicParams              | EventSubscription        |
| events.unsubscribe         | events:subscribe | renderer, backend | EventSubscriptionParams       | boolean                  |
| files.choose               | dialog           | renderer, backend | object                        | null / FileReference[]   |
| files.info                 | files:read       | renderer, backend | FileResolveParams             | FileInfo                 |
| files.list                 | files:read       | renderer, backend | FileResolveParams             | FileInfo[]               |
| files.publish              | files:publish    | backend           | FilePublishParams             | FileReference            |
| files.read                 | files:read       | renderer, backend | FileReadParams                | object                   |
| files.resolve              | files:read       | backend           | FileResolveParams             | string                   |
| files.stream.close         | files:read       | renderer, backend | FileStreamCloseParams         | boolean                  |
| files.stream.open          | files:read       | renderer, backend | FileResolveParams             | FileStream               |
| files.stream.read          | files:read       | renderer, backend | FileStreamReadParams          | FileStreamChunk          |
| files.writeResult          | files:publish    | backend           | FileWriteResultParams         | FileReference            |
| logging.write              | logging:write    | renderer, backend | LogWriteParams                | boolean                  |
| network.download           | network:download | backend           | NetworkDownloadParams         | FileReference            |
| network.fetch              | network:fetch    | backend           | NetworkFetchParams            | NetworkFetchResult       |
| network.route              | network:fetch    | backend           | object                        | object                   |
| notification.show          | notification     | renderer          | NotificationShowParams        | NotificationShowResult   |
| process.run                | process:run      | backend           | ProcessRunParams              | ProcessResult            |
| resources.list             | —                | backend           | Empty                         | ResourceMount[]          |
| rpc.cancel                 | —                | renderer          | RequestCancelParams           | boolean                  |
| runtime.list               | —                | renderer, backend | Empty                         | RuntimeStatus[]          |
| runtime.ping               | —                | renderer, backend | RuntimePingParams             | RuntimePingResult        |
| storage.batch              | storage:write    | renderer, backend | StorageBatchParams            | StorageBatchResult       |
| storage.delete             | storage:write    | renderer, backend | StorageDeleteParams           | StorageDeleteResult      |
| storage.get                | storage:read     | renderer, backend | StorageGetParams              | StorageGetResult         |
| storage.keys               | storage:read     | renderer, backend | StorageKeysParams             | StorageKeysResult        |
| storage.list               | storage:read     | renderer, backend | StorageListParams             | StorageListResult        |
| storage.read.begin         | storage:read     | renderer, backend | StorageReadBeginParams        | StorageReadBeginResult   |
| storage.read.chunk         | storage:read     | renderer, backend | StorageReadChunkParams        | StorageReadChunkResult   |
| storage.read.close         | storage:read     | renderer, backend | StorageReadCloseParams        | StorageReadCloseResult   |
| storage.set                | storage:write    | renderer, backend | StorageSetParams              | StorageSetResult         |
| storage.usage              | storage:read     | renderer, backend | Empty                         | StorageUsage             |
| storage.write.abort        | storage:write    | renderer, backend | StorageWriteAbortParams       | StorageWriteAbortResult  |
| storage.write.begin        | storage:write    | renderer, backend | StorageWriteBeginParams       | StorageWriteBeginResult  |
| storage.write.chunk        | storage:write    | renderer, backend | StorageWriteChunkParams       | StorageWriteChunkResult  |
| storage.write.commit       | storage:write    | renderer, backend | StorageWriteCommitParams      | StorageWriteCommitResult |
| tasks.beginCommit          | tasks:execute    | backend           | object                        | boolean                  |
| tasks.cancel               | tasks:control    | renderer, backend | TasksCancelParams             | TasksCancelResult        |
| tasks.controlState         | tasks:execute    | backend           | TasksControlStateParams       | object                   |
| tasks.get                  | tasks:read       | renderer, backend | TasksGetParams                | TasksGetResult           |
| tasks.list                 | tasks:read       | renderer, backend | TasksListParams               | TasksListResult          |
| tasks.pause                | tasks:control    | renderer, backend | TasksGetParams                | boolean                  |
| tasks.progress             | tasks:execute    | backend           | TaskProgressParams            | boolean                  |
| tasks.resume               | tasks:control    | renderer, backend | TasksGetParams                | boolean                  |
| tasks.start                | tasks:control    | renderer, backend | TaskStartParams               | object                   |
| theme.assets.get           | theme:assets     | renderer, backend | ThemeAssetGetParams           | ThemeAsset               |
| theme.assets.import        | theme:assets     | renderer, backend | ThemeAssetImportParams        | ThemeAsset               |
| theme.assets.importBundled | theme:assets     | renderer, backend | ThemeBundledAssetImportParams | ThemeAsset               |
| theme.assets.list          | theme:assets     | renderer, backend | ThemeAssetListParams          | ThemeAssetListResult     |
| theme.commit               | theme:write      | renderer          | ThemeCommitParams             | ThemeCommitResult        |
| theme.get                  | theme:read       | renderer          | ThemeGetParams                | ThemeGetResult           |
| theme.list                 | theme:read       | renderer          | ThemeListParams               | ThemeListResult          |
| theme.preview              | theme:write      | renderer          | ThemePreviewParams            | ThemePreviewResult       |
| theme.rollback             | theme:write      | renderer          | ThemeRollbackParams           | ThemeRollbackResult      |
| theme.set                  | theme:write      | renderer          | ThemeSetParams                | ThemeSetResult           |

业务通过 backend(method, params) 或声明的 service(id, method, params) 调用。成功响应为 ok:true/result，失败为 ok:false/error.code/error.message，requestId 必须关联原调用。示例见 [SDK 指南](plugin-sdk.md)、[Provider 指南](provider-guide.md) 与 [验证指南](testing.md)。
