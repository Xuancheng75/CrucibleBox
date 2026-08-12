import React, { useEffect } from 'react'
import { App, Modal, Form, Input, InputNumber, Switch, Select, theme } from 'antd'
import type { PluginMeta, PluginConfig, ConfigField } from '@shared/types/plugin.types'
import { usePluginStore } from '../store/plugin.store'

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

  useEffect(() => {
    if (open && plugin?.id) {
      form.resetFields()
    }
  }, [open, plugin?.id, form])

  if (!plugin) return null

  const schema = plugin.configSchema || {}
  const initialValues = plugin.configData || {}

  const handleSave = async () => {
    try {
      const values = await form.validateFields()
      const success = await updatePluginConfig(plugin.id, values as PluginConfig)
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
          <Form.Item key={key} {...commonProps}>
            <Input placeholder={field.description} />
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
      title={`配置 - ${plugin.displayName}`}
      open={open}
      onCancel={onClose}
      onOk={handleSave}
      okText="保存"
      cancelText="取消"
      width={480}
    >
      {schemaKeys.length > 0 ? (
        <Form form={form} layout="vertical" style={{ marginTop: 16 }} initialValues={initialValues}>
          {schemaKeys.map((key) => renderField(key, schema[key]))}
        </Form>
      ) : (
        <div style={{ padding: '24px 0', textAlign: 'center', color: token.colorTextTertiary }}>
          此插件没有可配置项
        </div>
      )}
    </Modal>
  )
}
