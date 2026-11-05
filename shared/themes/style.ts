import * as css from 'css-tree'
import type { ThemeAsset, RendererThemeStyle, ToolboxTheme } from '../types/theme.types'

const SCOPE = '[data-ob-theme-version="2"]'
const ASSET_URL = /^http:\/\/cruciblebox-theme\.localhost\/[a-f0-9]{64}\/[a-f0-9]{64}$/
const KEYFRAME_PREFIX = 'cbx-user-'

export function effectiveMotion(
  level: 'full' | 'reduced' | 'off' = 'full',
  systemReduced = false
): 'full' | 'reduced' | 'off' {
  return systemReduced && level === 'full' ? 'reduced' : level
}

function scopedCss(source: string, assets: Map<string, ThemeAsset>, motion: string): string {
  const ast = css.parse(source, { positions: false, parseCustomProperty: true })
  const keyframes = new Map<string, string>()
  css.walk(ast, {
    enter(node: css.CssNode) {
      if (
        node.type === 'PseudoElementSelector' &&
        [
          'view-transition-group',
          'view-transition-image-pair',
          'view-transition-old',
          'view-transition-new'
        ].includes(node.name)
      ) {
        const value = node.children?.first
        if (
          node.children?.size !== 1 ||
          value?.type !== 'Raw' ||
          !/^[a-z_][a-z0-9_-]{0,63}$/i.test(value.value)
        )
          throw new Error('THEME_CSS_UNSUPPORTED_SYNTAX')
        node.children = new css.List<css.CssNode>()
        node.children.appendData({ type: 'Identifier', name: value.value })
      }
      if (node.type === 'Raw' || node.type === 'NestingSelector')
        throw new Error('THEME_CSS_UNSUPPORTED_SYNTAX')
      if (node.type === 'Atrule') {
        const name = css.ident.decode(node.name).toLowerCase()
        node.name = name
        if (!['media', 'supports', 'container', 'keyframes', '-webkit-keyframes'].includes(name))
          throw new Error(`THEME_CSS_AT_RULE_DENIED: ${name}`)
        if (name.endsWith('keyframes')) {
          const value = node.prelude?.type === 'AtrulePrelude' ? node.prelude.children.first : null
          if (!value || !['Identifier', 'String'].includes(value.type))
            throw new Error('THEME_CSS_KEYFRAMES_NAME')
          const original =
            value.type === 'Identifier'
              ? css.ident.decode(value.name)
              : value.type === 'String'
                ? value.value
                : ''
          keyframes.set(original, KEYFRAME_PREFIX + original)
          if (value.type === 'Identifier') value.name = css.ident.encode(KEYFRAME_PREFIX + original)
          else if (value.type === 'String') value.value = KEYFRAME_PREFIX + original
        }
      }
      if (node.type === 'Url') {
        const match = /^asset:([a-f0-9]{64})$/.exec(node.value)
        const asset = match && assets.get(match[1])
        if (!asset || !ASSET_URL.test(asset.url)) throw new Error('THEME_CSS_LOCAL_ASSET_REQUIRED')
        node.value = asset.url
      }
      if (
        node.type === 'Function' &&
        ['expression', 'image', 'image-set', '-webkit-image-set', 'src'].includes(
          css.ident.decode(node.name).toLowerCase()
        )
      )
        throw new Error('THEME_CSS_FUNCTION_DENIED')
    }
  })
  css.walk(ast, {
    visit: 'Rule',
    enter(node) {
      if (this.atrule?.name.toLowerCase().endsWith('keyframes')) return
      if (node.prelude.type !== 'SelectorList') throw new Error('THEME_CSS_SELECTOR_REQUIRED')
      node.prelude.children.forEach((child) => {
        const selector = child as css.Selector
        const first = selector.children.first
        const isRoot =
          first &&
          ((first.type === 'PseudoClassSelector' &&
            css.ident.decode(first.name).toLowerCase() === 'root') ||
            (first.type === 'TypeSelector' &&
              css.ident.decode(first.name).toLowerCase() === 'html'))
        if (isRoot) selector.children.shift()
        const prefix = css.parse(SCOPE, { context: 'selector' }) as css.Selector
        if (!isRoot) prefix.children.appendData({ type: 'Combinator', name: ' ' })
        selector.children.prependList(prefix.children)
      })
    }
  })
  css.walk(ast, {
    visit: 'Declaration',
    enter(node, item, list) {
      const property = css.ident.decode(node.property).toLowerCase()
      if (['behavior', '-moz-binding'].includes(property))
        throw new Error('THEME_CSS_PROPERTY_DENIED')
      if (motion !== 'full' && /^(?:-webkit-)?(?:animation|transition)(?:-|$)/.test(property)) {
        if (item) list.remove(item)
        return
      }
      if (
        !['animation', 'animation-name', '-webkit-animation', '-webkit-animation-name'].includes(
          property
        )
      )
        return
      let variable = false
      css.walk(node.value, (child) => {
        if (child.type === 'Function' && child.name.toLowerCase() === 'var') variable = true
      })
      if (variable) throw new Error('THEME_CSS_STATIC_ANIMATION_NAME_REQUIRED')
      const match = css.lexer.matchProperty(property.replace(/^-webkit-/, ''), node.value)
      if (match.error) throw new Error('THEME_CSS_INVALID_ANIMATION')
      css.walk(node.value, (child) => {
        if (!match.isType(child, 'keyframes-name')) return
        const name =
          child.type === 'Identifier'
            ? css.ident.decode(child.name)
            : child.type === 'String'
              ? child.value
              : ''
        if (name === 'none') return
        const renamed = keyframes.get(name)
        if (!renamed) throw new Error('THEME_CSS_UNDECLARED_ANIMATION')
        if (child.type === 'Identifier') child.name = css.ident.encode(renamed)
        else if (child.type === 'String') child.value = renamed
      })
    }
  })
  return css.generate(ast)
}

