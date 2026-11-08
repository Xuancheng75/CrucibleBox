# Next 3.0 公共 API 索引

由 contracts/next2/contract.json 生成；修改契约后运行 npm run generate:next2。

Manifest 7、API 7、wire 5；JS SDK 7.0.0-beta.3。基础最低宿主 2.2.0-beta.1；各方法最低宿主见表，既有方法的新选项按契约属性 since 字段声明。

完整字段、必需项与大小边界见 [契约 Schema](../contracts/next2/contract.json) 和 [TypeScript 类型](../packages/cruciblebox-next2-api/src/generated.d.mts)。本页不把业务 Json 载荷解释为宿主内建业务方法。

权限列为空仍需要合法会话、启用的安装身份及对应上下文。backend 并不自动具有任务权限；结果写入、下载和工具执行还要求有效任务归属。界面主题、对话框与通知由可信主窗口完成授权后的操作。

| 方法                       | 权限             | 上下文            | 参数类型                      | 返回类型                 | 最低宿主     |
| -------------------------- | ---------------- | ----------------- | ----------------------------- | ------------------------ | ------------ |
| capabilities.describe      | —                | renderer, backend | CapabilityResolveParams       | CapabilityDescription    | 2.2.0-beta.7 |
| capabilities.list          | —                | renderer, backend | CapabilityListParams          | CapabilityPage           | 2.2.0-beta.7 |
| capabilities.resolve       | —                | renderer, backend | CapabilityResolveParams       | CapabilityBinding        | 2.2.0-beta.1 |
| clipboard.read             | clipboard        | renderer, backend | Empty                         | string                   | 2.2.0-beta.1 |
| clipboard.write            | clipboard        | renderer, backend | object                        | boolean                  | 2.2.0-beta.1 |
| config.get                 | storage:read     | renderer, backend | ConfigGetParams               | ConfigGetResult          | 2.2.0-beta.1 |
| config.patch               | storage:write    | renderer, backend | ConfigPatchParams             | ConfigPatchResult        | 2.2.0-beta.1 |
| data.directory             | —                | backend           | Empty                         | string                   | 2.2.0-beta.1 |
| dialog.confirm             | dialog           | renderer          | DialogConfirmParams           | DialogConfirmResult      | 2.2.0-beta.1 |
| dialog.open                | dialog           | renderer          | DialogOpenParams              | DialogOpenResult         | 2.2.0-beta.1 |
| events.poll                | events:subscribe | renderer, backend | EventSubscriptionParams       | EventPollResult          | 2.2.0-beta.1 |
| events.publish             | events:publish   | backend           | EventPublishParams            | EventPublishResult       | 2.2.0-beta.1 |
| events.subscribe           | events:subscribe | renderer, backend | EventTopicParams              | EventSubscription        | 2.2.0-beta.1 |
| events.unsubscribe         | events:subscribe | renderer, backend | EventSubscriptionParams       | boolean                  | 2.2.0-beta.1 |
| files.child                | files:read       | renderer, backend | FileChildParams               | FileReference            | 2.2.0-beta.7 |
| files.choose               | dialog           | renderer, backend | object                        | null / FileReference[]   | 2.2.0-beta.1 |
| files.copy                 | files:write      | renderer, backend | FileTransferParams            | FileReference            | 2.2.0-beta.7 |
| files.directory.create     | files:write      | renderer, backend | FileDirectoryCreateParams     | FileReference            | 2.2.0-beta.7 |
| files.directory.walk       | files:read       | renderer, backend | FileWalkParams                | FileWalkPage             | 2.2.0-beta.7 |
| files.info                 | files:read       | renderer, backend | FileResolveParams             | FileInfo                 | 2.2.0-beta.2 |
| files.list                 | files:read       | renderer, backend | FileResolveParams             | FileInfo[]               | 2.2.0-beta.2 |
| files.move                 | files:write      | renderer, backend | FileTransferParams            | FileReference            | 2.2.0-beta.7 |
| files.publish              | files:publish    | backend           | FilePublishParams             | FileReference            | 2.2.0-beta.1 |
| files.read                 | files:read       | renderer, backend | FileReadParams                | object                   | 2.2.0-beta.1 |
| files.remove               | files:write      | renderer, backend | FileRemoveParams              | boolean                  | 2.2.0-beta.7 |
| files.resolve              | files:read       | backend           | FileResolveParams             | string                   | 2.2.0-beta.1 |
| files.stream.close         | files:read       | renderer, backend | FileStreamCloseParams         | boolean                  | 2.2.0-beta.1 |
| files.stream.open          | files:read       | renderer, backend | FileResolveParams             | FileStream               | 2.2.0-beta.1 |
| files.stream.read          | files:read       | renderer, backend | FileStreamReadParams          | FileStreamChunk          | 2.2.0-beta.1 |
| files.temporary            | files:write      | renderer, backend | FileTemporaryParams           | FileReference            | 2.2.0-beta.7 |
| files.temporary.release    | files:write      | renderer, backend | FileReferenceParams           | boolean                  | 2.2.0-beta.7 |
| files.watch                | files:read       | renderer, backend | FileWatchParams               | FileWatch                | 2.2.0-beta.7 |
| files.watch.close          | files:read       | renderer, backend | FileWatchIdParams             | boolean                  | 2.2.0-beta.7 |
| files.watch.poll           | files:read       | renderer, backend | FileWatchIdParams             | FileWatchPoll            | 2.2.0-beta.7 |
| files.write.abort          | files:write      | renderer, backend | FileWriteStream               | boolean                  | 2.2.0-beta.7 |
| files.write.append         | files:write      | renderer, backend | FileWriteChunkParams          | FileWriteChunkResult     | 2.2.0-beta.7 |
| files.write.commit         | files:write      | renderer, backend | FileWriteStream               | FileReference            | 2.2.0-beta.7 |
| files.write.open           | files:write      | renderer, backend | FileWriteOpenParams           | FileWriteStream          | 2.2.0-beta.7 |
| files.writeResult          | files:publish    | backend           | FileWriteResultParams         | FileReference            | 2.2.0-beta.2 |
| logging.write              | logging:write    | renderer, backend | LogWriteParams                | boolean                  | 2.2.0-beta.1 |
| network.download           | network:download | backend           | NetworkDownloadParams         | FileReference            | 2.2.0-beta.2 |
| network.fetch              | network:fetch    | backend           | NetworkFetchParams            | NetworkFetchResult       | 2.2.0-beta.1 |
| network.route              | network:fetch    | backend           | object                        | object                   | 2.2.0-beta.1 |
| notification.show          | notification     | renderer          | NotificationShowParams        | NotificationShowResult   | 2.2.0-beta.1 |
| process.run                | process:run      | backend           | ProcessRunParams              | ProcessResult            | 2.2.0-beta.2 |
| resources.list             | —                | backend           | Empty                         | ResourceMount[]          | 2.2.0-beta.1 |
| rpc.cancel                 | —                | renderer          | RequestCancelParams           | boolean                  | 2.2.0-beta.1 |
| runtime.list               | —                | renderer, backend | Empty                         | RuntimeStatus[]          | 2.2.0-beta.2 |
| runtime.ping               | —                | renderer, backend | RuntimePingParams             | RuntimePingResult        | 2.2.0-beta.1 |
| runtime.resolve            | —                | renderer, backend | CapabilityResolveParams       | CapabilityDescription    | 2.2.0-beta.7 |
| storage.batch              | storage:write    | renderer, backend | StorageBatchParams            | StorageBatchResult       | 2.2.0-beta.1 |
| storage.delete             | storage:write    | renderer, backend | StorageDeleteParams           | StorageDeleteResult      | 2.2.0-beta.1 |
| storage.get                | storage:read     | renderer, backend | StorageGetParams              | StorageGetResult         | 2.2.0-beta.1 |
| storage.keys               | storage:read     | renderer, backend | StorageKeysParams             | StorageKeysResult        | 2.2.0-beta.1 |
| storage.list               | storage:read     | renderer, backend | StorageListParams             | StorageListResult        | 2.2.0-beta.1 |
| storage.read.begin         | storage:read     | renderer, backend | StorageReadBeginParams        | StorageReadBeginResult   | 2.2.0-beta.1 |
| storage.read.chunk         | storage:read     | renderer, backend | StorageReadChunkParams        | StorageReadChunkResult   | 2.2.0-beta.1 |
| storage.read.close         | storage:read     | renderer, backend | StorageReadCloseParams        | StorageReadCloseResult   | 2.2.0-beta.1 |
| storage.set                | storage:write    | renderer, backend | StorageSetParams              | StorageSetResult         | 2.2.0-beta.1 |
| storage.usage              | storage:read     | renderer, backend | Empty                         | StorageUsage             | 2.2.0-beta.2 |
| storage.write.abort        | storage:write    | renderer, backend | StorageWriteAbortParams       | StorageWriteAbortResult  | 2.2.0-beta.1 |
| storage.write.begin        | storage:write    | renderer, backend | StorageWriteBeginParams       | StorageWriteBeginResult  | 2.2.0-beta.1 |
| storage.write.chunk        | storage:write    | renderer, backend | StorageWriteChunkParams       | StorageWriteChunkResult  | 2.2.0-beta.1 |
| storage.write.commit       | storage:write    | renderer, backend | StorageWriteCommitParams      | StorageWriteCommitResult | 2.2.0-beta.1 |
| tasks.beginCommit          | tasks:execute    | backend           | object                        | boolean                  | 2.2.0-beta.1 |
| tasks.cancel               | tasks:control    | renderer, backend | TasksCancelParams             | TasksCancelResult        | 2.2.0-beta.1 |
| tasks.controlState         | tasks:execute    | backend           | TasksControlStateParams       | object                   | 2.2.0-beta.1 |
| tasks.get                  | tasks:read       | renderer, backend | TasksGetParams                | TasksGetResult           | 2.2.0-beta.1 |
| tasks.list                 | tasks:read       | renderer, backend | TasksListParams               | TasksListResult          | 2.2.0-beta.1 |
| tasks.pause                | tasks:control    | renderer, backend | TasksGetParams                | boolean                  | 2.2.0-beta.1 |
| tasks.progress             | tasks:execute    | backend           | TaskProgressParams            | boolean                  | 2.2.0-beta.1 |
| tasks.publishNative        | native:publish   | backend           | NativePublicationParams       | NativePublicationResult  | 2.2.0-beta.7 |
| tasks.resume               | tasks:control    | renderer, backend | TasksGetParams                | boolean                  | 2.2.0-beta.1 |
| tasks.start                | tasks:control    | renderer, backend | TaskStartParams               | object                   | 2.2.0-beta.1 |
| theme.assets.get           | theme:assets     | renderer, backend | ThemeAssetGetParams           | ThemeAsset               | 2.2.0-beta.1 |
| theme.assets.import        | theme:assets     | renderer, backend | ThemeAssetImportParams        | ThemeAsset               | 2.2.0-beta.1 |
| theme.assets.importBundled | theme:assets     | renderer, backend | ThemeBundledAssetImportParams | ThemeAsset               | 2.2.0-beta.1 |
| theme.assets.list          | theme:assets     | renderer, backend | ThemeAssetListParams          | ThemeAssetListResult     | 2.2.0-beta.1 |
| theme.commit               | theme:write      | renderer          | ThemeCommitParams             | ThemeCommitResult        | 2.2.0-beta.1 |
| theme.get                  | theme:read       | renderer          | ThemeGetParams                | ThemeGetResult           | 2.2.0-beta.1 |
| theme.list                 | theme:read       | renderer          | ThemeListParams               | ThemeListResult          | 2.2.0-beta.1 |
| theme.preview              | theme:write      | renderer          | ThemePreviewParams            | ThemePreviewResult       | 2.2.0-beta.1 |
| theme.rollback             | theme:write      | renderer          | ThemeRollbackParams           | ThemeRollbackResult      | 2.2.0-beta.1 |
| theme.set                  | theme:write      | renderer          | ThemeSetParams                | ThemeSetResult           | 2.2.0-beta.1 |

业务通过 backend(method, params) 或声明的 service(id, method, params) 调用。成功响应为 ok:true/result，失败为 ok:false/error.code/error.message，requestId 必须关联原调用。示例见 [SDK 指南](plugin-sdk.md)、[Provider 指南](provider-guide.md) 与 [验证指南](testing.md)。
