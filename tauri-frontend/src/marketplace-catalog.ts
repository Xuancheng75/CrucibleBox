export interface MarketplacePlugin {
  id: string
  name: string
  version: string
  publisher: string
  category: string
  description: string
  highlights: string[]
  minHostVersion?: string
  icon?: string
  artifact?: string
  sha256?: string
  size?: number
  url?: string
}

export const OFFICIAL_MARKETPLACE_CATALOG: MarketplacePlugin[] = [
  { id: 'document-engine', name: 'Document Engine', version: '0.7.0', publisher: 'CrucibleBox', category: '文档与 AI', description: '本地 PDF、轻量 OCR、版面分流、公式 LaTeX、结构化解析、格式转换和 RAG 数据准备。', highlights: ['文本清洗与章节树', 'Markdown/LaTeX 输出', 'Chunk 与真实 PDF 拆分'], minHostVersion: '2.0.0-beta.6' },
  { id: 'unienv', name: 'UniEnv', version: '0.11.0', publisher: 'CrucibleBox', category: '开发环境', description: '多语言开发环境检测、安装、切换和项目配置。', highlights: ['11 种工具链', 'Rust/PHP 多版本', '镜像与组合包'] },
  { id: 'diary', name: '日记', version: '0.5.0', publisher: 'CrucibleBox', category: '记录与写作', description: '支持 Markdown、LaTeX、日历与结构化整理的本地日记。', highlights: ['Markdown/LaTeX', '日历视图', '本地存储'] },
  { id: 'gif-editor', name: 'GIF 编辑器', version: '0.5.0', publisher: 'CrucibleBox', category: '图像与媒体', description: '时间轴式 GIF 拆帧、编辑、优化和导出工具。', highlights: ['逐帧编辑', '批量删除', '重新合成'] },
  { id: 'clipboard-manager', name: '剪贴板管理器', version: '0.3.0', publisher: 'CrucibleBox', category: '效率工具', description: '保存、搜索、分类并快速回填剪贴板文本。', highlights: ['内容分类', '筛选与置顶', 'JSON 导出'] },
  { id: 'json-toolkit', name: 'JSON/文本工具箱', version: '0.3.0', publisher: 'CrucibleBox', category: '开发工具', description: 'JSON、文本编码、校验、查询和格式转换工具集。', highlights: ['点路径查询', '结构化 Diff', '编码与哈希'] },
  { id: 'turntable', name: '转盘抽奖', version: '0.3.0', publisher: 'CrucibleBox', category: '随机与决策', description: '带权重、排除和历史记录的随机决策工具。', highlights: ['权重配置', '防连续重复', '最近中奖记录'] },
  { id: 'dice-roller', name: '骰子与随机数', version: '0.3.0', publisher: 'CrucibleBox', category: '随机与决策', description: '支持桌游表达式、快捷预设和优势/劣势的随机工具。', highlights: ['NdM±K 表达式', '优势/劣势', '会话历史'] },
  { id: 'exchange-rates', name: '实时汇率', version: '0.3.0', publisher: 'CrucibleBox', category: '数据与查询', description: '多数据源汇率换算、缓存和来源标识。', highlights: ['币种交换', '常用货币收藏', '备用数据源'] },
  { id: 'system-info', name: '系统信息面板', version: '0.3.0', publisher: 'CrucibleBox', category: '系统工具', description: '低开销展示系统资源、趋势和诊断快照。', highlights: ['资源趋势', '阈值提醒', '诊断导出'] },
  { id: 'theme-manager', name: '主题管理', version: '0.3.0', publisher: 'CrucibleBox', category: '个性化', description: '管理 16 套各具风格的主题，并提供安全预览和自定义主题。', highlights: ['16 套主题', '预览与回滚', '导入与导出'] }
]
