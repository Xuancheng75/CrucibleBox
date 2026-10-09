import React, { useEffect, useState } from 'react'
import { App, Modal, Form, Input, InputNumber, Switch, Select, theme } from 'antd'
import type { PluginMeta, PluginConfig, ConfigField } from '../../../shared/types/plugin.types'
import { usePluginStore } from '../store/plugin.store'
import { tauriApi } from '../api/tauriApi'

interface PluginConfigProps {
  plugin: PluginMeta | null
  open: boolean
  onClose: () => void
}

export default function PluginConfig({ plugin, open, onClose }: PluginConfigProps) {
  const { token } = theme.useToken()
  const { message } = App.useApp()
  const [form] = Form.useForm()
  const updatePluginConfig = usePluginStore((s) => s.updatePluginConfig)
  const plugins = usePluginStore((s) => s.plugins)
  const enablePlugin = usePluginStore((s) => s.enablePlugin)
  const disablePlugin = usePluginStore((s) => s.disablePlugin)
  const [toggleBusy, setToggleBusy] = useState(false)

  useEffect(() => {
    if (open && plugin?.id) {
      form.resetFields()
    }
  }, [open, plugin?.id, form])

  if (!plugin) return null

  const current = plugins.find((item) => item.id === plugin.id) ?? plugin
  const schema = current.configSchema || {}
  const initialValues = current.configData || {}

  const handleToggle = async (enabled: boolean) => {
    if (toggleBusy) return
    setToggleBusy(true)
    try {
      const ok = enabled ? await enablePlugin(current.id) : await disablePlugin(current.id)
      if (ok) message.success(enabled ? '插件已启用' : '插件已停用')
      else message.error('操作失败')
    } finally {
      setToggleBusy(false)
    }
  }

  const handleSave = async () => {
    try {
      const values = await form.validateFields()
      const success = await updatePluginConfig(current.id, values as PluginConfig)
      if (success) {
        message.success('配置已保存')
        onClose()
      } else {
        message.error('保存失败')
      }
    } catch {
      // validation failed
    }
  }

  const renderField = (key: string, field: ConfigField) => {
    const commonProps = {
      label: field.label,
      name: key,
      rules: field.required ? [{ required: true, message: `请输入${field.label}` }] : undefined
    }

    switch (field.type) {
      case 'string':
        return (
          <Form.Item key={key} {...commonProps} extra={field.description}>
            {key === 'formulaAddonDirectory' ? (
              <Input.Search
                placeholder="选择已解压的高精度附加包目录"
                enterButton="浏览…"
                onSearch={() => {
                  void tauriApi.dialog.openDirectory().then((path) => {
                    if (path) form.setFieldValue(key, path)
                  })
                }}
              />
            ) : (
              <Input placeholder={field.description} />
            )}
          </Form.Item>
        )
      case 'number':
        return (
          <Form.Item key={key} {...commonProps}>
            <InputNumber style={{ width: '100%' }} placeholder={field.description} />
          </Form.Item>
        )
      case 'boolean':
        return (
          <Form.Item key={key} {...commonProps} valuePropName="checked">
            <Switch />
          </Form.Item>
        )
      case 'select':
        return (
          <Form.Item key={key} {...commonProps}>
            <Select options={field.options} placeholder={field.description} />
          </Form.Item>
        )
      case 'multiselect':
        return (
          <Form.Item key={key} {...commonProps}>
            <Select mode="multiple" options={field.options} placeholder={field.description} />
          </Form.Item>
        )
      default:
        return null
    }
  }

  const schemaKeys = Object.keys(schema)

  return (
    <Modal
      title={`插件配置 · ${current.displayName}`}
      open={open}
      onCancel={onClose}
      onOk={schemaKeys.length > 0 ? handleSave : onClose}
      okText={schemaKeys.length > 0 ? '保存' : '完成'}
      cancelText="取消"
      width={480}
    >
      <div className="cbx-plugin-config-general" style={{ marginTop: 12, marginBottom: 18 }}>
        <div
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            gap: 16,
            fontSize: 12,
            color: token.colorTextSecondary,
            marginBottom: 14
          }}
        >
          <span>版本 {current.version}</span>
          <span>{current.author || '未知发布者'}</span>
        </div>
        <div
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            gap: 16,
            padding: '12px 0',
            borderTop: `1px solid ${token.colorBorderSecondary}`
          }}
        >
          <div>
            <strong style={{ fontSize: 13 }}>启用插件</strong>
            <div style={{ color: token.colorTextSecondary, fontSize: 11, marginTop: 4 }}>
              停用后保留插件及其配置
            </div>
          </div>
          <Switch
            checked={current.enabled}
            loading={toggleBusy}
            onChange={(value) => void handleToggle(value)}
            aria-label={`启用 ${current.displayName}`}
          />
        </div>
      </div>
      {schemaKeys.length > 0 ? (
        <Form form={form} layout="vertical" style={{ marginTop: 16 }} initialValues={initialValues}>
          {schemaKeys.map((key) => renderField(key, schema[key]))}
        </Form>
      ) : (
        <div style={{ padding: '24px 0', textAlign: 'center', color: token.colorTextTertiary }}>
          此插件未提供专属配置项
        </div>
      )}
    </Modal>
  )
}
