// Generated; edit contracts/next/contract.json.
pub const CONTRACT_JSON: &str = r#"{"status":"frozen","versions":{"manifestVersion":5,"sdkApiVersion":5,"wireVersion":3,"dataSchemaVersion":1},"budget":{"bytes":65536,"depth":16,"nodes":4096,"inflight":32,"storageValueBytes":4194304,"storageTransactionBytes":8388608,"storageChunkBytes":24576,"storageTransactionOps":32},"envelope":{"type":"object","properties":{"wireVersion":{"const":3},"requestId":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"session":{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"},"method":{"enum":["runtime.ping","storage.get","storage.set","backend.call","storage.delete","storage.batch","storage.list","storage.keys","storage.read.begin","storage.read.chunk","storage.read.close","storage.write.begin","storage.write.chunk","storage.write.commit","storage.write.abort","document.call","environment.call","archive.call","config.get","config.patch","tasks.get","tasks.cancel","tasks.list","theme.get","theme.list","theme.preview","theme.commit","theme.rollback","theme.set","dialog.open","dialog.confirm","notification.show","result.read.chunk","result.read.close"]},"params":{"type":"object"}},"required":["wireVersion","requestId","session","method","params"],"additionalProperties":false},"methods":{"runtime.ping":{"capability":null,"params":{"type":"object","properties":{},"required":[],"additionalProperties":false}},"storage.get":{"capability":"storage:read","params":{"type":"object","properties":{"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["key"],"additionalProperties":false}},"storage.set":{"capability":"storage:write","params":{"type":"object","properties":{"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"value":{}},"required":["key","value"],"additionalProperties":false}},"backend.call":{"capability":null,"params":{"type":"object","properties":{"method":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[A-Za-z][A-Za-z0-9_]{0,63}$"},"args":{"type":"array","maxItems":32,"items":{}}},"required":["method","args"],"additionalProperties":false}},"storage.delete":{"capability":"storage:write","params":{"type":"object","properties":{"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["key"],"additionalProperties":false}},"storage.batch":{"capability":"storage:write","params":{"type":"object","properties":{"operations":{"type":"array","minItems":1,"maxItems":64,"items":{"oneOf":[{"type":"object","properties":{"type":{"const":"set"},"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"value":{}},"required":["type","key","value"],"additionalProperties":false},{"type":"object","properties":{"type":{"const":"delete"},"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["type","key"],"additionalProperties":false}]}}},"required":["operations"],"additionalProperties":false}},"storage.list":{"capability":"storage:read","params":{"type":"object","properties":{"prefix":{"type":"string","minLength":0,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]*$"},"limit":{"type":"integer","minimum":1,"maximum":100},"after":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["prefix","limit"],"additionalProperties":false},"result":{"type":"object","properties":{"items":{"type":"array","maxItems":100,"items":{"type":"object","properties":{"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"value":{}},"required":["key","value"],"additionalProperties":false}},"nextCursor":{"oneOf":[{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},{"type":"null"}]}},"required":["items","nextCursor"],"additionalProperties":false}},"storage.keys":{"capability":"storage:read","params":{"type":"object","properties":{"prefix":{"type":"string","minLength":0,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]*$"},"limit":{"type":"integer","minimum":1,"maximum":100},"after":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["prefix","limit"],"additionalProperties":false},"result":{"type":"object","properties":{"items":{"type":"array","maxItems":100,"items":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"nextCursor":{"oneOf":[{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},{"type":"null"}]}},"required":["items","nextCursor"],"additionalProperties":false}},"storage.read.begin":{"capability":"storage:read","params":{"type":"object","properties":{"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["key"],"additionalProperties":false},"result":{"type":"object","properties":{"found":{"type":"boolean"},"readId":{"oneOf":[{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"},{"type":"null"}]},"byteLength":{"type":"integer","minimum":0,"maximum":4194304}},"required":["found","readId","byteLength"],"additionalProperties":false}},"storage.read.chunk":{"capability":"storage:read","params":{"type":"object","properties":{"readId":{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"},"offset":{"type":"integer","minimum":0,"maximum":4194304}},"required":["readId","offset"],"additionalProperties":false},"result":{"type":"object","properties":{"offset":{"type":"integer","minimum":0,"maximum":4194304},"data":{"type":"string","minLength":1,"maxLength":32768,"pattern":"^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$"}},"required":["offset","data"],"additionalProperties":false}},"storage.read.close":{"capability":"storage:read","params":{"type":"object","properties":{"readId":{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"}},"required":["readId"],"additionalProperties":false}},"storage.write.begin":{"capability":"storage:write","params":{"type":"object","properties":{"writes":{"type":"array","maxItems":32,"items":{"type":"object","properties":{"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"byteLength":{"type":"integer","minimum":1,"maximum":4194304}},"required":["key","byteLength"],"additionalProperties":false}},"deletes":{"type":"array","maxItems":32,"items":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}}},"required":["writes","deletes"],"additionalProperties":false},"result":{"type":"object","properties":{"transactionId":{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"}},"required":["transactionId"],"additionalProperties":false}},"storage.write.chunk":{"capability":"storage:write","params":{"type":"object","properties":{"transactionId":{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"},"key":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"offset":{"type":"integer","minimum":0,"maximum":4194304},"data":{"type":"string","minLength":1,"maxLength":32768,"pattern":"^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$"}},"required":["transactionId","key","offset","data"],"additionalProperties":false},"result":{"type":"object","properties":{"receivedBytes":{"type":"integer","minimum":1,"maximum":8388608}},"required":["receivedBytes"],"additionalProperties":false}},"storage.write.commit":{"capability":"storage:write","params":{"type":"object","properties":{"transactionId":{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"}},"required":["transactionId"],"additionalProperties":false}},"storage.write.abort":{"capability":"storage:write","params":{"type":"object","properties":{"transactionId":{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"}},"required":["transactionId"],"additionalProperties":false}},"document.call":{"capability":"trusted:document-engine","params":{"type":"object","properties":{"payload":{"type":"object","properties":{"type":{"type":"string","minLength":1,"maxLength":128}},"required":["type"],"additionalProperties":true}},"required":["payload"],"additionalProperties":false}},"environment.call":{"capability":"trusted:unienv","params":{"type":"object","properties":{"payload":{"type":"object","properties":{"type":{"type":"string","minLength":1,"maxLength":128}},"required":["type"],"additionalProperties":true}},"required":["payload"],"additionalProperties":false}},"archive.call":{"capability":"trusted:archive-extractor","params":{"type":"object","properties":{"payload":{"type":"object","properties":{"type":{"type":"string","minLength":1,"maxLength":128}},"required":["type"],"additionalProperties":true}},"required":["payload"],"additionalProperties":false}},"config.get":{"capability":"storage:read","params":{"type":"object","properties":{},"required":[],"additionalProperties":false}},"config.patch":{"capability":"storage:write","params":{"type":"object","properties":{"values":{"type":"object","additionalProperties":true}},"required":["values"],"additionalProperties":false}},"tasks.get":{"capability":"tasks:read","params":{"type":"object","properties":{"taskId":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["taskId"],"additionalProperties":false}},"tasks.cancel":{"capability":"tasks:control","params":{"type":"object","properties":{"taskId":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["taskId"],"additionalProperties":false}},"tasks.list":{"capability":"tasks:read","params":{"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":20},"after":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["limit"],"additionalProperties":false}},"theme.get":{"capability":"theme:read","params":{"type":"object","properties":{},"required":[],"additionalProperties":false}},"theme.list":{"capability":"theme:read","params":{"type":"object","properties":{},"required":[],"additionalProperties":false}},"theme.preview":{"capability":"theme:write","params":{"type":"object","properties":{"theme":{"type":"object","properties":{"id":{"type":"string","minLength":1,"maxLength":64},"name":{"type":"string","minLength":1,"maxLength":80},"mode":{"enum":["light","dark"]},"tokens":{"type":"object","properties":{"colorBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBgLayout":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBgContainer":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBgElevated":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorPrimary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorPrimaryHover":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorPrimaryBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorText":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorTextSecondary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorTextTertiary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBorder":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBorderSecondary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorSuccess":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorSuccessBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorWarning":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorWarningBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorError":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorErrorBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorLink":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"borderRadius":{"type":"number","minimum":0,"maximum":32},"fontFamily":{"type":"string","minLength":1,"maxLength":512,"pattern":"^[^;<>\\\\]+$"}},"required":["colorBg","colorBgLayout","colorBgContainer","colorBgElevated","colorPrimary","colorPrimaryHover","colorPrimaryBg","colorText","colorTextSecondary","colorTextTertiary","colorBorder","colorBorderSecondary","colorSuccess","colorSuccessBg","colorWarning","colorWarningBg","colorError","colorErrorBg","colorLink","borderRadius","fontFamily"],"additionalProperties":false}},"required":["id","name","mode","tokens"],"additionalProperties":false}},"required":["theme"],"additionalProperties":false}},"theme.commit":{"capability":"theme:write","params":{"type":"object","properties":{},"required":[],"additionalProperties":false}},"theme.rollback":{"capability":"theme:write","params":{"type":"object","properties":{},"required":[],"additionalProperties":false}},"theme.set":{"capability":"theme:write","params":{"type":"object","properties":{"theme":{"type":"object","properties":{"id":{"type":"string","minLength":1,"maxLength":64},"name":{"type":"string","minLength":1,"maxLength":80},"mode":{"enum":["light","dark"]},"tokens":{"type":"object","properties":{"colorBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBgLayout":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBgContainer":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBgElevated":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorPrimary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorPrimaryHover":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorPrimaryBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorText":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorTextSecondary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorTextTertiary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBorder":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBorderSecondary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorSuccess":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorSuccessBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorWarning":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorWarningBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorError":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorErrorBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorLink":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"borderRadius":{"type":"number","minimum":0,"maximum":32},"fontFamily":{"type":"string","minLength":1,"maxLength":512,"pattern":"^[^;<>\\\\]+$"}},"required":["colorBg","colorBgLayout","colorBgContainer","colorBgElevated","colorPrimary","colorPrimaryHover","colorPrimaryBg","colorText","colorTextSecondary","colorTextTertiary","colorBorder","colorBorderSecondary","colorSuccess","colorSuccessBg","colorWarning","colorWarningBg","colorError","colorErrorBg","colorLink","borderRadius","fontFamily"],"additionalProperties":false}},"required":["id","name","mode","tokens"],"additionalProperties":false}},"required":["theme"],"additionalProperties":false}},"dialog.open":{"capability":"dialog","params":{"type":"object","properties":{"type":{"enum":["file","folder"]},"multiple":{"type":"boolean"},"extensions":{"type":"array","maxItems":32,"items":{"type":"string","minLength":1,"maxLength":16,"pattern":"^[A-Za-z0-9]+$"}}},"required":["type"],"additionalProperties":false}},"dialog.confirm":{"capability":"dialog","params":{"type":"object","properties":{"title":{"type":"string","minLength":1,"maxLength":200},"message":{"type":"string","minLength":1,"maxLength":4000},"confirmLabel":{"type":"string","minLength":1,"maxLength":80},"cancelLabel":{"type":"string","minLength":1,"maxLength":80}},"required":["title","message"],"additionalProperties":false}},"notification.show":{"capability":"notification","params":{"type":"object","properties":{"title":{"type":"string","minLength":1,"maxLength":200},"body":{"type":"string","maxLength":4000}},"required":["title","body"],"additionalProperties":false}},"result.read.chunk":{"capability":null,"params":{"type":"object","properties":{"readId":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"offset":{"type":"integer","minimum":0,"maximum":4194304}},"required":["readId","offset"],"additionalProperties":false}},"result.read.close":{"capability":null,"params":{"type":"object","properties":{"readId":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"}},"required":["readId"],"additionalProperties":false}}},"responses":{"success":{"type":"object","properties":{"wireVersion":{"const":3},"requestId":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"ok":{"const":true},"result":{}},"required":["wireVersion","requestId","ok","result"],"additionalProperties":false},"failure":{"type":"object","properties":{"wireVersion":{"const":3},"requestId":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"ok":{"const":false},"error":{"type":"object","properties":{"code":{"enum":["INVALID_REQUEST","INVALID_RESPONSE","BUDGET_EXCEEDED","SESSION_DENIED","SESSION_EXPIRED","SESSION_EXHAUSTED","PERMISSION_DENIED","REPLAY_DENIED","BUSY","TIMEOUT","STORAGE_UNAVAILABLE","STORAGE_CORRUPT","INTERNAL_ERROR"]},"message":{"type":"string","minLength":1,"maxLength":256}},"required":["code","message"],"additionalProperties":false}},"required":["wireVersion","requestId","ok","error"],"additionalProperties":false}},"manifest":{"type":"object","properties":{"id":{"type":"string","minLength":2,"maxLength":64,"pattern":"^[a-z][a-z0-9-]{1,63}$"},"version":{"type":"string","minLength":5,"maxLength":128,"format":"semver-without-build"},"displayName":{"type":"string","minLength":1,"maxLength":128},"manifestVersion":{"const":5},"sdkApiVersion":{"const":5},"wireVersion":{"const":3},"dataSchemaVersion":{"const":1},"renderer":{"const":"dist/renderer.js"},"backend":{"const":"dist/main.js"},"permissions":{"type":"array","maxItems":16,"uniqueItems":true,"items":{"enum":["storage:read","storage:write","browser:downloads","trusted:document-engine","trusted:unienv","trusted:archive-extractor","tasks:read","tasks:control","theme:read","theme:write","dialog","notification"]}},"description":{"type":"string","maxLength":1000},"author":{"type":"string","maxLength":100},"icon":{"type":"string","maxLength":256},"category":{"type":"string","maxLength":64},"config":{"type":"object","additionalProperties":true}},"required":["id","version","displayName","manifestVersion","sdkApiVersion","wireVersion","dataSchemaVersion","renderer","permissions"],"additionalProperties":false},"rendererTransport":{"handshakeVersion":1,"handshakeTimeoutMs":10000,"requestTimeoutMs":10000,"sandbox":"allow-scripts","messageOrigin":"null","leaseMs":1800000,"minLeaseMs":100,"maxLeaseMs":86400000},"backendTransport":{"wireVersion":3,"controlMethods":["activate","call","dispose"],"maxQueue":32,"maxWorkers":16,"timeoutMs":30000,"frames":{"control":{"type":"object","properties":{"kind":{"const":"control"},"wireVersion":{"const":3},"token":{"type":"string","minLength":32,"maxLength":128,"pattern":"^[A-Za-z0-9]+$"},"requestId":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_.:-]+$"},"method":{"enum":["activate","call","dispose"]},"params":{"type":"object"}},"required":["kind","wireVersion","token","requestId","method","params"],"additionalProperties":false},"capability":{"type":"object","properties":{"kind":{"const":"capability"},"request":{"type":"object"}},"required":["kind","request"],"additionalProperties":false},"result":{"type":"object","properties":{"kind":{"const":"result"},"response":{"type":"object"}},"required":["kind","response"],"additionalProperties":false},"capability-result":{"type":"object","properties":{"kind":{"const":"capability-result"},"response":{"type":"object"}},"required":["kind","response"],"additionalProperties":false}},"controlParams":{"activate":{"type":"object","properties":{},"required":[],"additionalProperties":false},"dispose":{"type":"object","properties":{},"required":[],"additionalProperties":false},"call":{"type":"object","properties":{"method":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[A-Za-z][A-Za-z0-9_]{0,63}$"},"args":{"type":"array","maxItems":32,"items":{}}},"required":["method","args"],"additionalProperties":false}}},"rendererAppearance":{"type":"object","properties":{"mode":{"enum":["light","dark"]},"cssVars":{"type":"object","properties":{"--ob-mode":{"type":"string","maxLength":512},"--ob-theme-id":{"type":"string","maxLength":512},"--ob-color-primary-contrast":{"type":"string","maxLength":512},"--ob-color-success-border":{"type":"string","maxLength":512},"--ob-color-warning-border":{"type":"string","maxLength":512},"--ob-color-error-border":{"type":"string","maxLength":512},"--ob-color-bg":{"type":"string","maxLength":512},"--ob-color-bg-layout":{"type":"string","maxLength":512},"--ob-color-bg-container":{"type":"string","maxLength":512},"--ob-color-bg-elevated":{"type":"string","maxLength":512},"--ob-color-primary":{"type":"string","maxLength":512},"--ob-color-primary-hover":{"type":"string","maxLength":512},"--ob-color-primary-bg":{"type":"string","maxLength":512},"--ob-color-text":{"type":"string","maxLength":512},"--ob-color-text-secondary":{"type":"string","maxLength":512},"--ob-color-text-tertiary":{"type":"string","maxLength":512},"--ob-color-border":{"type":"string","maxLength":512},"--ob-color-border-secondary":{"type":"string","maxLength":512},"--ob-color-success":{"type":"string","maxLength":512},"--ob-color-success-bg":{"type":"string","maxLength":512},"--ob-color-warning":{"type":"string","maxLength":512},"--ob-color-warning-bg":{"type":"string","maxLength":512},"--ob-color-error":{"type":"string","maxLength":512},"--ob-color-error-bg":{"type":"string","maxLength":512},"--ob-color-link":{"type":"string","maxLength":512},"--ob-radius":{"type":"string","maxLength":512},"--ob-font-family":{"type":"string","maxLength":512},"--ob-colorBg":{"type":"string","maxLength":512},"--ob-colorBgLayout":{"type":"string","maxLength":512},"--ob-colorBgContainer":{"type":"string","maxLength":512},"--ob-colorBgElevated":{"type":"string","maxLength":512},"--ob-colorPrimary":{"type":"string","maxLength":512},"--ob-colorPrimaryHover":{"type":"string","maxLength":512},"--ob-colorPrimaryBg":{"type":"string","maxLength":512},"--ob-colorText":{"type":"string","maxLength":512},"--ob-colorTextSecondary":{"type":"string","maxLength":512},"--ob-colorTextTertiary":{"type":"string","maxLength":512},"--ob-colorBorder":{"type":"string","maxLength":512},"--ob-colorBorderSecondary":{"type":"string","maxLength":512},"--ob-colorSuccess":{"type":"string","maxLength":512},"--ob-colorSuccessBg":{"type":"string","maxLength":512},"--ob-colorWarning":{"type":"string","maxLength":512},"--ob-colorWarningBg":{"type":"string","maxLength":512},"--ob-colorError":{"type":"string","maxLength":512},"--ob-colorErrorBg":{"type":"string","maxLength":512},"--ob-colorLink":{"type":"string","maxLength":512},"--ob-borderRadius":{"type":"string","maxLength":512},"--ob-fontFamily":{"type":"string","maxLength":512}},"required":["--ob-mode","--ob-theme-id","--ob-color-primary-contrast","--ob-color-success-border","--ob-color-warning-border","--ob-color-error-border","--ob-color-bg","--ob-color-bg-layout","--ob-color-bg-container","--ob-color-bg-elevated","--ob-color-primary","--ob-color-primary-hover","--ob-color-primary-bg","--ob-color-text","--ob-color-text-secondary","--ob-color-text-tertiary","--ob-color-border","--ob-color-border-secondary","--ob-color-success","--ob-color-success-bg","--ob-color-warning","--ob-color-warning-bg","--ob-color-error","--ob-color-error-bg","--ob-color-link","--ob-radius","--ob-font-family","--ob-colorBg","--ob-colorBgLayout","--ob-colorBgContainer","--ob-colorBgElevated","--ob-colorPrimary","--ob-colorPrimaryHover","--ob-colorPrimaryBg","--ob-colorText","--ob-colorTextSecondary","--ob-colorTextTertiary","--ob-colorBorder","--ob-colorBorderSecondary","--ob-colorSuccess","--ob-colorSuccessBg","--ob-colorWarning","--ob-colorWarningBg","--ob-colorError","--ob-colorErrorBg","--ob-colorLink","--ob-borderRadius","--ob-fontFamily"],"additionalProperties":false}},"required":["mode","cssVars"],"additionalProperties":false},"taskSnapshot":{"type":"object","properties":{"taskId":{"type":"string","minLength":1,"maxLength":128},"status":{"enum":["queued","running","paused","succeeded","failed","cancelled","interrupted"]},"sequence":{"type":"integer","minimum":0,"maximum":9007199254740991},"cancelRequested":{"type":"boolean"},"resultRefs":{"type":"array","maxItems":32,"items":{"type":"string","maxLength":2048}},"resourceKey":{"type":"string","maxLength":256},"error":{},"progress":{},"publication":{},"checkpoint":{}},"required":["taskId","status","sequence","cancelRequested","resultRefs","resourceKey"],"additionalProperties":true},"theme":{"type":"object","properties":{"id":{"type":"string","minLength":1,"maxLength":64},"name":{"type":"string","minLength":1,"maxLength":80},"mode":{"enum":["light","dark"]},"tokens":{"type":"object","properties":{"colorBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBgLayout":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBgContainer":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBgElevated":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorPrimary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorPrimaryHover":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorPrimaryBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorText":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorTextSecondary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorTextTertiary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBorder":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorBorderSecondary":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorSuccess":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorSuccessBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorWarning":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorWarningBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorError":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorErrorBg":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"colorLink":{"type":"string","minLength":1,"maxLength":128,"pattern":"^(#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})|rgba?\\([0-9.,\\s]+\\))$"},"borderRadius":{"type":"number","minimum":0,"maximum":32},"fontFamily":{"type":"string","minLength":1,"maxLength":512,"pattern":"^[^;<>\\\\]+$"}},"required":["colorBg","colorBgLayout","colorBgContainer","colorBgElevated","colorPrimary","colorPrimaryHover","colorPrimaryBg","colorText","colorTextSecondary","colorTextTertiary","colorBorder","colorBorderSecondary","colorSuccess","colorSuccessBg","colorWarning","colorWarningBg","colorError","colorErrorBg","colorLink","borderRadius","fontFamily"],"additionalProperties":false}},"required":["id","name","mode","tokens"],"additionalProperties":false},"rendererEvents":{"filesDropped":{"type":"array","minItems":1,"maxItems":256,"items":{"type":"string","minLength":1,"maxLength":2048}}},"resultTransport":{"maxBytes":4194304,"maxSnapshots":32,"maxSnapshotsPerOwner":4,"maxTotalBytes":67108864,"leaseMs":90000,"inlineBytes":49152}}"#;
pub const CONTRACT_SHA256: &str =
    "502bf0b45f8bff7b6278576e8952b964bc645a05ce0f2c1786e583e5565bb505";
