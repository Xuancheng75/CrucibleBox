# M2.7 宿主页面级加载与启动体积门禁

## 决策

保留 React 19、antd 5 和 zustand。当前 UI、token 与状态边界稳定，迁移到 MUI/Chakra 或
Redux/Jotai 会扩大回归面，却不会直接减少启动加载。React Server Components 不适合本地静态
Electron renderer；采用现代 Client Components、`React.lazy` 和 Vite 动态入口。

## 实现

- `src/app-pages.ts` 集中声明页面 ID、类型和动态模块加载器。
- `App` 只静态加载布局、主题和全局事件；工作台、日志、设置、插件详情由 `Suspense` 按需加载。
- app store 与导航组件共享 `AppPage`，不能写入未声明页面。
- Vite 输出 manifest；性能脚本递归计算入口的静态 import 闭包，而不是依赖散列文件名。
- 门禁分别限制 renderer 总 JS、静态入口和“入口 + 默认首页”启动闭包，避免通过把必需代码伪装成
  dynamic import 来粉饰启动指标。

## 结果

| 指标             | M2.6        | M2.7        | 预算        |
| ---------------- | ----------- | ----------- | ----------- |
| renderer 总 JS   | 2,777,573 B | 2,790,229 B | 3,400,000 B |
| 静态入口         | 2,777,573 B | 1,091,990 B | 1,300,000 B |
| 默认首页启动闭包 | 2,777,573 B | 2,018,469 B | 2,300,000 B |

静态入口减少约 60.7%，默认首页实际启动闭包减少约 27.3%；总代码只增加约 12 KB。页面切换仍使用原
zustand 状态和同一 `MainLayout`，插件 iframe/RPC 生命周期不变。

## 验证

- 页面注册表测试覆盖完整键集合、独立加载器和非法页面 ID。
- `npm run check`：宿主 25 文件/163 项，GIF Editor 35 项，UniEnv 116 项，供应链 3 项。
- `npm run build` 必须生成四个页面动态入口并通过三层宿主体积预算及全部既有插件预算。
- Windows packaged smoke 验证 `file://` 下动态 import、首页加载、插件详情 iframe、两个 utility backend、
  UniEnv 可信服务与旧数据库迁移；结果为 1,090 ms、447,532 KiB working set。
