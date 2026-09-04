import { useMutation, useQueryClient } from '@tanstack/react-query';
import { applyProject, restartProject, stopProject, tearDownProject } from '../../../ipc/commands';
import { projectKeys } from '../query-keys';
import type { LifecycleResult } from '../../../ipc/types';
import { inventoryKeys } from '../../../features/runtime/query-keys';
import { inventoryStructuralSharing } from '../../../features/runtime/hooks/useInventory';
export const shouldPublishLifecycleInventory = (result: LifecycleResult) => result.success;
export function useLifecycleActions() {
  const client = useQueryClient();
  const onSuccess = (result: LifecycleResult) => { if (shouldPublishLifecycleInventory(result)) client.setQueryData(inventoryKeys.snapshot(), (old: import('../../../ipc/types').RuntimeInventory | undefined) => inventoryStructuralSharing(old, result.inventory)); };
  const apply = useMutation({ mutationFn: applyProject, onSuccess });
  const stop = useMutation({ mutationFn: stopProject, onSuccess });
  const tearDown = useMutation({ mutationFn: tearDownProject, onSuccess });
  const restart = useMutation({ mutationFn: restartProject, onSuccess });
  return { apply, stop, tearDown, restart };
}
