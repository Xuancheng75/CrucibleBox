# API 6 JavaScript runner

独立 QuickJS 进程，只接受 `--next2 <plugin-directory> <session>`，执行 wire 4 握手与有界帧协议。stdout 仅输出协议，诊断写 stderr。CommonJS 加载限于插件根目录，不提供 Node.js。

平台能力通过宿主交换请求；具体 backend 方法由插件提供，runner 不包含业务方法枚举。资源、文件与任务身份来自宿主初始化，停用/升级/卸载/退出由通用监督器停止进程树。

cargo fmt --check、cargo clippy --all-targets -- -D warnings、cargo test 是该 crate 的既有门禁；端到端行为由宿主 API 6 集成测试及 next2-runtime 的 test-backend 验证。旧 wire 3 和更旧 ctx 执行代码已归档。
