import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach, vi } from "vitest";
import { invokeMock, listenMock, resetTauriMocks } from "./tauri";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: Parameters<typeof invokeMock>) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: Parameters<typeof listenMock>) => listenMock(...args),
}));

beforeEach(() => {
  resetTauriMocks();
});

afterEach(() => {
  cleanup();
});
