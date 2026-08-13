# GIF Editor M1.1：正确性与资源安全

- 状态：已完成
- 日期：2026-08-09
- 插件版本：0.3.2

## 本轮目标

在不引入 Worker、可变尺寸帧模型或大规模 UI 拆分的前提下，修复已确认的像素错误、
统一完整画布不变量，并为不可信 GIF 输入和高内存编辑操作建立首个可执行预算。

## 单一文档不变量

`GifDocument.width/height` 是唯一画布尺寸；每一帧必须满足：

```text
frame.imageData.width  === document.width
frame.imageData.height === document.height
frame.imageData.data.byteLength === width * height * 4
```

导入、文档提交、撤销/重做、画布变换和导出前均执行守卫。旋转、镜像、手工裁剪会
在一次历史提交中对全部帧应用同一变换；自动裁剪有两种明确语义：

- 按当前帧裁剪：从当前帧计算边界，用同一个矩形裁所有帧。
- 按全部帧联合裁剪：取所有非空帧边界的 union，再裁所有帧。

全透明边界是 no-op。画布发生变化后会清除依赖旧坐标系的裁剪框、选区、mask 和图层
会话。残留修复会原子恢复帧及文档尺寸，不再只替换帧数组。

## 正确性修复

- 非正方形 90°/270° 旋转使用 `旧高 × 旧宽` 的正确输出尺寸，并有 2×3 精确像素
  映射测试。
- GIF 帧合成改为可纯单测的 RGBA 缓冲算法。
- disposal=2 只清除当前帧矩形，不再错误清空整个逻辑画布；disposal=3 恢复前一
  画布快照。
- resize 监听使用同一个 handler 注册和注销。
- 动态 CSS 支持内容更新、引用计数及最后一个实例卸载。
- 滤镜滑块只更新预览；单项或全部“应用”只产生一次历史提交。
- 历史快照保留 frame ID、只深拷贝一次，并同时受 50 次和 256 MiB 限制。
- 缩略图以 frame ID 和 `ImageData` 引用增量刷新，覆盖新增、编辑、撤销与重做。
- manifest 显式声明实际使用的 `notification` 权限。

## 默认资源预算

| 边界 | 默认值 |
| --- | ---: |
| 压缩 GIF 文件 | 64 MiB |
| 画布单边 | 4096 px |
| 帧数 | 500 |
| 解码/编码估算 RGBA 工作集 | 256 MiB |
| 历史快照 | 256 MiB、最多 50 次 |
| 网格行/列 | 各 16 |
| 分层完整画布输出 | 最多 16 层、256 MiB |

所有尺寸乘法使用 BigInt，资源检查尽可能位于 `decompressFrames`、分层输出或编码器
分配之前。解码还会校验 GIF 逻辑尺寸、帧区域、patch 长度和帧数量一致性。

## 测试与验收

GIF Editor 现在是独立 Vitest 项目，`npm run check` 会执行：

```powershell
npm run typecheck
npm test
npm run build
```

当前包含 4 个测试文件、35 项测试，覆盖旋转、disposal 1/2/3、透明 patch、输入/输出
预算、画布不变量、文档联合裁剪、历史淘汰、稳定 ID、缩略图刷新、滤镜提交和动态样
式生命周期。根工程门禁还包含原有 40 项测试。

最终制品：`artifacts/plugins/gif-editor-0.3.2.zip`

```text
SHA-256 07721b81a0cb2eef617a5c456f410711c073171e0e67e4981debfd71892bbefb
```

连续两次打包哈希一致。宿主 clean build、插件白名单校验、Electron unpacked 打包和
隐藏窗口启动冒烟均通过；本里程碑没有数据库迁移或用户数据写入。

## 明确延后

- disposal=2 当前恢复透明背景，尚未按 GIF 逻辑背景色恢复。
- `gifuct-js` 内部的 `pixels: number[]` 和编码器压缩缓冲未被预算精确建模。
- 残影分析仍是高复杂度同步算法；解析、量化、编码、分层和残影分析迁入可取消 Worker
  属于后续性能里程碑。
- 缩略图仍使用 PNG data URL，尚未进行时间轴虚拟化和 Object URL/ImageBitmap 优化。
- 固定阈值可能拒绝合法的超大工程；在建立目标硬件基准后再调整，不能直接取消边界。
