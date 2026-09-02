import { useEffect, useRef, type ReactNode } from 'react';
import { useConnectRuntime, useRuntimeState } from './hooks/useRuntimeSession';
import { AppErrorException } from '../../ipc/errors';

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
  return <>{children}
    {error ? <p role="alert">{error.code === 'protocol_mismatch' ? `Response format mismatch: ${error.message}` : error.message}</p> : null}
    {connectError ? <p role="alert">{connectError.code === 'protocol_mismatch' ? `Response format mismatch: ${connectError.message}` : connectError.message}</p> : null}
  </>;
}
