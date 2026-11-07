# Next 3.0 Provider 开发指南

公开 API/UI 为 7.0.0-beta.2，CLI 为 3.0.0-beta.2，Rust SDK 为 0.2.0-beta.2。Manifest/API 7、wire 5。已通过固定宿主验收的基线是 2.2.0-beta.3；beta.4 的新构建受安全告警阻断，见 [当前状态](2.2.0-security-incident-20261107.md)。

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

任务辅助来自公开 UI 包；基础 API 包可用 platform('tasks.start', ...) 等价调用。等待终态且 completedAt 大于 0 后读取完整结果。取消被接受后仍应等待执行器停止确认；超时、崩溃和提交后的收尾失败分别显示，不自动重跑副作用。

## 原生与运行时资源

原生后端使用 serve(factory, handler)，通过 Call 获得宿主签发的 callerId、taskId 和取消标记，通过 Client::platform/service 调用能力。stdout 专用于长度前缀协议，诊断走标准错误或声明的日志 API。实现长操作时检查取消；Job Object 处理最终进程树停止。

外部 Python/二进制工具使用 resources 声明版本范围，tools 指向资源内的 EXE。用户导入并批准资源后，后端在自己的任务中调用 process.run。资源缺失应显示明确状态，不在激活时偷偷下载。当前接口限制输入、输出和执行时间；完整参数见 [API 索引](plugin-api-reference.md) 和契约。可信本地代码拥有当前用户权限，不能宣称 OS 沙箱。

## 验证和维护

先验证方法正常/错误结果与数据事务，再运行 check/pack、安装许可、消费者调用、运行中取消、Provider 崩溃隔离、兼容/不兼容升级、停用保护及重启恢复。实际业务验收留在独立插件仓库，宿主只保存通用平台证据。不要把提供者名称或业务路由加入宿主。流程见 [测试指南](testing.md)、[维护指南](ai-maintenance.md) 和 [迁移说明](next3-migration.md)。
