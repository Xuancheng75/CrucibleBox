# Next 3.0 Provider 开发指南

当前补完源码使用 API/UI 7.0.0-beta.3、CLI 3.0.0-beta.4、Rust SDK 0.2.0-beta.3；宿主候选 beta.7、Manifest/API 7、wire 5。正在实现与验证，不能沿用 beta.6 的动态通过结论。历史证据见 [P7/P8](2.2.0-p7-p8.md)，当前范围见 [补完记录](2.2.0-completion-audit.md)。

## 独立创建

将公开 npm 包安装到插件自己的开发目录，使用 cruciblebox-next2 init my-provider provider。模板包含 plugin.source.json、plugin.build.json、后端入口和 npm 的 build/test/pack 脚本。原生模板另需解包后的公开 Rust SDK；Cargo 目标目录留在插件项目内。模板检查验证契约及载荷，不替代方法的业务测试和实际安装验收。

接口写入 exports，调用方写入 dependencies。接口版本与插件包版本各自管理。发布者、插件 ID、Capability ID 和宿主版本不是同一概念。

```json
{
  "exports": [
    {
      "id": "example-sum",
      "version": "1.0.0",
      "interchangeable": true,
      "methods": [
        {
          "name": "sum",
          "contexts": ["renderer", "backend"],
          "params": {
            "type": "object",
            "properties": { "numbers": { "type": "array", "items": { "type": "integer" } } },
            "required": ["numbers"],
            "additionalProperties": false
          },
          "result": { "type": "integer" }
        }
      ]
    }
  ]
}
```

这是完整清单中的片段，其他必需字段由 CLI 模板提供。宿主检查参数、结果、上下文和依赖范围；插件负责业务语义，例如求和溢出仍须给出明确错误。接口不可兼容修改升 Capability 主版本，插件包版本改变不能代替接口升版。

## QuickJS 实现

```js
import { defineProvider, createClient } from '@cruciblebox/next2-api'

export const activate = defineProvider(({ api }) => ({
  async sum({ numbers }) {
    const result = numbers.reduce((a, b) => a + b, 0)
    if (!Number.isSafeInteger(result)) {
      throw Object.assign(Error('Sum exceeds safe integer range'), { code: 'OVERFLOW' })
    }
    return result
  }
}))

export async function background(context) {
  const api = createClient(context)
  const count = (await api.platform('storage.get', { key: 'ticks' })) ?? 0
  await api.platform('storage.set', { key: 'ticks', value: count + 1 })
}
```

若使用 background，清单需 background:true、backend.kind:js、适当的 backgroundIntervalMs 及 storage:read/write。宿主周期调用此钩子；不提供 Node.js、setInterval 或任意 shell。普通请求有执行期限，长任务走任务通道。isCancelled 反映当前执行的取消状态；不能将调用方传来的任意 ID 当作任务权限。

## 消费与长任务

```json
{
  "dependencies": [
    {
      "id": "example-sum",
      "providerId": "my-provider",
      "version": { "min": "1.0.0", "maxExclusive": "2.0.0" }
    }
  ]
}
```

固定 providerId 适用于依赖特定提供者、状态或隐私的调用。省略时只在明确可互换的导出中选择；发现结果是当前绑定，不代表之后执行必然成功。

以下消费示例中的 api 是 renderer 中由 @cruciblebox/next2-ui 的 createClient(context) 创建的客户端。

```js
const binding = await api.resolveCapability('example-sum')
const value = await api.service(binding.export.id, 'sum', { numbers: [7, 19] })
const { taskId } = await api.tasks.start({ kind: 'service', id: 'example-sum' }, 'sum', {
  numbers: [7, 19]
})
const state = await api.tasks.get(taskId)
```

任务辅助来自公开 API 包，UI 包沿用同一客户端。等待终态且 completedAt 大于 0 后读取完整结果。取消被接受后仍应等待执行器停止确认；超时、崩溃和提交后的收尾失败分别显示，不自动重跑副作用。前端也可以提交契约标记可任务化的内建方法，例如 tasks.start({kind:'platform'},'files.copy',params)，仍由同一 Registry 和权限规则裁决。

capabilities.list/describe 查询调用方声明的绑定及实际可用状态；capabilities.providers 分页列出已安装 Provider，包括已停用项。registered 表示未启动，starting/ready 依据实际运行实例，failed 保留启动或进程故障，stopping 表示维护/退出中，stopped 表示禁用，unavailable 表示依赖或资源不满足。查询结果不授予调用权；执行时重新解析版本、身份和权限。内建 host-* 能力由保留的 cruciblebox-host 提供，通过同一注册表发现和解析。

## 原生与运行时资源

原生后端使用 serve(factory, handler)，通过 Call 获得宿主签发的 callerId、taskId 和取消标记，通过 Client::platform/service 调用能力。stdout 专用于长度前缀协议，诊断走标准错误或声明的日志 API。实现长操作时检查取消；Job Object 处理最终进程树停止。

外部 Python/二进制工具使用 resources 声明版本范围，tools 指向资源内的 EXE。用户导入并批准资源后，后端在任务中调用 process.run，工作目录可使用调用方授权引用，环境变量显式声明。资源缺失应显示明确状态，不在激活时偷偷下载。当前接口限制输入、输出和执行时间；完整参数见 [API 索引](plugin-api-reference.md) 和契约。可信本地代码拥有当前用户权限，不能宣称 OS 沙箱。

共享运行时使用 cruciblebox-next2 init my-python runtime 模板。Provider 声明 runtimeCapabilities:[{capabilityId:'my-python',toolId:'python',engine:'python'}]，导出 run/environment，并持有单份批准资源。资源清单的 runtimeIdentity:{engine:'python',version:'3.14.7'} 记录实际引擎版本；资源包 version（例如 1.4.0）及 Capability 接口 version（例如 1.0.0）分别维护。模板不捆绑或自动下载解释器。

消费者同时声明 dependencies 中的接口兼容范围、runtimeRequirements:[{capabilityId:'my-python',engine:'python',version:{min:'3.14.0',maxExclusive:'3.15.0'}}] 和 runtime:execute 权限，无需声明或复制 Provider 的资源。api.runtime.resolve('my-python') 返回兼容绑定、实际环境和状态；随后 tasks.start({kind:'service',id:'my-python'},'run',{args:[...]}) 经 Provider 调用 process.run。未批准、缺失、不兼容或停用均不返回可执行绑定。公共宿主集成已验证两个消费者复用同一真实 Python；发布形态独立模板及固定候选验证以补完记录为准。

## 验证和维护

先验证方法正常/错误结果与数据事务，再运行 check/pack、安装许可、消费者调用、运行中取消、Provider 崩溃隔离、兼容/不兼容升级、停用保护及重启恢复。实际业务验收留在独立插件仓库，宿主只保存通用平台证据。不要把提供者名称或业务路由加入宿主。流程见 [测试指南](testing.md)、[维护指南](ai-maintenance.md) 和 [迁移说明](next3-migration.md)。