export function compileThemeStyle(
  theme: ToolboxTheme,
  assetList: ThemeAsset[] = [],
  systemReduced = false
): RendererThemeStyle {
  const design = theme.design
  if (!design) return { version: 2, css: '', motion: 'off', layers: [] }
  const assets = new Map(assetList.map((asset) => [asset.id, asset]))
  for (const id of design.assets ?? [])
    if (!assets.has(id)) throw new Error(`THEME_ASSET_MISSING: ${id}`)
  const motion = effectiveMotion(design.motion?.level, systemReduced)
  const components = design.components ?? {}
  const radius = theme.tokens.borderRadius
  const vars: Record<string, string | number> = {
    '--ob-button-radius': `${components.buttonRadius ?? radius}px`,
    '--ob-card-radius': `${components.cardRadius ?? radius}px`,
    '--ob-input-radius': `${components.inputRadius ?? radius}px`,
    '--ob-border-width': `${components.borderWidth ?? 1}px`,
    '--ob-button-skew': `${components.buttonSkew ?? 0}deg`,
    '--ob-heading-weight': components.headingWeight ?? 700,
    '--ob-card-gap': `${design.layout?.cardGap ?? 16}px`,
    '--ob-content-padding': `${design.layout?.contentPadding ?? 22}px`,
    '--ob-content-max-width': `${design.layout?.contentMaxWidth ?? 2560}px`,
    '--ob-theme-duration': `${motion === 'off' ? 0 : motion === 'reduced' ? Math.min(design.motion?.durationMs ?? 180, 120) : (design.motion?.durationMs ?? 180)}ms`,
    '--ob-page-transition-display':
      motion === 'full' && (design.motion?.transition ?? 'none') !== 'none' ? 'block' : 'none',
    '--ob-surface-shadow':
      components.surfaceShadow === 'hard'
        ? '5px 5px 0 var(--ob-color-border)'
        : components.surfaceShadow === 'soft'
          ? '0 8px 28px #0002'
          : 'none'
  }
  let output = `${SCOPE}{${Object.entries(vars)
    .map(([key, value]) => `${key}:${value}`)
    .join(';')}}`
  for (const font of design.fonts ?? []) {
    const asset = assets.get(font.assetId)
    if (!asset || asset.kind !== 'font' || !ASSET_URL.test(asset.url))
      throw new Error('THEME_FONT_ASSET_REQUIRED')
    output += `@font-face{font-family:"${font.family}";src:url("${asset.url}");font-weight:100 900;font-display:swap}`
  }
  output += `${SCOPE} .ob-button{border-radius:var(--ob-button-radius);transform:skewX(var(--ob-button-skew))}${SCOPE} .ob-button>span{display:inline-block;transform:skewX(calc(-1 * var(--ob-button-skew)))}${SCOPE} .ob-surface-card{border-radius:var(--ob-card-radius);box-shadow:var(--ob-surface-shadow);border-width:var(--ob-border-width)}${SCOPE} :is(.ob-input,.ob-select,.ob-select-content){border-radius:var(--ob-input-radius)}${SCOPE} :is(h1,h2,h3,h4){font-weight:var(--ob-heading-weight)}${SCOPE} .ob-main-content{padding:var(--ob-content-padding)!important}${SCOPE} .ob-main-surface{max-width:var(--ob-content-max-width);margin-inline:auto}${SCOPE} .ob-market-grid{gap:var(--ob-card-gap)}.cbx-theme-decorations{position:fixed;inset:0;pointer-events:none;overflow:hidden;z-index:0}.cbx-theme-decorations>div{position:absolute;display:none;background-size:cover;background-position:center;white-space:pre-wrap}.ob-app-layout{position:relative;z-index:1}`
  output += `${SCOPE}[data-ob-theme-surface=host] .cbx-theme-decorations>[data-layer-scope=shell],${SCOPE}[data-ob-theme-surface=host][data-ob-theme-page=home] .cbx-theme-decorations>[data-layer-scope=home],${SCOPE}[data-ob-theme-surface=host][data-ob-theme-page=pluginView] .cbx-theme-decorations>[data-layer-scope=workspace],${SCOPE}[data-ob-theme-surface=plugin] .cbx-theme-decorations>[data-layer-scope=workspace]{display:block}`
  for (const layer of design.layers ?? []) {
    const color = layer.color ?? theme.tokens.colorPrimary
    let image = ''
    if (layer.type === 'image') {
      const asset = assets.get(layer.assetId ?? '')
      if (!asset || asset.kind !== 'image' || !ASSET_URL.test(asset.url))
        throw new Error('THEME_IMAGE_ASSET_REQUIRED')
      image = `background-image:url("${asset.url}");`
    }
    const clip =
      layer.shape === 'ellipse'
        ? 'ellipse(50% 50% at 50% 50%)'
        : layer.shape === 'polygon'
          ? `polygon(${layer.points!.map((p) => `${p.x}% ${p.y}%`).join(',')})`
          : 'none'
    output += `${SCOPE} .cbx-theme-decorations>[data-layer-id="${layer.id}"]{left:${layer.x}%;top:${layer.y}%;width:${layer.width}%;height:${layer.height}%;opacity:${layer.opacity};z-index:${layer.order};--cbx-layer-angle:${layer.rotation}deg;transform:rotate(var(--cbx-layer-angle));clip-path:${clip};${layer.type === 'shape' ? 'background-color' : 'color'}:${color};${image}${motion === 'full' && layer.motion && layer.motion !== 'none' ? `animation:cbx-theme-${layer.motion} ${layer.durationMs ?? 3000}ms ease-in-out infinite alternate;` : ''}}`
  }
  if (design.layers?.length)
    output += `${SCOPE} :is(.ob-app-layout,.ob-app-main-layout,#root){background:transparent!important}`
  output +=
    '@keyframes cbx-theme-float{to{transform:translateY(-12px) rotate(var(--cbx-layer-angle))}}@keyframes cbx-theme-tilt{to{transform:rotate(calc(var(--cbx-layer-angle) + 3deg))}}'
  const transition = design.motion?.transition ?? 'none'
  if (motion === 'full' && transition !== 'none') {
    const effect = transition
    output += `${SCOPE} .cbx-page-instance:not([hidden]){animation:cbx-page-${effect} var(--ob-theme-duration) cubic-bezier(.16,1,.3,1) both}${SCOPE} iframe[data-plugin-ready=true]{animation:cbx-page-${effect} var(--ob-theme-duration) ease-out both}@keyframes cbx-page-fade{from{opacity:0}to{opacity:1}}@keyframes cbx-page-slide{from{opacity:0;transform:translate(24px,6px)}to{opacity:1;transform:translate(0,0)}}`
  }
  output += `${SCOPE}[data-ob-page-visible=false] *{animation-play-state:paused!important;transition:none!important}`
  if (motion !== 'full')
    output += `${SCOPE} *,${SCOPE} *::before,${SCOPE} *::after{animation:none!important;transition-duration:var(--ob-theme-duration)!important;scroll-behavior:auto!important}`
  output += scopedCss(design.css ?? '', assets, motion)
  if (new TextEncoder().encode(output).length > 49152)
    throw new Error('THEME_STYLE_BUDGET_EXCEEDED')
  const result: RendererThemeStyle = {
    version: 2,
    css: output,
    motion,
    layers: (design.layers ?? []).map((layer) => ({
      id: layer.id,
      scope: layer.scope,
      text: layer.type === 'text' ? (layer.text ?? '') : ''
    }))
  }
  if (new TextEncoder().encode(JSON.stringify(result)).length > 65536)
    throw new Error('THEME_STYLE_BUDGET_EXCEEDED')
  return result
}
