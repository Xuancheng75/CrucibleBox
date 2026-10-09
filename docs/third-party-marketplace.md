# 第三方插件目录（2.1.0-beta.2 开发中）

工具箱默认使用官方目录，也允许用户在插件市场添加自己的 HTTPS JSON 目录。当前源码已支持目录添加、停用、删除、下载与安装来源固定；便携版实际验收尚未完成。

## 最小目录

将下列 JSON 发布到可直接读取的 HTTPS 地址，例如 `https://example.org/cruciblebox/catalog.json`。下载 URL 也必须是 HTTPS，且应指向该插件的 ZIP 包。

```json
{
  "schemaVersion": 2,
  "plugins": [
    {
      "id": "sample-tool",
      "version": "0.1.0",
      "artifact": "sample-tool-0.1.0.zip",
      "size": 123456,
      "url": "https://example.org/cruciblebox/sample-tool-0.1.0.zip",
      "displayName": "示例文件工具",
      "publisher": "示例开发者",
      "category": "文件工具",
      "description": "说明插件实际提供的功能。",
      "highlights": ["批量处理", "本地运行"],
      "tags": ["文件", "批量"],
      "keywords": ["file", "batch"],
      "minHostVersion": "2.1.0-beta.2"
    }
  ]
}
```

`id` 对应插件 `plugin.json` 的 `name`。插件 ZIP 应包含 `plugin.json`、`dist/main.js` 和 `dist/renderer.js`。模板位于 `templates/plugin-template`，Manifest v4 字段及命令入口见 `docs/plugin-sdk.md`。旧版官方 schema v1 仍可读取；新版展示/标签字段使用 schema v2。

用户安装后，工具箱记录该插件的目录来源；更新时不应自动换到同名的其他来源。插件是可信代码而非操作系统沙箱，安装和新增权限时用户应阅读权限说明，尤其是 `shell:exec` 本地程序运行权限。删除目录不删除已安装插件，但会使其失去原来源更新地址，直至重新添加相同目录。

## 尚未交付

本文件是 beta.2 的开发文档。Manifest v4 的 `contributes.fileHandlers` 已能在插件自身页面接收匹配扩展名的系统拖入文件路径；跨页面的系统文件关联、右键操作、后台任务和插件间服务接口仍在开发，不能仅在 `contributes` 中声明就视为可用。正式发布前需用独立模板插件、自建 HTTPS 目录及本地 ZIP 完整走通安装、授权、更新、卸载流程。
