import { useEffect, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { createRegistryBackup, getContainerLogs, getDiagnostics, restoreRegistryBackup } from '../../ipc/commands';
import type { ContainerLogsRequest, DiagnosticsSnapshot } from '../../ipc/types';
import { diagnosticsKeys, logKeys } from './query-keys';
import { discoveryKeys } from '../discovery/query-keys';
import { invalidateApplicationState } from '../runtime/useApplicationStateEvents';

const isVisible = () => typeof document === 'undefined' || document.visibilityState === 'visible';

// Mounted once by the shell; views opt out of ownership and observe the same key.
export function useDiagnostics({ owner = true }: { owner?: boolean } = {}) {
  const client = useQueryClient();
  const [visible, setVisible] = useState(isVisible);
  useEffect(() => {
    if (!owner) return;
    const changed = () => setVisible(isVisible());
    document.addEventListener('visibilitychange', changed);
    return () => document.removeEventListener('visibilitychange', changed);
  }, [owner]);
  return useQuery({
    queryKey: diagnosticsKeys.snapshot(),
    queryFn: async () => {
      const next = await getDiagnostics();
      const previous = client.getQueryData<DiagnosticsSnapshot>(diagnosticsKeys.snapshot());
      if (previous) {
        const registryChanged = JSON.stringify(discoveryKeys.list(null, 0, previous.registry.health)) !== JSON.stringify(discoveryKeys.list(null, 0, next.registry.health));
        const journalChanged = (previous.journal.entries.at(-1)?.sequence ?? 0) !== (next.journal.entries.at(-1)?.sequence ?? 0);
        if (registryChanged || journalChanged) invalidateApplicationState(client, ['profiles', 'discovery', 'definitions']);
        if (JSON.stringify(previous.runtime.state) !== JSON.stringify(next.runtime.state)) invalidateApplicationState(client, ['runtime', 'discovery']);
      }
      return next;
    },
    enabled: owner && visible,
    staleTime: 0,
    refetchInterval: owner && visible ? 2000 : false,
    refetchOnWindowFocus: false,
  });
}

export function useRegistryRecovery() {
  const client = useQueryClient();
  const backup = useMutation({ mutationFn: createRegistryBackup, onSuccess: () => invalidateApplicationState(client, ['diagnostics']) });
  const restore = useMutation({ mutationFn: restoreRegistryBackup, onSuccess: () => invalidateApplicationState(client, ['profiles', 'discovery', 'definitions', 'diagnostics']) });
  return { backup, restore };
}

export function useContainerLogs(request: ContainerLogsRequest) {
  return useQuery({ queryKey: logKeys.snapshot(request), queryFn: () => getContainerLogs(request), enabled: false, retry: false, refetchInterval: false, refetchOnWindowFocus: false, refetchOnReconnect: false, refetchOnMount: false });
}
