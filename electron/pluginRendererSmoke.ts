// ARCHIVED (Electron line, frozen until 1.9.2) - see docs/electron-legacy-registry.md
import type { WebContents } from 'electron'

export interface PluginRendererSmokeResult {
  backendResponsive: boolean | null
  isolated: boolean
  layoutHeight: number
  rendererApiVersion: number
}

export async function runPluginRendererSmoke(
  webContents: WebContents,
  pluginId: string,
  backendExpected: boolean
): Promise<PluginRendererSmokeResult> {
  const result = await webContents.executeJavaScript(`(async () => {
    const pluginId = ${JSON.stringify(pluginId)};
    const plugin = await window.electronAPI.plugin.get(pluginId);
    if (!plugin) throw new Error('smoke plugin metadata is unavailable');
    const backendExpected = ${JSON.stringify(backendExpected)};
    const backendResponse = backendExpected
      ? await window.electronAPI.plugin.sendMessage(pluginId, { type: 'ping' })
      : null;
    const waitFor = async (read, description) => {
      const deadline = Date.now() + 10000;
      while (Date.now() < deadline) {
        const value = read();
        if (value) return value;
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
      throw new Error(description + ' timed out');
    };
    const moduleName = plugin.name.toUpperCase().slice(0, 12);
    const card = await waitFor(
      () => document.querySelector('.ob-launcher[data-module="' + CSS.escape(moduleName) + '"]'),
      'plugin launcher'
    );
    const openButton = card.querySelector('.ob-launcher-open');
    if (!openButton) throw new Error('plugin launcher button is unavailable');
    openButton.click();
    const iframe = await waitFor(
      () => document.querySelector('iframe[data-plugin-ready="true"]'),
      'isolated plugin frame'
    );
    let isolated = false;
    try {
      void iframe.contentWindow.document.documentElement;
    } catch {
      isolated = true;
    }
    return {
      backendResponsive: backendExpected ? backendResponse?.ok === true : null,
      isolated,
      layoutHeight: Number.parseInt(iframe.style.height, 10),
      rendererApiVersion: Number(iframe.dataset.rendererApiVersion)
    };
  })()`)
  return result as PluginRendererSmokeResult
}
