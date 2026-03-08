import { vi } from "vitest";

type EventCallback<T = unknown> = (event: { payload: T }) => void | Promise<void>;
type InvokeImplementation = (
  command: string,
  args?: unknown,
) => Promise<unknown>;
type ListenImplementation = (
  eventName: string,
  callback: EventCallback,
) => Promise<() => void>;

const listeners = new Map<string, Set<EventCallback>>();

const defaultInvokeImplementation: InvokeImplementation = async (
  _command,
  _args,
) => undefined;

const defaultListenImplementation: ListenImplementation = async (
  eventName,
  callback,
) => {
  const callbacks = listeners.get(eventName) ?? new Set<EventCallback>();
  callbacks.add(callback);
  listeners.set(eventName, callbacks);

  return () => {
    listeners.get(eventName)?.delete(callback);
  };
};

export const invokeMock = vi.fn(defaultInvokeImplementation);

export const listenMock = vi.fn(defaultListenImplementation);

export async function emitTauriEvent<T>(eventName: string, payload: T) {
  for (const callback of listeners.get(eventName) ?? []) {
    await callback({ payload });
  }
}

export function resetTauriMocks() {
  listeners.clear();
  invokeMock.mockReset();
  invokeMock.mockImplementation(defaultInvokeImplementation);
  listenMock.mockReset();
  listenMock.mockImplementation(defaultListenImplementation);
}
