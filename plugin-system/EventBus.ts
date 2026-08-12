type EventHandler = (data: unknown) => void

export class EventBus {
  private listeners: Map<string, Set<EventHandler>> = new Map()

  on(event: string, handler: EventHandler): () => void {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, new Set())
    }
    this.listeners.get(event)!.add(handler)

    return () => {
      this.listeners.get(event)?.delete(handler)
    }
  }

  emit(event: string, data?: unknown): void {
    const handlers = this.listeners.get(event)
    if (handlers) {
      for (const handler of handlers) {
        try {
          handler(data)
        } catch {
          // Swallow per-handler errors
        }
      }
    }
  }

  removeAll(event?: string): void {
    if (event) {
      this.listeners.delete(event)
    } else {
      this.listeners.clear()
    }
  }
}
