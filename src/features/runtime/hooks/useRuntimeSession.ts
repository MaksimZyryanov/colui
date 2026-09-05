import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';
import { connectRuntime, disconnectRuntime, getRuntimeState, reconnectRuntime } from '../../../ipc/commands';
import type { RuntimeInventory, RuntimeState } from '../../../ipc/types';
import { inventoryKeys, runtimeKeys, runtimeSessionId } from '../query-keys';
import { discoveryKeys } from '../../discovery/query-keys';
import { logKeys } from '../../diagnostics/query-keys';
import { inventoryStructuralSharing } from './useInventory';
import { invalidateApplicationState } from '../useApplicationStateEvents';

function clearChangedSession(client: QueryClient, state: RuntimeState) {
  const previous = client.getQueryData<RuntimeState>(runtimeKeys.state());
  if (runtimeSessionId(previous) !== runtimeSessionId(state)) {
    client.removeQueries({ queryKey: discoveryKeys.all() });
    client.removeQueries({ queryKey: logKeys.all() });
  }
}

function publishRuntimeState(client: QueryClient, state: RuntimeState) {
  clearChangedSession(client, state);
  client.setQueryData(runtimeKeys.state(), state);
  invalidateApplicationState(client, ['discovery', 'diagnostics']);
}

export function useRuntimeState() {
  const client = useQueryClient();
  return useQuery({ queryKey: runtimeKeys.state(), queryFn: async () => { const state = await getRuntimeState(); clearChangedSession(client, state); return state; } });
}

export function useConnectRuntime() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: connectRuntime,
    onSuccess: state => publishRuntimeState(client, state),
  });
}

export function useDisconnectRuntime() {
  const client = useQueryClient();
  return useMutation({ mutationFn: disconnectRuntime, onSuccess: state => publishRuntimeState(client, state) });
}

export function useReconnectRuntime() {
  const client = useQueryClient();
  return useMutation({ mutationFn: reconnectRuntime, onSuccess: result => {
    client.removeQueries({ queryKey: discoveryKeys.all() });
    client.removeQueries({ queryKey: logKeys.all() });
    publishRuntimeState(client, result.runtimeState);
    if (result.inventory) client.setQueryData(inventoryKeys.snapshot(), (old: RuntimeInventory | undefined) => inventoryStructuralSharing(old, result.inventory!));
  } });
}
