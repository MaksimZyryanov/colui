import { applicationStateChangedSchema } from './schemas';
import { decodeResponse } from './validation';
import type { ApplicationStateChanged } from './types';

export async function subscribeApplicationState(listener: (event: ApplicationStateChanged) => void): Promise<() => void> {
  const receive = (raw: unknown) => {
    let event: ApplicationStateChanged;
    try { event = decodeResponse(applicationStateChangedSchema, raw, 'application_state_changed'); }
    catch { return; } // Malformed hints cannot authorize cache changes; polling repairs missed hints.
    listener(event);
  };
  if (import.meta.env.VITE_MOCK_IPC === 'true' || import.meta.env.MODE === 'test') return () => {};
  const { listen } = await import('@tauri-apps/api/event');
  return listen<unknown>('application_state_changed', event => receive(event.payload));
}
