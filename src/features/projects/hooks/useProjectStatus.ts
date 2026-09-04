import { useQuery } from '@tanstack/react-query';
import { useInventory } from '../../../features/runtime/hooks/useInventory';
import { getProjectDetails } from '../../../ipc/commands';
import { projectKeys } from '../query-keys';
import type { ProjectStatus } from '../../../ipc/types';
import type { ProfileSummary, RuntimeInventory } from '../../../ipc/types';
import type { QueryObserverResult } from '@tanstack/react-query';
export function projectStatusFromInventory(profile: ProfileSummary, inventory: RuntimeInventory): ProjectStatus {
  const project = inventory.projects.find(value => value.composeProjectName === profile.composeProjectName);
  const containers = project?.containers ?? [];
  const running = containers.filter(container => container.state === 'running').length;
  const presence = !inventory.hasSnapshot ? 'unavailable' : project ? 'present' : 'absent';
  return { profileId: profile.id, runtime: { presence, activity: presence === 'present' ? running === containers.length && running > 0 ? 'all-running' : running > 0 ? 'mixed' : 'none-running' : null, containerCount: containers.length, runningContainerCount: running, observedAt: presence === 'present' ? inventory.observedAt : null }, definition: { state: 'unchecked', revision: null, serviceCount: null }, operation: null, issues: inventory.error ? [{ message: inventory.error.message }] : [] };
}

export function useProjectStatus(profile: ProfileSummary, sharedInventory?: QueryObserverResult<RuntimeInventory>, enabled = true) {
  const ownedInventory = useInventory({ owner: enabled && !sharedInventory });
  const inventory = sharedInventory ?? ownedInventory;
  const details = useQuery({ queryKey: projectKeys.definition(profile.id), queryFn: () => getProjectDetails(profile.id) });
  const data = inventory.data ? projectStatusFromInventory(profile, inventory.data) : undefined;
  if (data && details.data) {
    data.definition = { state: details.data.definition.state, revision: details.data.definition.definitionRevision ?? null, serviceCount: details.data.definition.services.length };
    data.issues = [...data.issues, ...details.data.definition.issues];
  }
  return { ...inventory, data, isLoading: inventory.isLoading || details.isLoading, isError: inventory.isError || details.isError, inventoryError: inventory.error, definitionError: details.error };
}
