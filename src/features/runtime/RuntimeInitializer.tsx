import { useEffect, useRef, type ReactNode } from 'react';
import { useConnectRuntime, useRuntimeState } from './hooks/useRuntimeSession';
import { AppErrorException } from '../../ipc/errors';
import { Alert } from '../../ui/components/Alert';
import { Button } from '../../ui/components/Button';

export function RuntimeInitializer({ children }: { children: ReactNode }) {
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
  const failure = error ?? connectError;
  return <>{children}
    {failure ? <Alert variant="destructive"><p>{failure.code === 'protocol_mismatch' ? `Response format mismatch: ${failure.message}` : failure.message}</p><Button onClick={() => { void connect.mutate(); }}>Retry</Button></Alert> : null}
  </>;
}
