export interface TurntableItem {
  id: number
  label: string
  weight: number
  color: string
  sort_order: number
  created_at: string
}

export type MessageType =
  | 'getItems'
  | 'addItem'
  | 'updateItem'
  | 'deleteItem'
  | 'reorderItems'
  | 'spin'

export interface PluginMessage {
  type: MessageType
  payload?: unknown
}

export interface AddItemPayload {
  label: string
  weight: number
  color: string
}

export interface UpdateItemPayload {
  id: number
  label?: string
  weight?: number
  color?: string
}

export interface DeleteItemPayload {
  id: number
}

export interface ReorderPayload {
  ids: number[]
}

export interface SpinResult {
  winner: TurntableItem
}
