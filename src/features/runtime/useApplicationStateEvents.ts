import { useEffect } from 'react';
import { useQueryClient, type QueryClient } from '@tanstack/react-query';
import { subscribeApplicationState } from '../../ipc/events';
import type { ApplicationStateChanged } from '../../ipc/types';
import { projectKeys } from '../projects/query-keys';
import { discoveryKeys } from '../discovery/query-keys';
import { diagnosticsKeys } from '../diagnostics/query-keys';
import { runtimeKeys } from './query-keys';

export function invalidateApplicationState(client: QueryClient, scopes: ApplicationStateChanged['scopes']) {
  for (const scope of new Set(scopes)) {
    if (scope === 'profiles') void client.invalidateQueries({ queryKey: projectKeys.list() });
    if (scope === 'discovery') void client.invalidateQueries({ queryKey: discoveryKeys.all() });
    if (scope === 'diagnostics') void client.invalidateQueries({ queryKey: diagnosticsKeys.all() });
    if (scope === 'runtime') void client.invalidateQueries({ queryKey: runtimeKeys.state() });
    if (scope === 'definitions') void client.invalidateQueries({ queryKey: projectKeys.all(), predicate: query => query.queryKey[1] === 'definition' || query.queryKey[1] === 'detail' });
  }
}

export function useApplicationStateEvents() {
  const client = useQueryClient();
  useEffect(() => {
    let disposed = false;
    let sequence = -1;
    let unlisten: (() => void) | undefined;
    void subscribeApplicationState(event => {
      if (disposed || event.sequence <= sequence) return;
      sequence = event.sequence;
      invalidateApplicationState(client, event.scopes);
    }).then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(() => { /* Visible polling repairs unavailable event transport. */ });
    return () => { disposed = true; unlisten?.(); };
  }, [client]);
}
