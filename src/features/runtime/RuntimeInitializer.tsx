import { useEffect, useRef, type ReactNode } from 'react';
import { useConnectRuntime, useRuntimeState } from './hooks/useRuntimeSession';
import { AppErrorException } from '../../ipc/errors';
import { Alert } from '../../ui/components/Alert';
import { Button } from '../../ui/components/Button';
import type { RuntimeState } from '../../ipc/types';
import { useInventory } from './hooks/useInventory';
import { useDiagnostics } from '../diagnostics/hooks';
import { useApplicationStateEvents } from './useApplicationStateEvents';

function terminalStateError(state: RuntimeState | undefined): AppErrorException | null {
  if (state?.state === 'failed') return new AppErrorException(state.error);
  if (state?.state === 'contextMismatch') return new AppErrorException({
    code: 'runtime_context_mismatch', operation: 'get_runtime_state', subject: null,
    message: 'Runtime context mismatch', details: `Runtime endpoint: ${state.details.endpoint}`, retryable: false,
  });
  return null;
}

export function RuntimeInitializer({ children }: { children: ReactNode }) {
  useApplicationStateEvents();
  useInventory();
  useDiagnostics();
  const attempted = useRef(false);
  const state = useRuntimeState();
  const connect = useConnectRuntime();

  useEffect(() => {
    if (!attempted.current) {
      attempted.current = true;
      void connect.mutate();
    }
  }, [connect.mutateAsync]);

  const error = state.error instanceof AppErrorException ? state.error : null;
  const connectError = connect.error instanceof AppErrorException ? connect.error : null;
  const failure = error ?? connectError ?? terminalStateError(state.data);
  return <>{children}
    {failure ? <Alert variant="destructive"><p>{failure.code === 'protocol_mismatch' ? `Response format mismatch: ${failure.message}` : failure.message}</p><Button onClick={() => { void connect.mutate(); }}>Retry</Button></Alert> : null}
  </>;
}
