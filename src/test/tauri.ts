import { vi } from "vitest";

type EventCallback<T = unknown> = (event: { payload: T }) => void | Promise<void>;

const listeners = new Map<string, Set<EventCallback>>();

export const invokeMock = vi.fn(
  async (_command: string, _args?: unknown): Promise<unknown> => undefined,
);

export const listenMock = vi.fn(
  async (eventName: string, callback: EventCallback) => {
    const callbacks = listeners.get(eventName) ?? new Set<EventCallback>();
    callbacks.add(callback);
    listeners.set(eventName, callbacks);

    return () => {
      listeners.get(eventName)?.delete(callback);
    };
  },
);

export async function emitTauriEvent<T>(eventName: string, payload: T) {
  for (const callback of listeners.get(eventName) ?? []) {
    await callback({ payload });
  }
}

export function resetTauriMocks() {
  invokeMock.mockReset();
  listenMock.mockReset();
  listeners.clear();
}
