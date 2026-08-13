# OpenBox 1.5.0 双主题版本

本版本在 OpenBox 1.4.2 源码基础上增加并完善两套赛博朋克主题：

- `cyber` / 赛博朋克：对称 HUD、深蓝黑网格、全息玻璃、扫描束、霓虹辉光和 glitch。
- `neon-district` / 零号城区：纵向城区数据脊线、非对称模块缺口、模块编号、三色信号轨和酸黄警戒标识。

两套主题使用不同的专属 CSS 构图，同时共享 OpenBox 主题 Token、antd Token 和插件 CSS 变量体系。

## 关键文件

- `shared/themes/presets.ts`：主题 Token 和内置预设。
- `src/styles/global.css`：两套主题的专属视觉规则与减少动态效果适配。
- `src/components/ThemeProvider.tsx`：写入 `data-ob-theme` 和 CSS 变量。
- `electron/theme.service.ts`：主题持久化、预设规范化和窗口广播。
- `themes/cyber.json`：赛博朋克主题导入文件。
- `themes/neon-district.json`：零号城区主题导入文件。

## 本地验证

```powershell
npm install
npm run typecheck
npm run lint
npm run test
npm run build
```

验证通过后，可使用 `npm run dev` 启动开发版，或执行 `npm run package` 生成 Windows 安装包。

## 应用主题

1. 运行由本源码构建的 OpenBox 1.5.0。
2. 在工作台导入 `theme-manager-0.1.6.zip`。
3. 打开「主题管理」。
4. 点击「赛博朋克」或「零号城区」。

也可在「备份 / 恢复」区域导入 `themes/*.json`。

## 兼容性

| 主程序 | ThemeManager | cyber | 零号城区 |
| --- | --- | --- | --- |
| OpenBox 1.5.0 | 0.1.6 | 完整效果 | 完整效果 |
| OpenBox 1.5.0 | 0.1.5 | 可直接选择 | 需导入 JSON |
| 原始 OpenBox 1.4.2 | 0.1.6 | 原版本效果 | 仅基础 Token，无专属布局特效 |

完整效果依赖主程序中的 `global.css`。主题插件负责选择和持久化，不能单独把新布局特效带入未更新的旧主程序。
