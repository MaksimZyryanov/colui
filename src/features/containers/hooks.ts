import { useMutation, useMutationState, useQueryClient } from '@tanstack/react-query';
import { openContainerPort, runContainerAction } from '../../ipc/commands';
import type { ContainerActionRequest, RuntimeInventory } from '../../ipc/types';
import { inventoryKeys } from '../runtime/query-keys';
import { inventoryStructuralSharing } from '../runtime/hooks/useInventory';
import { invalidateApplicationState } from '../runtime/useApplicationStateEvents';
import { AppErrorException } from '../../ipc/errors';

const containerActionKeys = ['containers', 'action'] as const;
type BatchRequest = Omit<ContainerActionRequest, 'containerId'> & { containerIds: string[] };

export function useContainerActionsPending(containerIds: string[], runtimeSessionId: string) {
  const pending = useMutationState({
    filters: { mutationKey: containerActionKeys, status: 'pending' },
    select: mutation => {
      const request = mutation.state.variables as ContainerActionRequest | BatchRequest | undefined;
      if (!request || request.runtimeSessionId !== runtimeSessionId) return false;
      const targets = 'containerIds' in request ? request.containerIds : [request.containerId];
      return targets.some(id => containerIds.includes(id));
    },
  });
  return pending.some(Boolean);
}

export function useContainerAction() {
  const client = useQueryClient();
  return useMutation({ mutationKey: [...containerActionKeys, 'single'], mutationFn: runContainerAction, onSuccess: result => {
    client.setQueryData(inventoryKeys.snapshot(), (old: RuntimeInventory | undefined) => inventoryStructuralSharing(old, result.inventory));
    invalidateApplicationState(client, ['discovery', 'diagnostics']);
  } });
}

export function useContainerActions() {
  const client = useQueryClient();
  return useMutation({
    mutationKey: [...containerActionKeys, 'batch'],
    mutationFn: async ({ containerIds, runtimeSessionId, action }: BatchRequest) => {
      for (const containerId of containerIds) {
        const result = await runContainerAction({ containerId, runtimeSessionId, action });
        client.setQueryData(inventoryKeys.snapshot(), (old: RuntimeInventory | undefined) => inventoryStructuralSharing(old, result.inventory));
        if (result.observation === 'indeterminate_after_session_change') throw new AppErrorException({ code: 'runtime_unavailable', operation: 'run_container_action', subject: { kind: 'container', id: containerId }, message: 'Runtime session changed', details: 'The action outcome is uncertain. Reconnect before retrying the remaining containers.', retryable: true });
      }
    },
    onSettled: () => invalidateApplicationState(client, ['discovery', 'diagnostics']),
  });
}

export function useOpenContainerPort() {
  return useMutation({ mutationFn: openContainerPort });
}
