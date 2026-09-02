import { mockBackend } from './mock-backend';
import { normalizeError } from './errors';

export async function dispatch(command: string, args?: unknown): Promise<unknown> {
  try {
    if (import.meta.env.VITE_MOCK_IPC === 'true' || import.meta.env.MODE === 'test') return await mockBackend.invoke(command, args);
    const { invoke } = await import('@tauri-apps/api/core');
    return await invoke(command, args as Record<string, unknown> | undefined);
  } catch (error) {
    throw normalizeError(error, command);
  }
}
