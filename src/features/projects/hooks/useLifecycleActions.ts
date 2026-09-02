import { useMutation, useQueryClient } from '@tanstack/react-query';
import { applyProject, restartProject, stopProject, tearDownProject } from '../../../ipc/commands';
import { projectKeys } from '../query-keys';
import type { LifecycleResult } from '../../../ipc/types';
export const shouldInvalidateLifecycle = (result: LifecycleResult) => result.success;
export function useLifecycleActions() {
  const client = useQueryClient();
  const onSuccess = (result: LifecycleResult, profileId: string) => { if (shouldInvalidateLifecycle(result)) void client.invalidateQueries({ queryKey: projectKeys.status(profileId), exact: true }); };
  const apply = useMutation({ mutationFn: applyProject, onSuccess });
  const stop = useMutation({ mutationFn: stopProject, onSuccess });
  const tearDown = useMutation({ mutationFn: tearDownProject, onSuccess });
  const restart = useMutation({ mutationFn: restartProject, onSuccess });
  return { apply, stop, tearDown, restart };
}