pub const MAX_INFLIGHT: usize = 32;
pub const HANDSHAKE_TIMEOUT_MS: u64 = 10000;
pub const RPC_TIMEOUT_MS: u64 = 10000;
pub const BACKEND_TIMEOUT_MS: u64 = 30000;
pub const MAX_FRAME_BYTES: usize = 65536;
pub const MAX_STORAGE_VALUE_BYTES: usize = 4194304;
pub const MAX_STORAGE_TRANSACTION_BYTES: usize = 8388608;
pub const MAX_STORAGE_CHUNK_BYTES: usize = 24576;
pub const MAX_STORAGE_TRANSACTION_OPS: usize = 32;
pub const LEASE_MS: u64 = 1800000;
pub const MIN_LEASE_MS: u64 = 100;
pub const MAX_LEASE_MS: u64 = 86400000;
pub const BACKEND_QUEUE: usize = 32;
pub const MAX_BACKEND_WORKERS: usize = 16;
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePingParams {}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageGetParams {
    pub key: String,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageSetParams {
    pub key: String,
    pub value: serde_json::Value,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendCallParams {
    pub method: String,
    pub args: Vec<serde_json::Value>,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageDeleteParams {
    pub key: String,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageBatchParams {
    pub operations: Vec<serde_json::Value>,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageListParams {
    pub prefix: String,
    pub limit: f64,
    pub after: Option<String>,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageKeysParams {
    pub prefix: String,
    pub limit: f64,
    pub after: Option<String>,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageReadBeginParams {
    pub key: String,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageReadChunkParams {
    pub read_id: String,
    pub offset: f64,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageReadCloseParams {
    pub read_id: String,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageWriteBeginParams {
    pub writes: Vec<serde_json::Value>,
    pub deletes: Vec<serde_json::Value>,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageWriteChunkParams {
    pub transaction_id: String,
    pub key: String,
    pub offset: f64,
    pub data: String,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageWriteCommitParams {
    pub transaction_id: String,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageWriteAbortParams {
    pub transaction_id: String,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentCallParams {
    pub payload: serde_json::Value,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentCallParams {
    pub payload: serde_json::Value,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchiveCallParams {
    pub payload: serde_json::Value,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigGetParams {}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigPatchParams {
    pub values: serde_json::Value,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TasksGetParams {
    pub task_id: String,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TasksCancelParams {
    pub task_id: String,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TasksListParams {
    pub limit: f64,
    pub after: Option<String>,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeGetParams {}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeListParams {}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemePreviewParams {
    pub theme: serde_json::Value,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeCommitParams {}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeRollbackParams {}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeSetParams {
    pub theme: serde_json::Value,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DialogOpenParams {
    pub r#type: serde_json::Value,
    pub multiple: Option<serde_json::Value>,
    pub extensions: Option<Vec<serde_json::Value>>,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DialogConfirmParams {
    pub title: String,
    pub message: String,
    pub confirm_label: Option<String>,
    pub cancel_label: Option<String>,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationShowParams {
    pub title: String,
    pub body: String,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResultReadChunkParams {
    pub read_id: String,
    pub offset: f64,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResultReadCloseParams {
    pub read_id: String,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Call {
    #[serde(rename = "runtime.ping")]
    RuntimePing(RuntimePingParams),
    #[serde(rename = "storage.get")]
    StorageGet(StorageGetParams),
    #[serde(rename = "storage.set")]
    StorageSet(StorageSetParams),
    #[serde(rename = "backend.call")]
    BackendCall(BackendCallParams),
    #[serde(rename = "storage.delete")]
    StorageDelete(StorageDeleteParams),
    #[serde(rename = "storage.batch")]
    StorageBatch(StorageBatchParams),
    #[serde(rename = "storage.list")]
    StorageList(StorageListParams),
    #[serde(rename = "storage.keys")]
    StorageKeys(StorageKeysParams),
    #[serde(rename = "storage.read.begin")]
    StorageReadBegin(StorageReadBeginParams),
    #[serde(rename = "storage.read.chunk")]
    StorageReadChunk(StorageReadChunkParams),
    #[serde(rename = "storage.read.close")]
    StorageReadClose(StorageReadCloseParams),
    #[serde(rename = "storage.write.begin")]
    StorageWriteBegin(StorageWriteBeginParams),
    #[serde(rename = "storage.write.chunk")]
    StorageWriteChunk(StorageWriteChunkParams),
    #[serde(rename = "storage.write.commit")]
    StorageWriteCommit(StorageWriteCommitParams),
    #[serde(rename = "storage.write.abort")]
    StorageWriteAbort(StorageWriteAbortParams),
    #[serde(rename = "document.call")]
    DocumentCall(DocumentCallParams),
    #[serde(rename = "environment.call")]
    EnvironmentCall(EnvironmentCallParams),
    #[serde(rename = "archive.call")]
    ArchiveCall(ArchiveCallParams),
    #[serde(rename = "config.get")]
    ConfigGet(ConfigGetParams),
    #[serde(rename = "config.patch")]
    ConfigPatch(ConfigPatchParams),
    #[serde(rename = "tasks.get")]
    TasksGet(TasksGetParams),
    #[serde(rename = "tasks.cancel")]
    TasksCancel(TasksCancelParams),
    #[serde(rename = "tasks.list")]
    TasksList(TasksListParams),
    #[serde(rename = "theme.get")]
    ThemeGet(ThemeGetParams),
    #[serde(rename = "theme.list")]
    ThemeList(ThemeListParams),
    #[serde(rename = "theme.preview")]
    ThemePreview(ThemePreviewParams),
    #[serde(rename = "theme.commit")]
    ThemeCommit(ThemeCommitParams),
    #[serde(rename = "theme.rollback")]
    ThemeRollback(ThemeRollbackParams),
    #[serde(rename = "theme.set")]
    ThemeSet(ThemeSetParams),
    #[serde(rename = "dialog.open")]
    DialogOpen(DialogOpenParams),
    #[serde(rename = "dialog.confirm")]
    DialogConfirm(DialogConfirmParams),
    #[serde(rename = "notification.show")]
    NotificationShow(NotificationShowParams),
    #[serde(rename = "result.read.chunk")]
    ResultReadChunk(ResultReadChunkParams),
    #[serde(rename = "result.read.close")]
    ResultReadClose(ResultReadCloseParams),
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub wire_version: u32,
    pub request_id: String,
    pub session: String,
    #[serde(flatten)]
    pub call: Call,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcError {
    pub code: String,
    pub message: String,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Success {
    pub wire_version: u32,
    pub request_id: String,
    pub ok: bool,
    pub result: serde_json::Value,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Failure {
    pub wire_version: u32,
    pub request_id: String,
    pub ok: bool,
    pub error: RpcError,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Response {
    Success(Success),
    Failure(Failure),
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub display_name: String,
    pub manifest_version: u32,
    pub sdk_api_version: u32,
    pub wire_version: u32,
    pub data_schema_version: u32,
    pub renderer: String,
    pub backend: Option<String>,
    pub permissions: Vec<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub icon: Option<String>,
    pub category: Option<String>,
    pub config: Option<serde_json::Value>,
}
