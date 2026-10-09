import documentengineManifest from '../../plugins/document-engine/plugin.json'
import thememanagerManifest from '../../plugins/theme-manager/plugin.json'
import unienvManifest from '../../plugins/unienv/plugin.json'
import diaryManifest from '../../plugins/diary/plugin.json'
import turntableManifest from '../../plugins/turntable/plugin.json'
import gifeditorManifest from '../../plugins/gif-editor/plugin.json'
import archiveextractorManifest from '../../plugins/archive-extractor/plugin.json'
import nextOfficialPolicy from '../../contracts/next/official-plugins.json'
export interface MarketplacePlugin {
  id: string
  name: string
  version: string
  publisher: string
  category: string
  description: string
  highlights: string[]
  tags: string[]
  keywords?: string[]
  minHostVersion?: string
  icon?: string
  artifact?: string
  size?: number
  url?: string
}

const PRESERVED_BETA3_CATALOG: MarketplacePlugin[] = [
  {
    id: 'clipboard-manager',
    name: '剪贴板管理',
    version: '0.4.0-beta.3',
    publisher: 'CrucibleBox',
    category: '效率工具',
    description: '后台收集、搜索筛选、置顶、去重、导出与暂停记录。',
    highlights: ['后台收集', '置顶与筛选', '暂停记录'],
    tags: ['剪贴板', '历史记录', '效率'],
    keywords: ['复制', '置顶', '暂停记录'],
    minHostVersion: '2.1.0-beta.3'
  },
  {
    id: 'diary',
    name: '日记与笔记',
    version: '0.6.0-beta.2',
    publisher: 'CrucibleBox',
    category: '效率工具',
    description: '日历日记与笔记，支持 Markdown、草稿恢复、版本、双向链接和回收站。',
    highlights: ['日历与草稿', 'Markdown 与导出', '笔记版本与双向链接'],
    tags: ['日记', '笔记', 'Markdown'],
    keywords: ['日历', '草稿', '双向链接', '回收站'],
    minHostVersion: '2.1.0-beta.2'
  },
  {
    id: 'gif-editor',
    name: 'GIF 动画编辑',
    version: '0.6.0-beta.2',
    publisher: 'CrucibleBox',
    category: '图像与媒体',
    description: '逐帧编辑 GIF 动画，支持时间轴、画布裁剪/旋转、叠加残留修复和 GIF 导出。',
    highlights: ['逐帧时间轴', '裁剪与旋转', '叠加残留修复'],
    tags: ['GIF', '动画', '逐帧编辑'],
    keywords: ['时间轴', '裁剪', '旋转', '残留修复'],
    minHostVersion: '2.1.0-beta.2'
  },
  {
    id: 'turntable',
    name: '随机决策',
    version: '0.4.0-beta.2',
    publisher: 'CrucibleBox',
    category: '效率工具',
    description: '转盘权重抽取、转盘动画、结果历史与骰子投掷。',
    highlights: ['权重转盘', '转盘动画', '骰子投掷'],
    tags: ['随机', '转盘', '骰子'],
    keywords: ['抽签', '权重', '投骰子'],
    minHostVersion: '2.1.0-beta.2'
  },
  {
    id: 'exchange-rates',
    name: '汇率与单位换算',
    version: '0.4.0-beta.2',
    publisher: 'CrucibleBox',
    category: '效率工具',
    description: '实时汇率、币种收藏与长度、重量、温度等单位换算。',
    highlights: ['实时汇率', '币种收藏', '单位换算'],
    tags: ['汇率', '单位换算', '货币'],
    keywords: ['外币', '长度', '重量', '温度'],
    minHostVersion: '2.1.0-beta.2'
  },
  {
    id: 'archive-extractor',
    name: '压缩与解压',
    version: '0.2.0-beta.2',
    publisher: 'CrucibleBox',
    category: '文件工具',
    description: '使用内置 7-Zip 创建 ZIP/7z 归档，或离线解压常见格式。',
    highlights: ['创建 ZIP/7z', '批量解压', '密码与取消'],
    tags: ['压缩', '解压', 'ZIP', '7z'],
    keywords: ['归档', 'RAR', 'TAR', '密码'],
    minHostVersion: '2.1.0-beta.2'
  },
  {
    id: 'json-toolkit',
    name: '数据格式转换',
    version: '0.4.0-beta.2',
    publisher: 'CrucibleBox',
    category: '开发工具',
    description: 'JSON、JSON5、YAML、TOML、XML、CSV、TSV 解析与转换，附差异和路径查询。',
    highlights: ['七种数据格式', '结构差异', '路径查询'],
    tags: ['数据转换', 'JSON', 'YAML', 'CSV'],
    keywords: ['JSON5', 'TOML', 'XML', 'TSV'],
    minHostVersion: '2.1.0-beta.2'
  },
  {
    id: 'document-engine',
    name: '文档与知识库',
    version: '0.11.0-beta.2',
    publisher: 'CrucibleBox',
    category: '文档与 AI',
    description: '综合文件处理、PDF 工具、文字识别、内容提取、格式转换和知识库预处理。',
    highlights: [
      'Office、网页、数据与电子书解析',
      '结构、表格、公式与版面提取',
      'RAG 分块与批量工作流'
    ],
    tags: ['OCR', 'PDF', 'RAG', '文件转换'],
    minHostVersion: '2.1.0-beta.3'
  },
  {
    id: 'unienv',
    name: '开发环境管理',
    version: '0.12.0-beta.1',
    publisher: 'CrucibleBox',
    category: '开发环境',
    description: '多语言工具链安装、真实版本切换、镜像路由、项目清单和离线安装。',
    highlights: ['11 种工具链', '镜像不偷跑官方源', '项目环境与离线包'],
    tags: ['开发环境', '版本管理', '镜像下载']
  },
  {
    id: 'media-toolkit',
    name: '图片处理',
    version: '0.2.0-beta.2',
    publisher: 'CrucibleBox',
    category: '图像与媒体',
    description: '图片编辑与导出。',
    highlights: ['图片编辑', '导出图片'],
    tags: ['图片', '编辑'],
    minHostVersion: '2.1.0-beta.2'
  },
  {
    id: 'audio-video-processor',
    name: '音视频处理',
    version: '0.1.0-beta.2',
    publisher: 'CrucibleBox',
    category: '图像与媒体',
    description: '本地 FFmpeg 裁剪、转码、提取音频和画面；当前需自行提供 FFmpeg。',
    highlights: ['裁剪', '转码', '抽取音频与画面'],
    tags: ['音频', '视频', '转码', '裁剪'],
    minHostVersion: '2.1.0-beta.2'
  },
  {
    id: 'developer-toolkit',
    name: '接口调试',
    version: '0.2.0-beta.2',
    publisher: 'CrucibleBox',
    category: '开发工具',
    description: 'HTTP 与 GraphQL 请求、环境变量、Bearer 认证、集合、历史与导出。',
    highlights: ['HTTP/GraphQL', '请求集合', '环境变量与历史'],
    tags: ['接口', 'HTTP', 'GraphQL'],
    keywords: ['REST', '请求', '认证'],
    minHostVersion: '2.1.0-beta.2'
  },
  {
    id: 'theme-manager',
    name: '主题管理',
    version: '0.3.1',
    publisher: 'CrucibleBox',
    category: '个性化',
    description: '切换六种内置主题，也可保存个人配色。',
    highlights: ['亮色、深色与科幻面板', '像素街机、陶土工坊与纸张编辑', '自定义配色'],
    tags: ['主题', '外观']
  },
  {
    id: 'system-info',
    name: '系统信息面板',
    version: '0.3.0',
    publisher: 'CrucibleBox',
    category: '系统工具',
    description: '查看 CPU、内存、磁盘、系统与网络信息。',
    highlights: ['系统概况', '资源使用情况', '自动刷新'],
    tags: ['系统信息', '监控']
  }
]

const nextManifests = [
  documentengineManifest,
  thememanagerManifest,
  unienvManifest,
  diaryManifest,
  turntableManifest,
  gifeditorManifest,
  archiveextractorManifest
]
const nextOfficialIds = new Set(nextOfficialPolicy.officialPlugins.map(({ id }) => id))
export function isNextOfficialPlugin(id: string): boolean {
  return nextOfficialIds.has(id)
}
export const OFFICIAL_MARKETPLACE_CATALOG: MarketplacePlugin[] =
  nextOfficialPolicy.officialPlugins.map(({ id, displayName }) => {
    const metadata = PRESERVED_BETA3_CATALOG.find((plugin) => plugin.id === id)
    if (!metadata) throw new Error(`Missing official metadata: ${id}`)
    const manifest = nextManifests.find((item) => item.id === id)
    if (!manifest) throw new Error(`Missing Next manifest: ${id}`)
    return { ...metadata, name: displayName, version: manifest.version }
  })
