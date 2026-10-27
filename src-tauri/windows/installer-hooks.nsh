; CrucibleBox NSIS installer hooks（tauri.conf.json → bundle.windows.nsis.installerHooks）
;
; 背景（Bug A）：替换应用 exe 后，Windows 资源管理器可能继续用按路径缓存的旧
; 图标渲染任务栏/桌面快捷方式，用户误判为"图标没换"。安装完成时广播
; SHCNE_ASSOCCHANGED 强制 shell 重新读取 exe 图标资源，无需用户手动重启 explorer。

!macro NSIS_HOOK_POSTINSTALL
  ; SHChangeNotify(SHCNE_ASSOCCHANGED=0x08000000, SHCNF_IDLIST=0, NULL, NULL)
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend
