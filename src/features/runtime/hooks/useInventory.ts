import { useEffect } from 'react';
import { useQuery, type QueryObserverResult } from '@tanstack/react-query';
import { getInventory } from '../../../ipc/commands';
import type { RuntimeInventory } from '../../../ipc/types';
import { inventoryKeys } from '../query-keys';

export const inventoryStructuralSharing = (oldData: RuntimeInventory | undefined, newData: RuntimeInventory): RuntimeInventory => {
  if (oldData?.hasSnapshot && newData.generation < oldData.generation) return oldData;
  if (oldData?.hasSnapshot && newData.generation === oldData.generation) {
    if (oldData.runtimeSessionId !== newData.runtimeSessionId) return oldData;
    if (oldData.freshness !== 'fresh' && newData.freshness === 'fresh') return oldData;
    // A failed refresh retains the observation generation but changes its health.
    // Equal generations never replace the observed containers or their identity.
    if (oldData.freshness === newData.freshness && JSON.stringify(oldData.error) === JSON.stringify(newData.error)) return oldData;
    return { ...oldData, freshness: newData.freshness, error: newData.error };
  }
  return newData;
};
const queryStructuralSharing = (oldData: unknown, newData: unknown): unknown => inventoryStructuralSharing(oldData as RuntimeInventory | undefined, newData as RuntimeInventory);

const isVisible = () => typeof document === 'undefined' || document.visibilityState === 'visible';

export function useInventory({ owner = true }: { owner?: boolean } = {}): QueryObserverResult<RuntimeInventory> {
  const query = useQuery<RuntimeInventory>({
    queryKey: inventoryKeys.snapshot(),
    queryFn: getInventory,
    enabled: owner,
    refetchInterval: () => isVisible() ? 3000 : false,
    structuralSharing: queryStructuralSharing,
    placeholderData: (previous: RuntimeInventory | undefined) => previous,
  });
  useEffect(() => {
    if (!owner) return;
    const onVisibilityChange = () => { if (isVisible()) void query.refetch(); };
    document.addEventListener('visibilitychange', onVisibilityChange);
    return () => document.removeEventListener('visibilitychange', onVisibilityChange);
  }, [owner, query.refetch]);
  return query;
}
