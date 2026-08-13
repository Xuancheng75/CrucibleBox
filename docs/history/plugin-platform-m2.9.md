# M2.9 GIF Editor 后台残影分析

## 范围与结果

本里程碑只处理 GIF Editor 中会长时间占用 renderer 主线程的残影检测与修复，不更改宿主 SDK、插件权限、数据库 schema、用户文件格式或其它五个插件。主工程版本为 1.5.9，GIF Editor 为 0.3.6。

残影检测和修复现运行在插件 frame 内的一次性 Blob Worker。拖动、预览、主题切换和其它编辑操作不再与 O(n²) 残影候选分析共享 UI 线程。用户可停止检测或修复；导入另一文件以及插件卸载时也会终止旧 Worker，晚到结果不能覆盖新文档。

## 设计边界

- 构建阶段把 `src/workers/residue.worker.ts` 独立打成 browser IIFE，再作为字符串嵌入插件 renderer；发布 ZIP 仍只有 `plugin.json`、`dist/main.js` 和 `dist/renderer.js`。
- 运行时使用现有 renderer v2 CSP 的 `worker-src 'self' blob:`，不开放网络、不增加可执行协议资源，也不恢复 `unsafe-eval`。
- client/worker 采用版本 1、随机 request ID、判别式操作和严格结果验证。文件缓冲和结果 RGBA 缓冲使用 transferable，完成、异常或取消均 terminate Worker 并 revoke Blob URL。
- 64 MiB 文件限制在 `arrayBuffer()` 前检查；worker 内的既有 GIF 维度、帧数和 RGBA 预算形成第二道防线。
- 检测返回的 `pollutedFrame` 直接传入修复。`applyResidueFix` 对该索引重新校验后只解码一次，不再隐式调用一次完整检测。
- 取消采用终止 Worker，而不是仅忽略 Promise；因此正在运行的纯 CPU 循环也会停止。

## 兼容性

旧 GIF、历史记录和导出格式不变。`applyResidueFix(file)` 的旧调用方式仍保留；新 Worker 使用可选的已分析帧号快速路径。renderer API 和 backend API 版本保持 v2，权限仍只有 notification，不发生用户数据迁移。

## 验收

- `npm run check`：宿主 25 文件/163 项、GIF Editor 5 文件/42 项、UniEnv 9 文件/116 项、供应链 Node 6 项全部通过。
- `npm run build`：六插件 clean build、自包含 renderer、可信服务摘要与全部体积门禁通过；GIF renderer 为 318,504/360,000 bytes。
- client 测试覆盖分析/修复结果复原、畸形响应、worker crash、主动取消、清理和超限文件读取前拒绝；核心测试确认修复复用污染帧号时只读取文件一次。
- 确定性插件制品 `artifacts/plugins/gif-editor-0.3.6.zip` 的 SHA-256 为 `6e2c1e7d6a808099c423a3349d25f9fea2e7a1b3e666a2c77755be034155ffaf`，清单与当前 dist 验证通过。
- Windows x64 的 OpenBox 1.5.9 unpacked 应用使用官方缓存 Electron 43.3.0 ZIP 构建；packaged smoke 在临时 userData 中完成真实 v1→v2 数据迁移、renderer/utility/可信服务检查，耗时 1,081 ms，working set 447,288 KiB。

## 后续边界

Worker 解决 UI 阻塞和取消问题，但没有改变残影算法的 O(n²) 复杂度。现有文件、帧数、像素和 RGBA 预算继续承担工作量上界；若未来需要处理更大素材，应先建立分尺寸基准，再考虑分段进度或算法替换。
