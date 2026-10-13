import { useMutation, useMutationState, useQueryClient } from '@tanstack/react-query';
import { applyProject, restartProject, stopProject, tearDownProject } from '../../../ipc/commands';
import type { LifecycleResult, ProjectStatus } from '../../../ipc/types';
import { inventoryKeys } from '../../../features/runtime/query-keys';
import { inventoryStructuralSharing } from '../../../features/runtime/hooks/useInventory';
const lifecycleKeys = ['projects', 'lifecycle'] as const;
export const shouldPublishLifecycleInventory = (result: LifecycleResult) => Boolean(result.inventory);

export function useProfileLifecycleOperation(profileId: string): ProjectStatus['operation'] {
  const operations = useMutationState({
    filters: { mutationKey: lifecycleKeys, status: 'pending' },
    select: mutation => mutation.state.variables === profileId ? {
      kind: mutation.options.mutationKey?.[2] as NonNullable<ProjectStatus['operation']>['kind'],
      phase: 'running' as const,
      startedAt: new Date(mutation.state.submittedAt).toISOString(),
    } : null,
  });
  return operations.find(operation => operation !== null) ?? null;
}

export function useLifecycleActions() {
  const client = useQueryClient();
  const onSuccess = (result: LifecycleResult) => { if (shouldPublishLifecycleInventory(result)) client.setQueryData(inventoryKeys.snapshot(), (old: import('../../../ipc/types').RuntimeInventory | undefined) => inventoryStructuralSharing(old, result.inventory)); };
  const apply = useMutation({ mutationKey: [...lifecycleKeys, 'apply'], mutationFn: applyProject, onSuccess });
  const stop = useMutation({ mutationKey: [...lifecycleKeys, 'stop'], mutationFn: stopProject, onSuccess });
  const tearDown = useMutation({ mutationKey: [...lifecycleKeys, 'tear-down'], mutationFn: tearDownProject, onSuccess });
  const restart = useMutation({ mutationKey: [...lifecycleKeys, 'restart'], mutationFn: restartProject, onSuccess });
  return { apply, stop, tearDown, restart };
}
