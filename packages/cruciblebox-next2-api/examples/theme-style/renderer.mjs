// Neutral public SDK example: no business assets or named visual themes.
import { createClient } from '@cruciblebox/next2-ui'
export function mount(context) {
  const api = createClient(context)
  const section = document.createElement('section')
  section.style.cssText = 'padding:20px;box-sizing:border-box;display:grid;gap:12px'
  section.innerHTML =
    '<h1>通用主题平台样例</h1><p>预览后切换宿主页和本页，检查样式、页面保留、减少动效与恢复。</p><label>导航 <select class="ob-select"><option value="rail">侧栏</option><option value="top">顶部</option><option value="menu">菜单</option></select></label><label>动效 <select data-motion class="ob-select"><option value="full">完整</option><option value="reduced">减少</option><option value="off">关闭</option></select></label><textarea class="ob-input" placeholder="切页保留的草稿"></textarea><button class="ob-button" data-action="preview"><span>预览中性设计</span></button><button class="ob-button" data-action="image"><span>导入自制图片并预览</span></button><button class="ob-button" data-action="commit"><span>保存预览</span></button><button class="ob-button" data-action="rollback"><span>撤销预览</span></button><div class="ob-surface-card" style="padding:16px;border:1px solid var(--ob-color-border)">通用组件样式卡片</div><p role="status"></p>'
  context.root.replaceChildren(section)
  const status = section.querySelector('[role=status]')
  let imageId
  const click = async (event) => {
    const action = event.target.closest('button')?.dataset.action
    if (!action) return
    try {
      if (action === 'commit' || action === 'rollback') {
        status.textContent = String(await api.theme[action]())
        return
      }
      if (action === 'image') {
        const files = await api.platform('files.choose', {
          type: 'file',
          multiple: false,
          extensions: ['png', 'jpg', 'jpeg', 'webp']
        })
        if (!files?.length) {
          status.textContent = '已取消'
          return
        }
        const asset = await api.theme.assets.import({
          fileRef: files[0].ref,
          license: 'User-created'
        })
        imageId = asset.id
      }
      const current = await api.theme.get()
      const design = {
        version: 2,
        layout: {
          navigation: section.querySelector('select').value,
          contentPadding: 18,
          cardGap: 20
        },
        components: {
          buttonRadius: 4,
          cardRadius: 12,
          inputRadius: 6,
          buttonSkew: -3,
          surfaceShadow: 'soft'
        },
        motion: {
          level: section.querySelector('[data-motion]').value,
          transition: 'slide',
          durationMs: 180
        },
        layers: [
          {
            id: 'accent',
            type: 'shape',
            shape: 'polygon',
            scope: 'shell',
            x: 55,
            y: 0,
            width: 45,
            height: 100,
            rotation: 0,
            opacity: 0.12,
            order: 0,
            color: '#58779c',
            points: [
              { x: 70, y: 0 },
              { x: 100, y: 100 },
              { x: 0, y: 100 }
            ],
            motion: 'float'
          }
        ],
        css: '.ob-surface-card{border-style:solid}'
      }
      if (imageId) {
        design.assets = [imageId]
        design.layers.push({
          id: 'art',
          type: 'image',
          scope: 'workspace',
          x: 65,
          y: 55,
          width: 30,
          height: 35,
          rotation: 0,
          opacity: 0.3,
          order: 1,
          assetId: imageId
        })
      }
      await api.theme.preview({
        ...current,
        id: 'neutral-platform-v2',
        name: '中性平台设计',
        design
      })
      status.textContent = '预览已应用；保存后重启可核查持久化。'
    } catch (error) {
      status.textContent = String(error)
    }
  }
  section.addEventListener('click', click)
  return () => {
    section.removeEventListener('click', click)
    context.root.replaceChildren()
  }
}
