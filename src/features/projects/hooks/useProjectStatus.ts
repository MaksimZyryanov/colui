import { useInventory } from '../../../features/runtime/hooks/useInventory';
import { useQuery } from '@tanstack/react-query';
import { getProjectDetails } from '../../../ipc/commands';
import { projectKeys } from '../query-keys';
import type { ProjectStatus } from '../../../ipc/types';
import type { ProfileSummary } from '../../../ipc/types';
export function projectStatusFromInventory(profile: ProfileSummary, inventory: NonNullable<ReturnType<typeof useInventory>['data']>): ProjectStatus {
  const project = inventory.projects.find(value => value.composeProjectName === profile.composeProjectName);
  const containers = project?.containers ?? [];
  const running = containers.filter(container => container.state === 'running').length;
  return { profileId: profile.id, runtime: { presence: inventory.hasSnapshot ? 'present' : 'unavailable', activity: inventory.hasSnapshot ? running === containers.length && running > 0 ? 'all-running' : running > 0 ? 'mixed' : 'none-running' : null, containerCount: containers.length, runningContainerCount: running, observedAt: inventory.hasSnapshot ? inventory.observedAt : null }, definition: { state: 'unchecked', revision: null, serviceCount: null }, operation: null, issues: inventory.error ? [{ message: inventory.error.message }] : [] };
}

export function useProjectStatus(profile: ProfileSummary) {
  const inventory = useInventory();
  const details = useQuery({ queryKey: projectKeys.definition(profile.id), queryFn: () => getProjectDetails(profile.id) });
  const data = inventory.data ? projectStatusFromInventory(profile, inventory.data) : undefined;
  if (data && details.data) {
    data.definition = { state: details.data.definition.state, revision: details.data.definition.definitionRevision ?? null, serviceCount: details.data.definition.services.length };
    data.issues = [...data.issues, ...details.data.definition.issues];
  }
  return { ...inventory, data, isLoading: inventory.isLoading, isError: inventory.isError };
}
