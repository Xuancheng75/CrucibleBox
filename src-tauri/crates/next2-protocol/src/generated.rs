// Generated from contracts/next2/contract.json.
pub const CONTRACT_JSON: &str = r###"{"status":"candidate","versions":{"manifestVersion":7,"sdkApiVersion":7,"sdkPackageVersion":"7.0.0-beta.1","minHostVersion":"2.2.0-beta.1","wireVersion":5,"dataSchemaVersion":1,"resourceManifestVersion":1,"catalogVersion":1},"budget":{"eventBytes":8192,"eventSubscriptions":128,"eventSubscriptionsPerSession":16,"eventQueue":32,"fileStreams":64,"fileStreamsPerSession":8,"fileStreamChunkBytes":24576,"capabilityDescriptorBytes":32768,"capabilityDescriptorNodes":8192,"bytes":65536,"depth":32,"nodes":16384,"inflight":32,"leaseMs":300000,"requestTimeoutMs":30000,"interactiveTimeoutMs":300000,"handshakeTimeoutMs":10000,"stopTimeoutMs":5000,"packageFiles":5000,"packageBytes":629145600,"resourceFiles":20000,"resourceBytes":17179869184,"manifestBytes":2097152,"resourceManifestBytes":8388608,"manifestNodes":200000,"sessionRequests":4096,"storageValueBytes":4194304,"storageTransactionBytes":8388608,"storageChunkBytes":24576,"storageTransactionOps":32},"$defs":{"Json":{},"Empty":{"type":"object","additionalProperties":false,"required":[],"properties":{}},"VersionRange":{"type":"object","additionalProperties":false,"required":["min","maxExclusive"],"properties":{"min":{"type":"string","minLength":1,"maxLength":100,"format":"semver-without-build"},"maxExclusive":{"type":"string","minLength":1,"maxLength":100,"format":"semver-without-build"}}},"ApiRange":{"type":"object","additionalProperties":false,"required":["min","maxExclusive"],"properties":{"min":{"type":"integer","const":7},"maxExclusive":{"type":"integer","const":8}}},"Publisher":{"type":"object","additionalProperties":false,"required":["id","displayName"],"properties":{"id":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"displayName":{"type":"string","minLength":1,"maxLength":200}}},"FileEntry":{"type":"object","additionalProperties":false,"required":["path","bytes","sha256"],"properties":{"path":{"type":"string","minLength":1,"maxLength":240,"format":"package-path"},"bytes":{"type":"integer","minimum":0,"maximum":17179869184},"sha256":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[a-f0-9]{64}$"}}},"JsBackend":{"type":"object","additionalProperties":false,"required":["kind","entry"],"properties":{"kind":{"const":"js"},"entry":{"type":"string","minLength":1,"maxLength":240,"format":"package-path"}}},"NativeBackend":{"type":"object","additionalProperties":false,"required":["kind","entry","os","arch","trust"],"properties":{"kind":{"const":"native"},"entry":{"type":"string","minLength":1,"maxLength":240,"format":"package-path"},"os":{"const":"windows"},"arch":{"const":"x86_64"},"trust":{"const":"trusted-local-code"}}},"Backend":{"oneOf":[{"$ref":"#/$defs/JsBackend"},{"$ref":"#/$defs/NativeBackend"}]},"Navigation":{"type":"object","additionalProperties":false,"required":["title"],"properties":{"title":{"type":"string","minLength":1,"maxLength":100},"icon":{"type":"string","minLength":1,"maxLength":240,"format":"package-path"}}},"Command":{"type":"object","additionalProperties":false,"required":["id","title","method"],"properties":{"id":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"title":{"type":"string","minLength":1,"maxLength":100},"method":{"type":"string","minLength":1,"maxLength":128}}},"Contributions":{"type":"object","additionalProperties":false,"required":["commands"],"properties":{"navigation":{"$ref":"#/$defs/Navigation"},"commands":{"type":"array","items":{"$ref":"#/$defs/Command"},"maxItems":64},"fileHandlers":{"type":"array","maxItems":16,"items":{"$ref":"#/$defs/FileHandler"}}}},"LegacyServiceMethod":{"type":"string","minLength":1,"maxLength":128},"CapabilitySchema":{"oneOf":[{"type":"object","additionalProperties":false,"required":["type"],"properties":{"type":{"enum":["null","boolean"]}}},{"type":"object","additionalProperties":false,"required":["type"],"properties":{"type":{"enum":["integer","number"]},"minimum":{"type":"number"},"maximum":{"type":"number"}}},{"type":"object","additionalProperties":false,"required":["type"],"properties":{"type":{"const":"string"},"minLength":{"type":"integer","minimum":0,"maximum":65536},"maxLength":{"type":"integer","minimum":0,"maximum":65536}}},{"type":"object","additionalProperties":false,"required":["type","items"],"properties":{"type":{"const":"array"},"items":{"$ref":"#/$defs/CapabilitySchema"},"minItems":{"type":"integer","minimum":0,"maximum":65536},"maxItems":{"type":"integer","minimum":0,"maximum":65536}}},{"type":"object","additionalProperties":false,"required":["type","properties","additionalProperties"],"properties":{"type":{"const":"object"},"properties":{"type":"object","additionalProperties":{"$ref":"#/$defs/CapabilitySchema"}},"required":{"type":"array","items":{"type":"string","minLength":1,"maxLength":128},"uniqueItems":true},"additionalProperties":{"const":false}}}]},"CapabilityMethod":{"type":"object","additionalProperties":false,"required":["name","params","result"],"properties":{"name":{"$ref":"#/$defs/LegacyServiceMethod"},"params":{"$ref":"#/$defs/CapabilitySchema"},"result":{"$ref":"#/$defs/CapabilitySchema"},"contexts":{"type":"array","items":{"enum":["renderer","backend"]},"minItems":1,"maxItems":2,"uniqueItems":true}}},"ServiceMethod":{"oneOf":[{"$ref":"#/$defs/LegacyServiceMethod"},{"$ref":"#/$defs/CapabilityMethod"}]},"CapabilityEvent":{"type":"object","additionalProperties":false,"required":["name","payload"],"properties":{"name":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"payload":{"$ref":"#/$defs/CapabilitySchema"}}},"EventTopicParams":{"type":"object","additionalProperties":false,"required":["capabilityId","event"],"properties":{"capabilityId":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"event":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"}}},"EventPublishParams":{"type":"object","additionalProperties":false,"required":["capabilityId","event","payload"],"properties":{"capabilityId":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"event":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"payload":{}}},"EventSubscriptionParams":{"type":"object","additionalProperties":false,"required":["subscriptionId"],"properties":{"subscriptionId":{"type":"string","minLength":64,"maxLength":64,"pattern":"^[a-f0-9]{64}$"}}},"EventSubscription":{"type":"object","additionalProperties":false,"required":["subscriptionId","providerId"],"properties":{"subscriptionId":{"type":"string","minLength":64,"maxLength":64,"pattern":"^[a-f0-9]{64}$"},"providerId":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"}}},"EventMessage":{"type":"object","additionalProperties":false,"required":["sequence","publisherId","capabilityId","event","payload"],"properties":{"sequence":{"type":"integer","minimum":1,"maximum":9007199254740991},"publisherId":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"capabilityId":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"event":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"payload":{}}},"EventPollResult":{"type":"object","additionalProperties":false,"required":["event","dropped"],"properties":{"event":{"oneOf":[{"$ref":"#/$defs/EventMessage"},{"type":"null"}]},"dropped":{"type":"integer","minimum":0,"maximum":9007199254740991}}},"EventPublishResult":{"type":"object","additionalProperties":false,"required":["delivered"],"properties":{"delivered":{"type":"integer","minimum":0,"maximum":128}}},"RequestCancelParams":{"type":"object","additionalProperties":false,"required":["requestId"],"properties":{"requestId":{"type":"string","minLength":1,"maxLength":128}}},"FileStream":{"type":"object","additionalProperties":false,"required":["streamId","bytes"],"properties":{"streamId":{"type":"string","minLength":64,"maxLength":64,"pattern":"^[a-f0-9]{64}$"},"bytes":{"type":"integer","minimum":0,"maximum":17179869184}}},"FileStreamCloseParams":{"type":"object","additionalProperties":false,"required":["streamId"],"properties":{"streamId":{"type":"string","minLength":64,"maxLength":64,"pattern":"^[a-f0-9]{64}$"}}},"FileStreamReadParams":{"type":"object","additionalProperties":false,"required":["streamId","offset","length"],"properties":{"streamId":{"type":"string","minLength":64,"maxLength":64,"pattern":"^[a-f0-9]{64}$"},"offset":{"type":"integer","minimum":0,"maximum":17179869184},"length":{"type":"integer","minimum":1,"maximum":24576}}},"FileStreamChunk":{"type":"object","additionalProperties":false,"required":["data","offset","nextOffset","eof"],"properties":{"data":{"type":"string","maxLength":32768},"offset":{"type":"integer","minimum":0,"maximum":17179869184},"nextOffset":{"type":"integer","minimum":0,"maximum":17179869184},"eof":{"type":"boolean"}}},"ServiceExport":{"type":"object","additionalProperties":false,"required":["id","version","methods"],"properties":{"id":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"version":{"type":"string","minLength":1,"maxLength":100,"format":"semver-without-build"},"methods":{"type":"array","items":{"$ref":"#/$defs/ServiceMethod"},"maxItems":128},"events":{"type":"array","items":{"$ref":"#/$defs/CapabilityEvent"},"maxItems":64},"interchangeable":{"type":"boolean"}}},"ServiceDependency":{"type":"object","additionalProperties":false,"required":["id","version"],"properties":{"id":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"providerId":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"version":{"$ref":"#/$defs/VersionRange"},"optional":{"type":"boolean"}}},"CapabilityResolveParams":{"type":"object","additionalProperties":false,"required":["id"],"properties":{"id":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"}}},"CapabilityBinding":{"type":"object","additionalProperties":false,"required":["providerId","providerVersion","lifecycle","export"],"properties":{"providerId":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"providerVersion":{"type":"string","minLength":1,"maxLength":100,"format":"semver-without-build"},"lifecycle":{"enum":["on-demand","background"]},"export":{"$ref":"#/$defs/ServiceExport"}}},"LogWriteParams":{"type":"object","additionalProperties":false,"required":["level","message"],"properties":{"level":{"enum":["debug","info","warn","error"]},"message":{"type":"string","minLength":1,"maxLength":2048}}},"ResourceDependency":{"type":"object","additionalProperties":false,"required":["id","version"],"properties":{"id":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"version":{"$ref":"#/$defs/VersionRange"},"catalog":{"type":"string","minLength":1,"maxLength":240,"format":"package-path"},"optional":{"type":"boolean"}}},"ResourceDownload":{"type":"object","additionalProperties":false,"required":["path","url"],"properties":{"path":{"type":"string","minLength":1,"maxLength":240,"format":"package-path"},"url":{"type":"string","minLength":9,"maxLength":4096,"pattern":"^https://"}}},"ResourceRelease":{"type":"object","additionalProperties":false,"required":["id","title","description","manifest","downloads"],"properties":{"id":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"title":{"type":"string","minLength":1,"maxLength":200},"description":{"type":"string","maxLength":2000},"manifest":{"$ref":"#/$defs/ResourceManifest"},"downloads":{"type":"array","minItems":1,"maxItems":20000,"items":{"$ref":"#/$defs/ResourceDownload"}}}},"ResourceCatalog":{"type":"object","additionalProperties":false,"required":["resourceCatalogVersion","releases"],"properties":{"resourceCatalogVersion":{"const":1},"releases":{"type":"array","maxItems":32,"items":{"$ref":"#/$defs/ResourceRelease"}}}},"PlatformTarget":{"type":"object","additionalProperties":false,"required":["kind"],"properties":{"kind":{"const":"platform"}}},"BackendTarget":{"type":"object","additionalProperties":false,"required":["kind"],"properties":{"kind":{"const":"backend"}}},"ServiceTarget":{"type":"object","additionalProperties":false,"required":["kind","id"],"properties":{"kind":{"const":"service"},"id":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"}}},"Target":{"oneOf":[{"$ref":"#/$defs/PlatformTarget"},{"$ref":"#/$defs/BackendTarget"},{"$ref":"#/$defs/ServiceTarget"}]},"Request":{"type":"object","additionalProperties":false,"required":["wireVersion","requestId","session","target","method","params"],"properties":{"wireVersion":{"const":5},"requestId":{"type":"string","minLength":1,"maxLength":128},"session":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[a-f0-9]{64}$"},"target":{"$ref":"#/$defs/Target"},"method":{"type":"string","minLength":1,"maxLength":128},"params":{}}},"RpcError":{"type":"object","additionalProperties":false,"required":["code","message"],"properties":{"code":{"type":"string","minLength":1,"maxLength":100},"message":{"type":"string","minLength":1,"maxLength":2048}}},"Success":{"type":"object","additionalProperties":false,"required":["wireVersion","requestId","ok","result"],"properties":{"wireVersion":{"const":5},"requestId":{"type":"string","minLength":1,"maxLength":128},"ok":{"const":true},"result":{}}},"Failure":{"type":"object","additionalProperties":false,"required":["wireVersion","requestId","ok","error"],"properties":{"wireVersion":{"const":5},"requestId":{"type":"string","minLength":1,"maxLength":128},"ok":{"const":false},"error":{"$ref":"#/$defs/RpcError"}}},"Response":{"oneOf":[{"$ref":"#/$defs/Success"},{"$ref":"#/$defs/Failure"}]},"FileReference":{"type":"object","additionalProperties":false,"required":["ref","name","bytes"],"properties":{"ref":{"type":"string","minLength":1,"maxLength":128},"name":{"type":"string","minLength":1,"maxLength":240},"bytes":{"type":"integer","minimum":0,"maximum":17179869184}}},"FileReadParams":{"type":"object","additionalProperties":false,"required":["ref","offset","length"],"properties":{"ref":{"type":"string","minLength":1,"maxLength":128},"offset":{"type":"integer","minimum":0,"maximum":17179869184},"length":{"type":"integer","minimum":1,"maximum":24576}}},"FileResolveParams":{"type":"object","additionalProperties":false,"required":["ref"],"properties":{"ref":{"type":"string","minLength":1,"maxLength":128}}},"FilePublishParams":{"type":"object","additionalProperties":false,"required":["taskId","path","name"],"properties":{"taskId":{"type":"string","minLength":1,"maxLength":128},"path":{"type":"string","minLength":1,"maxLength":32767},"name":{"type":"string","minLength":1,"maxLength":240}}},"TaskStartParams":{"type":"object","additionalProperties":false,"required":["target","method","params"],"properties":{"target":{"$ref":"#/$defs/Target"},"method":{"type":"string","minLength":1,"maxLength":128},"params":{},"timeoutMs":{"type":"integer","minimum":100,"maximum":86400000}}},"TaskProgressParams":{"type":"object","additionalProperties":false,"required":["taskId","sequence","stage","percent","message"],"properties":{"taskId":{"type":"string","minLength":1,"maxLength":128},"sequence":{"type":"integer","minimum":1,"maximum":9007199254740991},"stage":{"type":"string","minLength":1,"maxLength":100},"percent":{"type":"integer","minimum":0,"maximum":100},"message":{"type":"string","minLength":1,"maxLength":2048}}},"BackendHello":{"type":"object","additionalProperties":false,"required":["kind","wireVersion","sdkApiVersion"],"properties":{"kind":{"const":"hello"},"wireVersion":{"const":5},"sdkApiVersion":{"const":7}}},"BackendInit":{"type":"object","additionalProperties":false,"required":["kind","wireVersion","session","pluginId","dataDirectory","resources"],"properties":{"kind":{"const":"initialize"},"wireVersion":{"const":5},"session":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[a-f0-9]{64}$"},"pluginId":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"dataDirectory":{"type":"string","minLength":1,"maxLength":32767},"resources":{"type":"array","items":{"$ref":"#/$defs/ResourceMount"},"maxItems":64}}},"ResourceMount":{"type":"object","additionalProperties":false,"required":["id","version","directory"],"properties":{"id":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"version":{"type":"string","minLength":1,"maxLength":100,"format":"semver-without-build"},"directory":{"type":"string","minLength":1,"maxLength":32767}}},"BackendReady":{"type":"object","additionalProperties":false,"required":["kind","wireVersion"],"properties":{"kind":{"const":"ready"},"wireVersion":{"const":5}}},"BackendExecute":{"type":"object","additionalProperties":false,"required":["kind","request","callerId"],"properties":{"kind":{"const":"execute"},"request":{"$ref":"#/$defs/Request"},"taskId":{"type":"string","minLength":1,"maxLength":128},"callerId":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"}}},"BackendReply":{"type":"object","additionalProperties":false,"required":["kind","response"],"properties":{"kind":{"const":"reply"},"response":{"$ref":"#/$defs/Response"}}},"BackendCapability":{"type":"object","additionalProperties":false,"required":["kind","request"],"properties":{"kind":{"const":"capability"},"request":{"$ref":"#/$defs/Request"}}},"BackendCapabilityReply":{"type":"object","additionalProperties":false,"required":["kind","response"],"properties":{"kind":{"const":"capability-result"},"response":{"$ref":"#/$defs/Response"}}},"BackendCancel":{"type":"object","additionalProperties":false,"required":["kind","wireVersion"],"properties":{"kind":{"const":"cancel"},"wireVersion":{"const":5},"taskId":{"type":"string","minLength":1,"maxLength":128},"requestId":{"type":"string","minLength":1,"maxLength":128}}},"BackendStop":{"type":"object","additionalProperties":false,"required":["kind","wireVersion"],"properties":{"kind":{"const":"stop"},"wireVersion":{"const":5}}},"BackendFrame":{"oneOf":[{"$ref":"#/$defs/BackendHello"},{"$ref":"#/$defs/BackendInit"},{"$ref":"#/$defs/BackendReady"},{"$ref":"#/$defs/BackendExecute"},{"$ref":"#/$defs/BackendReply"},{"$ref":"#/$defs/BackendCapability"},{"$ref":"#/$defs/BackendCapabilityReply"},{"$ref":"#/$defs/BackendCancel"},{"$ref":"#/$defs/BackendStop"}]},"RuntimePingParams":{"type":"object","properties":{},"required":[],"additionalProperties":false},"RuntimePingResult":{},"StorageGetParams":{"type":"object","properties":{"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["key"],"additionalProperties":false},"StorageGetResult":{},"StorageSetParams":{"type":"object","properties":{"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"value":{}},"required":["key","value"],"additionalProperties":false},"StorageSetResult":{},"StorageDeleteParams":{"type":"object","properties":{"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["key"],"additionalProperties":false},"StorageDeleteResult":{},"StorageBatchParams":{"type":"object","properties":{"operations":{"type":"array","minItems":1,"maxItems":64,"items":{"oneOf":[{"type":"object","properties":{"type":{"const":"set"},"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"value":{}},"required":["type","key","value"],"additionalProperties":false},{"type":"object","properties":{"type":{"const":"delete"},"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["type","key"],"additionalProperties":false}]}}},"required":["operations"],"additionalProperties":false},"StorageBatchResult":{},"StorageListParams":{"type":"object","properties":{"prefix":{"type":"string","minLength":0,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]*$"},"limit":{"type":"integer","minimum":1,"maximum":100},"after":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["prefix","limit"],"additionalProperties":false},"StorageListResult":{"type":"object","properties":{"items":{"type":"array","maxItems":100,"items":{"type":"object","properties":{"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"value":{}},"required":["key","value"],"additionalProperties":false}},"nextCursor":{"oneOf":[{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},{"type":"null"}]}},"required":["items","nextCursor"],"additionalProperties":false},"StorageKeysParams":{"type":"object","properties":{"prefix":{"type":"string","minLength":0,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]*$"},"limit":{"type":"integer","minimum":1,"maximum":100},"after":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["prefix","limit"],"additionalProperties":false},"StorageKeysResult":{"type":"object","properties":{"items":{"type":"array","maxItems":100,"items":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"nextCursor":{"oneOf":[{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},{"type":"null"}]}},"required":["items","nextCursor"],"additionalProperties":false},"StorageReadBeginParams":{"type":"object","properties":{"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["key"],"additionalProperties":false},"StorageReadBeginResult":{"type":"object","properties":{"found":{"type":"boolean"},"readId":{"oneOf":[{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"},{"type":"null"}]},"byteLength":{"type":"integer","minimum":0,"maximum":4194304}},"required":["found","readId","byteLength"],"additionalProperties":false},"StorageReadChunkParams":{"type":"object","properties":{"readId":{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"},"offset":{"type":"integer","minimum":0,"maximum":4194304}},"required":["readId","offset"],"additionalProperties":false},"StorageReadChunkResult":{"type":"object","properties":{"offset":{"type":"integer","minimum":0,"maximum":4194304},"data":{"type":"string","minLength":1,"maxLength":32768,"pattern":"^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$"}},"required":["offset","data"],"additionalProperties":false},"StorageReadCloseParams":{"type":"object","properties":{"readId":{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"}},"required":["readId"],"additionalProperties":false},"StorageReadCloseResult":{},"StorageWriteBeginParams":{"type":"object","properties":{"writes":{"type":"array","maxItems":32,"items":{"type":"object","properties":{"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"byteLength":{"type":"integer","minimum":1,"maximum":4194304}},"required":["key","byteLength"],"additionalProperties":false}},"deletes":{"type":"array","maxItems":32,"items":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}}},"required":["writes","deletes"],"additionalProperties":false},"StorageWriteBeginResult":{"type":"object","properties":{"transactionId":{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"}},"required":["transactionId"],"additionalProperties":false},"StorageWriteChunkParams":{"type":"object","properties":{"transactionId":{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"},"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"offset":{"type":"integer","minimum":0,"maximum":4194304},"data":{"type":"string","minLength":1,"maxLength":32768,"pattern":"^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$"}},"required":["transactionId","key","offset","data"],"additionalProperties":false},"StorageWriteChunkResult":{"type":"object","properties":{"receivedBytes":{"type":"integer","minimum":1,"maximum":8388608}},"required":["receivedBytes"],"additionalProperties":false},"StorageWriteCommitParams":{"type":"object","properties":{"transactionId":{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"}},"required":["transactionId"],"additionalProperties":false},"StorageWriteCommitResult":{},"StorageWriteAbortParams":{"type":"object","properties":{"transactionId":{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"}},"required":["transactionId"],"additionalProperties":false},"StorageWriteAbortResult":{},"ConfigGetParams":{"type":"object","properties":{},"required":[],"additionalProperties":false},"ConfigGetResult":{},"ConfigPatchParams":{"type":"object","properties":{"values":{"type":"object","additionalProperties":true}},"required":["values"],"additionalProperties":false},"ConfigPatchResult":{},"TasksGetParams":{"type":"object","properties":{"taskId":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["taskId"],"additionalProperties":false},"TasksGetResult":{},"TasksControlStateParams":{"type":"object","properties":{"taskId":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"executionPaused":{"type":"boolean"}},"required":["taskId"],"additionalProperties":false},"TasksCancelParams":{"type":"object","properties":{"taskId":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["taskId"],"additionalProperties":false},"TasksCancelResult":{},"TasksListParams":{"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":20},"after":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["limit"],"additionalProperties":false},"TasksListResult":{},"ThemeGetParams":{"type":"object","properties":{},"required":[],"additionalProperties":false},"ThemeGetResult":{},"ThemeListParams":{"type":"object","properties":{},"required":[],"additionalProperties":false},"ThemeListResult":{},"ThemePreviewParams":{"type":"object","properties":{"theme":{"$ref":"#/$defs/ThemeDefinition"}},"required":["theme"],"additionalProperties":false},"ThemePreviewResult":{},"ThemeCommitParams":{"type":"object","properties":{},"required":[],"additionalProperties":false},"ThemeCommitResult":{},"ThemeRollbackParams":{"type":"object","properties":{},"required":[],"additionalProperties":false},"ThemeRollbackResult":{},"ThemeSetParams":{"type":"object","properties":{"theme":{"$ref":"#/$defs/ThemeDefinition"}},"required":["theme"],"additionalProperties":false},"ThemeSetResult":{},"DialogOpenParams":{"type":"object","properties":{"type":{"enum":["file","folder"]},"multiple":{"type":"boolean"},"extensions":{"type":"array","maxItems":32,"items":{"type":"string","minLength":1,"maxLength":16,"pattern":"^[A-Za-z0-9]+$"}}},"required":["type"],"additionalProperties":false},"DialogOpenResult":{},"DialogConfirmParams":{"type":"object","properties":{"title":{"type":"string","minLength":1,"maxLength":200},"message":{"type":"string","minLength":1,"maxLength":4000},"confirmLabel":{"type":"string","minLength":1,"maxLength":80},"cancelLabel":{"type":"string","minLength":1,"maxLength":80}},"required":["title","message"],"additionalProperties":false},"DialogConfirmResult":{},"NotificationShowParams":{"type":"object","properties":{"title":{"type":"string","minLength":1,"maxLength":200},"body":{"type":"string","maxLength":4000}},"required":["title","body"],"additionalProperties":false},"NotificationShowResult":{},"PluginDataMigration":{"type":"object","additionalProperties":false,"required":["fromPluginVersion","summary"],"properties":{"fromPluginVersion":{"$ref":"#/$defs/VersionRange"},"summary":{"type":"string","minLength":1,"maxLength":1000}}},"Manifest":{"type":"object","additionalProperties":false,"required":["manifestVersion","sdkApiVersion","id","version","displayName","description","publisher","minHostVersion","sdk","permissions","background","contributes","exports","dependencies","resources","files"],"properties":{"manifestVersion":{"const":7},"sdkApiVersion":{"const":7},"id":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"version":{"type":"string","minLength":1,"maxLength":100,"format":"semver-without-build"},"displayName":{"type":"string","minLength":1,"maxLength":100},"description":{"type":"string","maxLength":2000},"publisher":{"$ref":"#/$defs/Publisher"},"minHostVersion":{"type":"string","minLength":1,"maxLength":100,"format":"semver-without-build"},"dataMigration":{"$ref":"#/$defs/PluginDataMigration"},"sdk":{"$ref":"#/$defs/ApiRange"},"renderer":{"type":"string","minLength":1,"maxLength":240,"format":"package-path"},"backend":{"$ref":"#/$defs/Backend"},"permissions":{"type":"array","items":{"enum":["storage:read","logging:write","events:publish","events:subscribe","storage:write","tasks:read","tasks:control","theme:read","theme:write","theme:assets","dialog","notification","clipboard","files:read","files:publish","tasks:execute","browser:downloads","network:fetch"]},"maxItems":128,"uniqueItems":true},"background":{"type":"boolean"},"backgroundIntervalMs":{"type":"integer","minimum":1000,"maximum":60000},"contributes":{"$ref":"#/$defs/Contributions"},"exports":{"type":"array","items":{"$ref":"#/$defs/ServiceExport"},"maxItems":64},"dependencies":{"type":"array","items":{"$ref":"#/$defs/ServiceDependency"},"maxItems":64},"resources":{"type":"array","items":{"$ref":"#/$defs/ResourceDependency"},"maxItems":64},"files":{"type":"array","items":{"$ref":"#/$defs/FileEntry"},"maxItems":5000,"minItems":1},"config":{"type":"object","additionalProperties":true}}},"ResourceManifest":{"type":"object","additionalProperties":false,"required":["resourceManifestVersion","id","pluginId","version","kind","containsNativeCode","source","license","pluginVersion","os","arch","files","publisher"],"properties":{"resourceManifestVersion":{"const":1},"id":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"pluginId":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"version":{"type":"string","minLength":1,"maxLength":100,"format":"semver-without-build"},"kind":{"enum":["model","runtime","data"]},"containsNativeCode":{"type":"boolean"},"source":{"type":"string","minLength":1,"maxLength":2048},"license":{"type":"string","minLength":1,"maxLength":2000},"pluginVersion":{"$ref":"#/$defs/VersionRange"},"os":{"const":"windows"},"arch":{"const":"x86_64"},"files":{"type":"array","items":{"$ref":"#/$defs/FileEntry"},"maxItems":20000,"minItems":1},"publisher":{"$ref":"#/$defs/Publisher"}}},"RendererAppearance":{"type":"object","properties":{"mode":{"enum":["light","dark"]},"cssVars":{"type":"object","properties":{"--ob-mode":{"type":"string","maxLength":512},"--ob-theme-id":{"type":"string","maxLength":512},"--ob-color-primary-contrast":{"type":"string","maxLength":512},"--ob-color-success-border":{"type":"string","maxLength":512},"--ob-color-warning-border":{"type":"string","maxLength":512},"--ob-color-error-border":{"type":"string","maxLength":512},"--ob-color-bg":{"type":"string","maxLength":512},"--ob-color-bg-layout":{"type":"string","maxLength":512},"--ob-color-bg-container":{"type":"string","maxLength":512},"--ob-color-bg-elevated":{"type":"string","maxLength":512},"--ob-color-primary":{"type":"string","maxLength":512},"--ob-color-primary-hover":{"type":"string","maxLength":512},"--ob-color-primary-bg":{"type":"string","maxLength":512},"--ob-color-text":{"type":"string","maxLength":512},"--ob-color-text-secondary":{"type":"string","maxLength":512},"--ob-color-text-tertiary":{"type":"string","maxLength":512},"--ob-color-border":{"type":"string","maxLength":512},"--ob-color-border-secondary":{"type":"string","maxLength":512},"--ob-color-success":{"type":"string","maxLength":512},"--ob-color-success-bg":{"type":"string","maxLength":512},"--ob-color-warning":{"type":"string","maxLength":512},"--ob-color-warning-bg":{"type":"string","maxLength":512},"--ob-color-error":{"type":"string","maxLength":512},"--ob-color-error-bg":{"type":"string","maxLength":512},"--ob-color-link":{"type":"string","maxLength":512},"--ob-radius":{"type":"string","maxLength":512},"--ob-font-family":{"type":"string","maxLength":512},"--ob-colorBg":{"type":"string","maxLength":512},"--ob-colorBgLayout":{"type":"string","maxLength":512},"--ob-colorBgContainer":{"type":"string","maxLength":512},"--ob-colorBgElevated":{"type":"string","maxLength":512},"--ob-colorPrimary":{"type":"string","maxLength":512},"--ob-colorPrimaryHover":{"type":"string","maxLength":512},"--ob-colorPrimaryBg":{"type":"string","maxLength":512},"--ob-colorText":{"type":"string","maxLength":512},"--ob-colorTextSecondary":{"type":"string","maxLength":512},"--ob-colorTextTertiary":{"type":"string","maxLength":512},"--ob-colorBorder":{"type":"string","maxLength":512},"--ob-colorBorderSecondary":{"type":"string","maxLength":512},"--ob-colorSuccess":{"type":"string","maxLength":512},"--ob-colorSuccessBg":{"type":"string","maxLength":512},"--ob-colorWarning":{"type":"string","maxLength":512},"--ob-colorWarningBg":{"type":"string","maxLength":512},"--ob-colorError":{"type":"string","maxLength":512},"--ob-colorErrorBg":{"type":"string","maxLength":512},"--ob-colorLink":{"type":"string","maxLength":512},"--ob-borderRadius":{"type":"string","maxLength":512},"--ob-fontFamily":{"type":"string","maxLength":512}},"required":["--ob-mode","--ob-theme-id","--ob-color-primary-contrast","--ob-color-success-border","--ob-color-warning-border","--ob-color-error-border","--ob-color-bg","--ob-color-bg-layout","--ob-color-bg-container","--ob-color-bg-elevated","--ob-color-primary","--ob-color-primary-hover","--ob-color-primary-bg","--ob-color-text","--ob-color-text-secondary","--ob-color-text-tertiary","--ob-color-border","--ob-color-border-secondary","--ob-color-success","--ob-color-success-bg","--ob-color-warning","--ob-color-warning-bg","--ob-color-error","--ob-color-error-bg","--ob-color-link","--ob-radius","--ob-font-family","--ob-colorBg","--ob-colorBgLayout","--ob-colorBgContainer","--ob-colorBgElevated","--ob-colorPrimary","--ob-colorPrimaryHover","--ob-colorPrimaryBg","--ob-colorText","--ob-colorTextSecondary","--ob-colorTextTertiary","--ob-colorBorder","--ob-colorBorderSecondary","--ob-colorSuccess","--ob-colorSuccessBg","--ob-colorWarning","--ob-colorWarningBg","--ob-colorError","--ob-colorErrorBg","--ob-colorLink","--ob-borderRadius","--ob-fontFamily"],"additionalProperties":false}},"required":["mode","cssVars"],"additionalProperties":false},"TaskSnapshot":{"type":"object","properties":{"taskId":{"type":"string","minLength":1,"maxLength":128},"status":{"enum":["queued","running","paused","succeeded","failed","cancelled","interrupted"]},"sequence":{"type":"integer","minimum":0,"maximum":9007199254740991},"cancelRequested":{"type":"boolean"},"resultRefs":{"type":"array","maxItems":32,"items":{"type":"string","maxLength":2048}},"resourceKey":{"type":"string","maxLength":256},"error":{},"progress":{},"publication":{},"checkpoint":{},"initiatorId":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"providerId":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"}},"required":["taskId","status","sequence","cancelRequested","resultRefs","resourceKey","initiatorId","providerId"],"additionalProperties":true},"SignatureRecord":{"type":"object","additionalProperties":false,"properties":{"signatureVersion":{"const":1},"publisherId":{"type":"string","minLength":1,"maxLength":64,"format":"plugin-id"},"keyId":{"type":"string","minLength":1,"maxLength":100},"signature":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9+/]{86}==$"}},"required":["signatureVersion","publisherId","keyId","signature"]},"NetworkFetchParams":{"type":"object","additionalProperties":false,"properties":{"url":{"type":"string","minLength":1,"maxLength":2048},"method":{"enum":["GET","POST","PUT","PATCH","DELETE","HEAD"]},"headers":{"type":"object","additionalProperties":true},"body":{"type":"string","maxLength":32768}},"required":["url","method"]},"NetworkFetchResult":{"type":"object","additionalProperties":false,"properties":{"status":{"type":"integer","minimum":100,"maximum":599},"body":{"type":"string","maxLength":32768},"headers":{"type":"object","additionalProperties":true}},"required":["status","body","headers"]},"FileHandler":{"type":"object","additionalProperties":false,"required":["extensions"],"properties":{"extensions":{"type":"array","minItems":1,"maxItems":64,"uniqueItems":true,"items":{"type":"string","minLength":1,"maxLength":20,"pattern":"^[a-z0-9]+$"}}}},"ThemeLayout":{"type":"object","properties":{"navigation":{"enum":["rail","top","menu"]},"density":{"enum":["comfortable","compact"]},"sidebarWidth":{"type":"integer","minimum":144,"maximum":360},"contentPadding":{"type":"integer","minimum":0,"maximum":48},"contentMaxWidth":{"type":"integer","minimum":640,"maximum":2560},"cardGap":{"type":"integer","minimum":4,"maximum":40}},"required":[],"additionalProperties":false},"ThemeComponents":{"type":"object","properties":{"buttonRadius":{"type":"integer","minimum":0,"maximum":32},"cardRadius":{"type":"integer","minimum":0,"maximum":40},"inputRadius":{"type":"integer","minimum":0,"maximum":24},"borderWidth":{"type":"integer","minimum":0,"maximum":6},"surfaceShadow":{"enum":["none","soft","hard"]},"buttonSkew":{"type":"number","minimum":-12,"maximum":12},"headingWeight":{"type":"integer","minimum":400,"maximum":900}},"required":[],"additionalProperties":false},"ThemeMotion":{"type":"object","properties":{"level":{"enum":["full","reduced","off"]},"transition":{"enum":["none","fade","slide"]},"durationMs":{"type":"integer","minimum":0,"maximum":600}},"required":[],"additionalProperties":false},"ThemePoint":{"type":"object","properties":{"x":{"type":"number","minimum":0,"maximum":100},"y":{"type":"number","minimum":0,"maximum":100}},"required":["x","y"],"additionalProperties":false},"ThemeLayer":{"type":"object","properties":{"id":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[a-zA-Z][a-zA-Z0-9_-]*$"},"type":{"enum":["shape","image","text"]},"scope":{"enum":["shell","home","workspace"]},"x":{"type":"number","minimum":-100,"maximum":100},"y":{"type":"number","minimum":-100,"maximum":100},"width":{"type":"number","minimum":1,"maximum":200},"height":{"type":"number","minimum":1,"maximum":200},"rotation":{"type":"number","minimum":-180,"maximum":180},"opacity":{"type":"number","minimum":0,"maximum":1},"order":{"type":"integer","minimum":0,"maximum":31},"color":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"shape":{"enum":["rectangle","ellipse","polygon"]},"points":{"type":"array","items":{"$ref":"#/$defs/ThemePoint"},"minItems":3,"maxItems":12},"text":{"type":"string","minLength":0,"maxLength":160},"assetId":{"type":"string","pattern":"^[a-f0-9]{64}$","minLength":64,"maxLength":64},"motion":{"enum":["none","float","tilt"]},"durationMs":{"type":"integer","minimum":200,"maximum":10000}},"required":["id","type","scope","x","y","width","height","rotation","opacity","order"],"additionalProperties":false},"ThemeFont":{"type":"object","properties":{"assetId":{"type":"string","pattern":"^[a-f0-9]{64}$","minLength":64,"maxLength":64},"family":{"type":"string","minLength":1,"maxLength":80,"pattern":"^[A-Za-z][A-Za-z0-9 _-]*$"}},"required":["assetId","family"],"additionalProperties":false},"ThemeDesign":{"type":"object","properties":{"version":{"const":2},"layout":{"$ref":"#/$defs/ThemeLayout"},"components":{"$ref":"#/$defs/ThemeComponents"},"motion":{"$ref":"#/$defs/ThemeMotion"},"layers":{"type":"array","items":{"$ref":"#/$defs/ThemeLayer"},"minItems":0,"maxItems":32},"assets":{"type":"array","items":{"type":"string","pattern":"^[a-f0-9]{64}$","minLength":64,"maxLength":64},"minItems":0,"maxItems":32,"uniqueItems":true},"fonts":{"type":"array","items":{"$ref":"#/$defs/ThemeFont"},"minItems":0,"maxItems":8},"css":{"type":"string","minLength":0,"maxLength":8192}},"required":["version"],"additionalProperties":false},"ThemeDefinition":{"type":"object","properties":{"id":{"type":"string","minLength":1,"maxLength":64},"name":{"type":"string","minLength":1,"maxLength":80},"mode":{"enum":["light","dark"]},"tokens":{"type":"object","properties":{"colorBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBgLayout":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBgContainer":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBgElevated":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorPrimary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorPrimaryHover":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorPrimaryBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorText":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorTextSecondary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorTextTertiary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBorder":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBorderSecondary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorSuccess":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorSuccessBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorWarning":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorWarningBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorError":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorErrorBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorLink":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"borderRadius":{"type":"number","minimum":0,"maximum":32},"fontFamily":{"type":"string","minLength":1,"maxLength":512,"pattern":"^[^;<>\\\\]+$"}},"required":["colorBg","colorBgLayout","colorBgContainer","colorBgElevated","colorPrimary","colorPrimaryHover","colorPrimaryBg","colorText","colorTextSecondary","colorTextTertiary","colorBorder","colorBorderSecondary","colorSuccess","colorSuccessBg","colorWarning","colorWarningBg","colorError","colorErrorBg","colorLink","borderRadius","fontFamily"],"additionalProperties":false},"design":{"$ref":"#/$defs/ThemeDesign"}},"required":["id","name","mode","tokens"],"additionalProperties":false},"ThemeAsset":{"type":"object","properties":{"id":{"type":"string","pattern":"^[a-f0-9]{64}$","minLength":64,"maxLength":64},"name":{"type":"string","minLength":1,"maxLength":240},"kind":{"enum":["image","font"]},"mime":{"enum":["image/png","image/jpeg","image/webp","font/ttf","font/otf","font/woff2"]},"bytes":{"type":"integer","minimum":1,"maximum":33554432},"url":{"type":"string","minLength":1,"maxLength":2048},"license":{"type":"string","minLength":1,"maxLength":120},"source":{"type":"string","minLength":0,"maxLength":2048},"notice":{"type":"string","maxLength":16384}},"required":["id","name","kind","mime","bytes","url","license"],"additionalProperties":false},"ThemeAssetImportParams":{"type":"object","properties":{"fileRef":{"type":"string","minLength":1,"maxLength":128},"license":{"type":"string","minLength":1,"maxLength":120},"notice":{"type":"string","minLength":0,"maxLength":16384},"source":{"type":"string","minLength":0,"maxLength":2048}},"required":["fileRef","license"],"additionalProperties":false},"ThemeBundledAssetImportParams":{"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":512},"license":{"type":"string","minLength":1,"maxLength":120},"notice":{"type":"string","minLength":0,"maxLength":16384},"source":{"type":"string","minLength":0,"maxLength":2048}},"required":["path","license"],"additionalProperties":false},"ThemeAssetGetParams":{"type":"object","properties":{"id":{"type":"string","pattern":"^[a-f0-9]{64}$","minLength":64,"maxLength":64}},"required":["id"],"additionalProperties":false},"ThemeAssetListParams":{"type":"object","properties":{"after":{"type":"string","pattern":"^[a-f0-9]{64}$","minLength":64,"maxLength":64},"limit":{"type":"integer","minimum":1,"maximum":16}},"required":[],"additionalProperties":false},"ThemeAssetListResult":{"type":"object","properties":{"assets":{"type":"array","items":{"$ref":"#/$defs/ThemeAsset"},"minItems":0,"maxItems":16},"next":{"oneOf":[{"type":"string","pattern":"^[a-f0-9]{64}$","minLength":64,"maxLength":64},{"type":"null"}]}},"required":["assets","next"],"additionalProperties":false},"RendererThemeStyle":{"type":"object","properties":{"version":{"const":2},"css":{"type":"string","minLength":0,"maxLength":49152},"motion":{"enum":["full","reduced","off"]},"layers":{"type":"array","maxItems":32,"items":{"type":"object","properties":{"id":{"type":"string","pattern":"^[a-zA-Z][a-zA-Z0-9_-]*$","minLength":1,"maxLength":64},"scope":{"enum":["shell","home","workspace"]},"text":{"type":"string","maxLength":160}},"required":["id","scope","text"],"additionalProperties":false}}},"required":["version","css","motion","layers"],"additionalProperties":false}},"methods":{"events.subscribe":{"permission":"events:subscribe","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/EventTopicParams"},"result":{"$ref":"#/$defs/EventSubscription"}},"events.publish":{"permission":"events:publish","contexts":["backend"],"params":{"$ref":"#/$defs/EventPublishParams"},"result":{"$ref":"#/$defs/EventPublishResult"}},"events.poll":{"permission":"events:subscribe","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/EventSubscriptionParams"},"result":{"$ref":"#/$defs/EventPollResult"}},"events.unsubscribe":{"permission":"events:subscribe","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/EventSubscriptionParams"},"result":{"type":"boolean"}},"rpc.cancel":{"permission":null,"contexts":["renderer"],"params":{"$ref":"#/$defs/RequestCancelParams"},"result":{"type":"boolean"}},"files.stream.open":{"permission":"files:read","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/FileResolveParams"},"result":{"$ref":"#/$defs/FileStream"}},"files.stream.read":{"permission":"files:read","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/FileStreamReadParams"},"result":{"$ref":"#/$defs/FileStreamChunk"}},"files.stream.close":{"permission":"files:read","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/FileStreamCloseParams"},"result":{"type":"boolean"}},"logging.write":{"permission":"logging:write","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/LogWriteParams"},"result":{"type":"boolean"}},"capabilities.resolve":{"permission":null,"contexts":["renderer","backend"],"params":{"$ref":"#/$defs/CapabilityResolveParams"},"result":{"$ref":"#/$defs/CapabilityBinding"}},"runtime.ping":{"permission":null,"contexts":["renderer","backend"],"params":{"$ref":"#/$defs/RuntimePingParams"},"result":{"$ref":"#/$defs/RuntimePingResult"}},"storage.get":{"permission":"storage:read","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/StorageGetParams"},"result":{"$ref":"#/$defs/StorageGetResult"}},"storage.set":{"permission":"storage:write","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/StorageSetParams"},"result":{"$ref":"#/$defs/StorageSetResult"}},"storage.delete":{"permission":"storage:write","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/StorageDeleteParams"},"result":{"$ref":"#/$defs/StorageDeleteResult"}},"storage.batch":{"permission":"storage:write","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/StorageBatchParams"},"result":{"$ref":"#/$defs/StorageBatchResult"}},"storage.list":{"permission":"storage:read","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/StorageListParams"},"result":{"$ref":"#/$defs/StorageListResult"}},"storage.keys":{"permission":"storage:read","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/StorageKeysParams"},"result":{"$ref":"#/$defs/StorageKeysResult"}},"storage.read.begin":{"permission":"storage:read","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/StorageReadBeginParams"},"result":{"$ref":"#/$defs/StorageReadBeginResult"}},"storage.read.chunk":{"permission":"storage:read","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/StorageReadChunkParams"},"result":{"$ref":"#/$defs/StorageReadChunkResult"}},"storage.read.close":{"permission":"storage:read","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/StorageReadCloseParams"},"result":{"$ref":"#/$defs/StorageReadCloseResult"}},"storage.write.begin":{"permission":"storage:write","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/StorageWriteBeginParams"},"result":{"$ref":"#/$defs/StorageWriteBeginResult"}},"storage.write.chunk":{"permission":"storage:write","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/StorageWriteChunkParams"},"result":{"$ref":"#/$defs/StorageWriteChunkResult"}},"storage.write.commit":{"permission":"storage:write","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/StorageWriteCommitParams"},"result":{"$ref":"#/$defs/StorageWriteCommitResult"}},"storage.write.abort":{"permission":"storage:write","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/StorageWriteAbortParams"},"result":{"$ref":"#/$defs/StorageWriteAbortResult"}},"config.get":{"permission":"storage:read","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/ConfigGetParams"},"result":{"$ref":"#/$defs/ConfigGetResult"}},"config.patch":{"permission":"storage:write","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/ConfigPatchParams"},"result":{"$ref":"#/$defs/ConfigPatchResult"}},"tasks.get":{"permission":"tasks:read","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/TasksGetParams"},"result":{"$ref":"#/$defs/TasksGetResult"}},"tasks.cancel":{"permission":"tasks:control","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/TasksCancelParams"},"result":{"$ref":"#/$defs/TasksCancelResult"}},"tasks.list":{"permission":"tasks:read","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/TasksListParams"},"result":{"$ref":"#/$defs/TasksListResult"}},"theme.get":{"permission":"theme:read","contexts":["renderer"],"params":{"$ref":"#/$defs/ThemeGetParams"},"result":{"$ref":"#/$defs/ThemeGetResult"}},"theme.list":{"permission":"theme:read","contexts":["renderer"],"params":{"$ref":"#/$defs/ThemeListParams"},"result":{"$ref":"#/$defs/ThemeListResult"}},"theme.preview":{"permission":"theme:write","contexts":["renderer"],"params":{"$ref":"#/$defs/ThemePreviewParams"},"result":{"$ref":"#/$defs/ThemePreviewResult"}},"theme.commit":{"permission":"theme:write","contexts":["renderer"],"params":{"$ref":"#/$defs/ThemeCommitParams"},"result":{"$ref":"#/$defs/ThemeCommitResult"}},"theme.rollback":{"permission":"theme:write","contexts":["renderer"],"params":{"$ref":"#/$defs/ThemeRollbackParams"},"result":{"$ref":"#/$defs/ThemeRollbackResult"}},"theme.set":{"permission":"theme:write","contexts":["renderer"],"params":{"$ref":"#/$defs/ThemeSetParams"},"result":{"$ref":"#/$defs/ThemeSetResult"}},"dialog.open":{"permission":"dialog","contexts":["renderer"],"params":{"$ref":"#/$defs/DialogOpenParams"},"result":{"$ref":"#/$defs/DialogOpenResult"}},"dialog.confirm":{"permission":"dialog","contexts":["renderer"],"params":{"$ref":"#/$defs/DialogConfirmParams"},"result":{"$ref":"#/$defs/DialogConfirmResult"}},"notification.show":{"permission":"notification","contexts":["renderer"],"params":{"$ref":"#/$defs/NotificationShowParams"},"result":{"$ref":"#/$defs/NotificationShowResult"}},"clipboard.read":{"permission":"clipboard","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/Empty"},"result":{"type":"string"}},"clipboard.write":{"permission":"clipboard","contexts":["renderer","backend"],"params":{"type":"object","additionalProperties":false,"required":["text"],"properties":{"text":{"type":"string","maxLength":32768}}},"result":{"type":"boolean"}},"data.directory":{"permission":null,"contexts":["backend"],"params":{"$ref":"#/$defs/Empty"},"result":{"type":"string","minLength":1,"maxLength":32767}},"resources.list":{"permission":null,"contexts":["backend"],"params":{"$ref":"#/$defs/Empty"},"result":{"type":"array","items":{"$ref":"#/$defs/ResourceMount"},"maxItems":64}},"files.choose":{"permission":"dialog","contexts":["renderer","backend"],"params":{"type":"object","additionalProperties":false,"required":["type"],"properties":{"type":{"enum":["file","folder"]},"multiple":{"type":"boolean"},"extensions":{"type":"array","items":{"type":"string","minLength":1,"maxLength":20},"maxItems":64}}},"result":{"oneOf":[{"type":"null"},{"type":"array","items":{"$ref":"#/$defs/FileReference"},"maxItems":128}]}},"files.read":{"permission":"files:read","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/FileReadParams"},"result":{"type":"object","additionalProperties":false,"required":["data","eof"],"properties":{"data":{"type":"string","maxLength":32768},"eof":{"type":"boolean"}}}},"files.resolve":{"permission":"files:read","contexts":["backend"],"params":{"$ref":"#/$defs/FileResolveParams"},"result":{"type":"string","minLength":1,"maxLength":32767}},"files.publish":{"permission":"files:publish","contexts":["backend"],"params":{"$ref":"#/$defs/FilePublishParams"},"result":{"$ref":"#/$defs/FileReference"}},"tasks.start":{"permission":"tasks:control","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/TaskStartParams"},"result":{"type":"object","additionalProperties":false,"required":["taskId"],"properties":{"taskId":{"type":"string","minLength":1,"maxLength":128}}}},"tasks.beginCommit":{"permission":"tasks:execute","contexts":["backend"],"params":{"type":"object","properties":{"taskId":{"type":"string","minLength":1,"maxLength":128}},"required":["taskId"],"additionalProperties":false},"result":{"type":"boolean"}},"tasks.progress":{"permission":"tasks:execute","contexts":["backend"],"params":{"$ref":"#/$defs/TaskProgressParams"},"result":{"type":"boolean"}},"network.route":{"permission":"network:fetch","contexts":["backend"],"params":{"type":"object","properties":{"url":{"type":"string","minLength":1,"maxLength":8192}},"required":["url"],"additionalProperties":false},"result":{"type":"object","properties":{"kind":{"enum":["direct","proxy"]},"url":{"type":"string","maxLength":8192}},"required":["kind"],"additionalProperties":false}},"network.fetch":{"permission":"network:fetch","contexts":["backend"],"params":{"$ref":"#/$defs/NetworkFetchParams"},"result":{"$ref":"#/$defs/NetworkFetchResult"}},"tasks.pause":{"permission":"tasks:control","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/TasksGetParams"},"result":{"type":"boolean"}},"tasks.resume":{"permission":"tasks:control","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/TasksGetParams"},"result":{"type":"boolean"}},"tasks.controlState":{"permission":"tasks:execute","contexts":["backend"],"params":{"$ref":"#/$defs/TasksControlStateParams"},"result":{"type":"object","properties":{"paused":{"type":"boolean"},"cancelRequested":{"type":"boolean"}},"required":["paused","cancelRequested"],"additionalProperties":false}},"theme.assets.import":{"permission":"theme:assets","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/ThemeAssetImportParams"},"result":{"$ref":"#/$defs/ThemeAsset"}},"theme.assets.importBundled":{"permission":"theme:assets","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/ThemeBundledAssetImportParams"},"result":{"$ref":"#/$defs/ThemeAsset"}},"theme.assets.get":{"permission":"theme:assets","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/ThemeAssetGetParams"},"result":{"$ref":"#/$defs/ThemeAsset"}},"theme.assets.list":{"permission":"theme:assets","contexts":["renderer","backend"],"params":{"$ref":"#/$defs/ThemeAssetListParams"},"result":{"$ref":"#/$defs/ThemeAssetListResult"}}},"lifecycle":{"backend":"on-demand","background":"only-when-declared","pageClose":"detach","disable":"stop-tasks-and-process-tree-before-commit","upgrade":"stop-old-before-swap","uninstall":"retain-data-and-stop-before-removal","shutdown":"stop-process-trees","restart":"interrupt-without-replay","cancel":"request-intent-then-confirm-executor-stopped"},"trust":{"native":"trusted-local-code","sandbox":false,"signature":"ed25519-detached-raw-manifest","unsigned":"explicit-local-install-decision","publisher":"verified-key-policy-not-display-name"},"methodsPolicy":"The gateway enforces built-in method contracts and typed service methods. Self-backend and legacy string service methods have opaque payloads.","transport":{"backgroundMethod":"cruciblebox.lifecycle.background","framing":"uint32-big-endian-length-prefixed-utf8-json","stdout":"protocol-only","stderr":"diagnostics","handshake":["hello","initialize","ready"],"session":"host-generated-256-bit-token","rendererLease":"bounded-renewal-by-trusted-mounted-owner;dispose-revokes","rendererReplay":"host-frame-sequence;64-id-replay-window;bounded-delivery-reordering;client-correlation-preserved","sessionRequestsBudget":"opaque-request-id-sessions;renderer-sequence-does-not-retain-history","backendInvocation":"host-replaces-session-and-derives-callerId","taskCompletion":"correlated-reply-after-executor-stops","cancellation":"cancel-frame-then-stop-process-tree-at-deadline","resourceMounts":"validated-installed-versions-only"},"install":{"layout":"versioned-code-and-resources-stable-data","codeDirectory":".cruciblebox-code","reservedPaths":["plugin.json","resource.json","signature.json",".openbox-host-transaction.json",".openbox-host-transaction.json.pending",".cruciblebox-code"],"approval":"immutable-prepared-package-token","publisherChange":"explicit-approval","nativeCode":"explicit-approval","permissionExpansion":"explicit-approval","failure":"preserve-old-install-and-user-data","resourceImport":"user-initiated-local-offline-package"}}"###;
pub const MANIFEST_VERSION: u8 = 7;
pub const SDK_API_VERSION: u8 = 7;
pub const WIRE_VERSION: u64 = 5;
pub const MIN_HOST_VERSION: &str = "2.2.0-beta.1";
pub const CODE_DIRECTORY: &str = ".cruciblebox-code";
pub const BACKGROUND_METHOD: &str = "cruciblebox.lifecycle.background";
pub const BUDGET_EVENT_BYTES: usize = 8192;
pub const BUDGET_EVENT_SUBSCRIPTIONS: usize = 128;
pub const BUDGET_EVENT_SUBSCRIPTIONS_PER_SESSION: usize = 16;
pub const BUDGET_EVENT_QUEUE: usize = 32;
pub const BUDGET_FILE_STREAMS: usize = 64;
pub const BUDGET_FILE_STREAMS_PER_SESSION: usize = 8;
pub const BUDGET_FILE_STREAM_CHUNK_BYTES: usize = 24576;
pub const BUDGET_CAPABILITY_DESCRIPTOR_BYTES: usize = 32768;
pub const BUDGET_CAPABILITY_DESCRIPTOR_NODES: usize = 8192;
pub const BUDGET_BYTES: usize = 65536;
pub const BUDGET_DEPTH: usize = 32;
pub const BUDGET_NODES: usize = 16384;
pub const BUDGET_INFLIGHT: usize = 32;
pub const BUDGET_LEASE_MS: usize = 300000;
pub const BUDGET_REQUEST_TIMEOUT_MS: usize = 30000;
pub const BUDGET_INTERACTIVE_TIMEOUT_MS: usize = 300000;
pub const BUDGET_HANDSHAKE_TIMEOUT_MS: usize = 10000;
pub const BUDGET_STOP_TIMEOUT_MS: usize = 5000;
pub const BUDGET_PACKAGE_FILES: usize = 5000;
pub const BUDGET_PACKAGE_BYTES: usize = 629145600;
pub const BUDGET_RESOURCE_FILES: usize = 20000;
pub const BUDGET_RESOURCE_BYTES: usize = 17179869184;
pub const BUDGET_MANIFEST_BYTES: usize = 2097152;
pub const BUDGET_RESOURCE_MANIFEST_BYTES: usize = 8388608;
pub const BUDGET_MANIFEST_NODES: usize = 200000;
pub const BUDGET_SESSION_REQUESTS: usize = 4096;
pub const BUDGET_STORAGE_VALUE_BYTES: usize = 4194304;
pub const BUDGET_STORAGE_TRANSACTION_BYTES: usize = 8388608;
pub const BUDGET_STORAGE_CHUNK_BYTES: usize = 24576;
pub const BUDGET_STORAGE_TRANSACTION_OPS: usize = 32;
pub type Json = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Empty {}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionRange {
    pub min: String,
    pub max_exclusive: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiRange {
    pub min: u64,
    pub max_exclusive: u64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Publisher {
    pub id: String,
    pub display_name: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileEntry {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum JsBackendKind {
    #[default]
    #[serde(rename = "js")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JsBackend {
    pub kind: JsBackendKind,
    pub entry: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum NativeBackendKind {
    #[default]
    #[serde(rename = "native")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum NativeBackendOs {
    #[default]
    #[serde(rename = "windows")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum NativeBackendArch {
    #[default]
    #[serde(rename = "x86_64")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum NativeBackendTrust {
    #[default]
    #[serde(rename = "trusted-local-code")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeBackend {
    pub kind: NativeBackendKind,
    pub entry: String,
    pub os: NativeBackendOs,
    pub arch: NativeBackendArch,
    pub trust: NativeBackendTrust,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Backend {
    JsBackend(JsBackend),
    NativeBackend(NativeBackend),
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Navigation {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Command {
    pub id: String,
    pub title: String,
    pub method: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Contributions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigation: Option<Navigation>,
    pub commands: Vec<Command>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_handlers: Option<Vec<FileHandler>>,
}
pub type LegacyServiceMethod = String;
pub type CapabilitySchema = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityMethod {
    pub name: LegacyServiceMethod,
    pub params: CapabilitySchema,
    pub result: CapabilitySchema,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contexts: Option<Vec<String>>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum ServiceMethod {
    LegacyServiceMethod(LegacyServiceMethod),
    CapabilityMethod(CapabilityMethod),
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityEvent {
    pub name: String,
    pub payload: CapabilitySchema,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventTopicParams {
    pub capability_id: String,
    pub event: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventPublishParams {
    pub capability_id: String,
    pub event: String,
    pub payload: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventSubscriptionParams {
    pub subscription_id: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventSubscription {
    pub subscription_id: String,
    pub provider_id: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventMessage {
    pub sequence: u64,
    pub publisher_id: String,
    pub capability_id: String,
    pub event: String,
    pub payload: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventPollResult {
    pub event: serde_json::Value,
    pub dropped: u64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventPublishResult {
    pub delivered: u64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestCancelParams {
    pub request_id: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileStream {
    pub stream_id: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileStreamCloseParams {
    pub stream_id: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileStreamReadParams {
    pub stream_id: String,
    pub offset: u64,
    pub length: u64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileStreamChunk {
    pub data: String,
    pub offset: u64,
    pub next_offset: u64,
    pub eof: bool,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceExport {
    pub id: String,
    pub version: String,
    pub methods: Vec<ServiceMethod>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<CapabilityEvent>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interchangeable: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceDependency {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    pub version: VersionRange,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityResolveParams {
    pub id: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityBinding {
    pub provider_id: String,
    pub provider_version: String,
    pub lifecycle: String,
    pub export: ServiceExport,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogWriteParams {
    pub level: String,
    pub message: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceDependency {
    pub id: String,
    pub version: VersionRange,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceDownload {
    pub path: String,
    pub url: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceRelease {
    pub id: String,
    pub title: String,
    pub description: String,
    pub manifest: ResourceManifest,
    pub downloads: Vec<ResourceDownload>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceCatalog {
    pub resource_catalog_version: u64,
    pub releases: Vec<ResourceRelease>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum PlatformTargetKind {
    #[default]
    #[serde(rename = "platform")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlatformTarget {
    pub kind: PlatformTargetKind,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum BackendTargetKind {
    #[default]
    #[serde(rename = "backend")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendTarget {
    pub kind: BackendTargetKind,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ServiceTargetKind {
    #[default]
    #[serde(rename = "service")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceTarget {
    pub kind: ServiceTargetKind,
    pub id: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Target {
    PlatformTarget(PlatformTarget),
    BackendTarget(BackendTarget),
    ServiceTarget(ServiceTarget),
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub wire_version: u64,
    pub request_id: String,
    pub session: String,
    pub target: Target,
    pub method: String,
    pub params: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RpcError {
    pub code: String,
    pub message: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Success {
    pub wire_version: u64,
    pub request_id: String,
    pub ok: bool,
    pub result: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Failure {
    pub wire_version: u64,
    pub request_id: String,
    pub ok: bool,
    pub error: RpcError,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Response {
    Success(Success),
    Failure(Failure),
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileReference {
    pub r#ref: String,
    pub name: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileReadParams {
    pub r#ref: String,
    pub offset: u64,
    pub length: u64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileResolveParams {
    pub r#ref: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FilePublishParams {
    pub task_id: String,
    pub path: String,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskStartParams {
    pub target: Target,
    pub method: String,
    pub params: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskProgressParams {
    pub task_id: String,
    pub sequence: u64,
    pub stage: String,
    pub percent: u64,
    pub message: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum BackendHelloKind {
    #[default]
    #[serde(rename = "hello")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendHello {
    pub kind: BackendHelloKind,
    pub wire_version: u64,
    pub sdk_api_version: u64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum BackendInitKind {
    #[default]
    #[serde(rename = "initialize")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendInit {
    pub kind: BackendInitKind,
    pub wire_version: u64,
    pub session: String,
    pub plugin_id: String,
    pub data_directory: String,
    pub resources: Vec<ResourceMount>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceMount {
    pub id: String,
    pub version: String,
    pub directory: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum BackendReadyKind {
    #[default]
    #[serde(rename = "ready")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendReady {
    pub kind: BackendReadyKind,
    pub wire_version: u64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum BackendExecuteKind {
    #[default]
    #[serde(rename = "execute")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendExecute {
    pub kind: BackendExecuteKind,
    pub request: Request,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub caller_id: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum BackendReplyKind {
    #[default]
    #[serde(rename = "reply")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendReply {
    pub kind: BackendReplyKind,
    pub response: Response,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum BackendCapabilityKind {
    #[default]
    #[serde(rename = "capability")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendCapability {
    pub kind: BackendCapabilityKind,
    pub request: Request,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum BackendCapabilityReplyKind {
    #[default]
    #[serde(rename = "capability-result")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendCapabilityReply {
    pub kind: BackendCapabilityReplyKind,
    pub response: Response,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum BackendCancelKind {
    #[default]
    #[serde(rename = "cancel")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendCancel {
    pub kind: BackendCancelKind,
    pub wire_version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum BackendStopKind {
    #[default]
    #[serde(rename = "stop")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendStop {
    pub kind: BackendStopKind,
    pub wire_version: u64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum BackendFrame {
    BackendHello(BackendHello),
    BackendInit(BackendInit),
    BackendReady(BackendReady),
    BackendExecute(BackendExecute),
    BackendReply(BackendReply),
    BackendCapability(BackendCapability),
    BackendCapabilityReply(BackendCapabilityReply),
    BackendCancel(BackendCancel),
    BackendStop(BackendStop),
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePingParams {}
pub type RuntimePingResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageGetParams {
    pub key: String,
}
pub type StorageGetResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageSetParams {
    pub key: String,
    pub value: serde_json::Value,
}
pub type StorageSetResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageDeleteParams {
    pub key: String,
}
pub type StorageDeleteResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageBatchParams {
    pub operations: Vec<serde_json::Value>,
}
pub type StorageBatchResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageListParams {
    pub prefix: String,
    pub limit: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageListResult {
    pub items: Vec<serde_json::Value>,
    pub next_cursor: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageKeysParams {
    pub prefix: String,
    pub limit: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageKeysResult {
    pub items: Vec<String>,
    pub next_cursor: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageReadBeginParams {
    pub key: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageReadBeginResult {
    pub found: bool,
    pub read_id: serde_json::Value,
    pub byte_length: u64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageReadChunkParams {
    pub read_id: String,
    pub offset: u64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageReadChunkResult {
    pub offset: u64,
    pub data: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageReadCloseParams {
    pub read_id: String,
}
pub type StorageReadCloseResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageWriteBeginParams {
    pub writes: Vec<serde_json::Value>,
    pub deletes: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageWriteBeginResult {
    pub transaction_id: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageWriteChunkParams {
    pub transaction_id: String,
    pub key: String,
    pub offset: u64,
    pub data: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageWriteChunkResult {
    pub received_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageWriteCommitParams {
    pub transaction_id: String,
}
pub type StorageWriteCommitResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageWriteAbortParams {
    pub transaction_id: String,
}
pub type StorageWriteAbortResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigGetParams {}
pub type ConfigGetResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigPatchParams {
    pub values: serde_json::Value,
}
pub type ConfigPatchResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TasksGetParams {
    pub task_id: String,
}
pub type TasksGetResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TasksControlStateParams {
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_paused: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TasksCancelParams {
    pub task_id: String,
}
pub type TasksCancelResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TasksListParams {
    pub limit: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}
pub type TasksListResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeGetParams {}
pub type ThemeGetResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeListParams {}
pub type ThemeListResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemePreviewParams {
    pub theme: ThemeDefinition,
}
pub type ThemePreviewResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeCommitParams {}
pub type ThemeCommitResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeRollbackParams {}
pub type ThemeRollbackResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeSetParams {
    pub theme: ThemeDefinition,
}
pub type ThemeSetResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DialogOpenParams {
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiple: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<String>>,
}
pub type DialogOpenResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DialogConfirmParams {
    pub title: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_label: Option<String>,
}
pub type DialogConfirmResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationShowParams {
    pub title: String,
    pub body: String,
}
pub type NotificationShowResult = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginDataMigration {
    pub from_plugin_version: VersionRange,
    pub summary: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub manifest_version: u64,
    pub sdk_api_version: u64,
    pub id: String,
    pub version: String,
    pub display_name: String,
    pub description: String,
    pub publisher: Publisher,
    pub min_host_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_migration: Option<PluginDataMigration>,
    pub sdk: ApiRange,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<Backend>,
    pub permissions: Vec<String>,
    pub background: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_interval_ms: Option<u64>,
    pub contributes: Contributions,
    pub exports: Vec<ServiceExport>,
    pub dependencies: Vec<ServiceDependency>,
    pub resources: Vec<ResourceDependency>,
    pub files: Vec<FileEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ResourceManifestOs {
    #[default]
    #[serde(rename = "windows")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ResourceManifestArch {
    #[default]
    #[serde(rename = "x86_64")]
    Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceManifest {
    pub resource_manifest_version: u64,
    pub id: String,
    pub plugin_id: String,
    pub version: String,
    pub kind: String,
    pub contains_native_code: bool,
    pub source: String,
    pub license: String,
    pub plugin_version: VersionRange,
    pub os: ResourceManifestOs,
    pub arch: ResourceManifestArch,
    pub files: Vec<FileEntry>,
    pub publisher: Publisher,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RendererAppearance {
    pub mode: String,
    pub css_vars: serde_json::Value,
}
pub type TaskSnapshot = serde_json::Value;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignatureRecord {
    pub signature_version: u64,
    pub publisher_id: String,
    pub key_id: String,
    pub signature: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkFetchParams {
    pub url: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkFetchResult {
    pub status: u64,
    pub body: String,
    pub headers: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileHandler {
    pub extensions: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeLayout {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_width: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_padding: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_max_width: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_gap: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeComponents {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button_radius: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_radius: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_radius: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_width: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_shadow: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button_skew: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading_weight: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeMotion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemePoint {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeLayer {
    pub id: String,
    pub r#type: String,
    pub scope: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    pub opacity: f64,
    pub order: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<ThemePoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeFont {
    pub asset_id: String,
    pub family: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeDesign {
    pub version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<ThemeLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<ThemeComponents>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<ThemeMotion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layers: Option<Vec<ThemeLayer>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fonts: Option<Vec<ThemeFont>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub css: Option<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeDefinition {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub tokens: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<ThemeDesign>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeAsset {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub mime: String,
    pub bytes: u64,
    pub url: String,
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeAssetImportParams {
    pub file_ref: String,
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeBundledAssetImportParams {
    pub path: String,
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeAssetGetParams {
    pub id: String,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeAssetListParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeAssetListResult {
    pub assets: Vec<ThemeAsset>,
    pub next: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RendererThemeStyle {
    pub version: u64,
    pub css: String,
    pub motion: String,
    pub layers: Vec<serde_json::Value>,
}
