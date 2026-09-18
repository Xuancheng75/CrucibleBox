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
  size?: number
  url?: string
}

export const OFFICIAL_MARKETPLACE_CATALOG: MarketplacePlugin[] = [
  { id: 'document-engine', name: '文档与知识库', version: '0.11.0-beta.1', publisher: 'CrucibleBox', category: '文档与 AI', description: '综合文件处理、PDF 工具、文字识别、内容提取、格式转换和知识库预处理。', highlights: ['Office、网页、数据与电子书解析', '结构、表格、公式与版面提取', 'RAG 分块与批量工作流'], minHostVersion: '2.1.0-beta.1' },
  { id: 'unienv', name: '开发环境管理', version: '0.12.0-beta.1', publisher: 'CrucibleBox', category: '开发环境', description: '多语言工具链安装、真实版本切换、镜像路由、项目清单和离线安装。', highlights: ['11 种工具链', '镜像不偷跑官方源', '项目环境与离线包'] },
  { id: 'media-toolkit', name: '图片与音视频', version: '0.1.0-beta.1', publisher: 'CrucibleBox', category: '图像与媒体', description: '图片非破坏编辑、动画帧时间轴、音视频预览与视频抽帧。', highlights: ['图片编辑与导出', '动画帧项目', '视频预览与抽帧'], minHostVersion: '2.1.0-beta.1' },
  { id: 'developer-toolkit', name: '数据与接口工具', version: '0.1.0-beta.1', publisher: 'CrucibleBox', category: '开发工具', description: '结构化数据、文本编码、差异查询及 HTTP 接口调试。', highlights: ['JSON/CSV/XML', '编码、正则与 JWT', 'REST/GraphQL 请求'], minHostVersion: '2.1.0-beta.1' },
  { id: 'productivity-toolkit', name: '笔记与效率', version: '0.1.0-beta.1', publisher: 'CrucibleBox', category: '效率工具', description: '笔记、日记、剪贴板、随机决策和常用换算。', highlights: ['笔记版本与双向链接', '剪贴板收集', '权重抽取与换算'], minHostVersion: '2.1.0-beta.1' }
]
