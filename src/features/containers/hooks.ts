import { useMutation, useQueryClient } from '@tanstack/react-query';
import { openContainerPort, runContainerAction } from '../../ipc/commands';
import type { RuntimeInventory } from '../../ipc/types';
import { inventoryKeys } from '../runtime/query-keys';
import { inventoryStructuralSharing } from '../runtime/hooks/useInventory';
import { invalidateApplicationState } from '../runtime/useApplicationStateEvents';

export function useContainerAction() {
  const client = useQueryClient();
  return useMutation({ mutationFn: runContainerAction, onSuccess: result => {
    client.setQueryData(inventoryKeys.snapshot(), (old: RuntimeInventory | undefined) => inventoryStructuralSharing(old, result.inventory));
    invalidateApplicationState(client, ['discovery', 'diagnostics']);
  } });
}

export function useOpenContainerPort() {
  return useMutation({ mutationFn: openContainerPort });
}
