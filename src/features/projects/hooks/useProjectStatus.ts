import { useQuery } from '@tanstack/react-query';
import { useInventory } from '../../../features/runtime/hooks/useInventory';
import { getProfile, getProjectDetails } from '../../../ipc/commands';
import { projectKeys } from '../query-keys';
import type { ProjectStatus } from '../../../ipc/types';
import type { ProfileDetails, ProfileSummary, RuntimeInventory } from '../../../ipc/types';
import type { QueryObserverResult } from '@tanstack/react-query';
import { useRuntimeState } from '../../runtime/hooks/useRuntimeSession';
import { useProfileLifecycleOperation } from './useLifecycleActions';

function absolutePath(path: string): string | null {
  if (!path.startsWith('/')) return null;
  const parts: string[] = [];
  for (const part of path.split('/')) {
    if (part === '..') parts.pop();
    else if (part && part !== '.') parts.push(part);
  }
  return `/${parts.join('/')}`;
}

function projectAssociation(profile: ProfileSummary, inventory: RuntimeInventory, composeFiles?: string[]) {
  const directory = absolutePath(profile.workingDirectory);
  if (!directory || !composeFiles) return { group: undefined, ambiguous: false };
  const files = [...new Set(composeFiles.map(file => absolutePath(file.startsWith('/') ? file : `${directory}/${file}`)))];
  if (files.some(file => !file)) return { group: undefined, ambiguous: false };
  const groups = inventory.composeObservationGroups.filter(group => group.composeProjectName === profile.composeProjectName);
  // Docker's aggregate project list merges same-name observations. Use the full
  // association tuple and retain the backend's name-ambiguity guard instead.
  const matching = groups.find(group => group.workingDirectory === directory && JSON.stringify(group.configFiles) === JSON.stringify(files));
  return { group: groups.length === 1 ? matching : undefined, ambiguous: Boolean(matching && groups.length > 1) };
}

export function projectContainersFromInventory(profile: ProfileSummary, inventory: RuntimeInventory, composeFiles?: string[]) {
  const { group } = projectAssociation(profile, inventory, composeFiles);
  if (!group) return [];
  return inventory.containers.filter(container => group.containerIds.includes(container.id))
    .sort((left, right) => left.name.localeCompare(right.name, undefined, { sensitivity: 'base', numeric: true }) || left.id.localeCompare(right.id));
}

export function projectStatusFromInventory(profile: ProfileSummary, inventory: RuntimeInventory, composeFiles?: string[]): ProjectStatus {
  const containers = projectContainersFromInventory(profile, inventory, composeFiles);
  const { ambiguous } = projectAssociation(profile, inventory, composeFiles);
  const running = containers.filter(container => container.state === 'running').length;
  const presence = !inventory.hasSnapshot || !composeFiles || ambiguous ? 'unavailable' : containers.length ? 'present' : 'absent';
  return { profileId: profile.id, runtime: { presence, activity: presence === 'present' ? running === containers.length && running > 0 ? 'all-running' : running > 0 ? 'mixed' : 'none-running' : null, containerCount: containers.length, runningContainerCount: running, observedAt: presence === 'present' ? inventory.observedAt : null }, definition: { state: 'unchecked', revision: null, serviceCount: null }, operation: null, issues: [...(inventory.error ? [{ message: inventory.error.message }] : []), ...(ambiguous ? [{ field: 'ambiguous_runtime_association', message: 'runtime association is ambiguous' }] : [])] };
}

export function useProjectStatus(profile: ProfileSummary, sharedInventory?: QueryObserverResult<RuntimeInventory>, enabled = true, definitionEnabled = true) {
  const ownedInventory = useInventory({ owner: enabled && !sharedInventory });
  const inventory = sharedInventory ?? ownedInventory;
  const runtime = useRuntimeState();
  const operation = useProfileLifecycleOperation(profile.id);
  const details = useQuery({ queryKey: projectKeys.definition(profile.id), queryFn: () => getProjectDetails(profile.id), enabled: definitionEnabled });
  const cachedProfile = useQuery<ProfileDetails>({ queryKey: projectKeys.detail(profile.id), queryFn: () => getProfile(profile.id), enabled: false });
  const currentDefinition = details.data?.profile.profile.revision === profile.revision ? details.data : undefined;
  const profileDetails = currentDefinition?.profile ?? (cachedProfile.data?.profile.revision === profile.revision ? cachedProfile.data : undefined);
  const profileAssociationUnique = currentDefinition?.profileAssociationUnique !== false;
  const containers = inventory.data && profileAssociationUnique ? projectContainersFromInventory(profile, inventory.data, profileDetails?.composeFiles) : [];
  const observationAmbiguous = inventory.data && projectAssociation(profile, inventory.data, profileDetails?.composeFiles).ambiguous;
  const observationCurrent = profileAssociationUnique && !observationAmbiguous && runtime.data?.state === 'ready' && inventory.data?.hasSnapshot && inventory.data.freshness === 'fresh' && inventory.data.runtimeSessionId === runtime.data.context.sessionId && !inventory.isError && !runtime.isError;
  const data = inventory.data ? projectStatusFromInventory(profile, inventory.data, profileDetails?.composeFiles) : undefined;
  if (data && currentDefinition) {
    data.definition = { state: currentDefinition.definition.state, revision: currentDefinition.definition.definitionRevision ?? null, serviceCount: currentDefinition.definition.services.length };
    data.issues = [...data.issues, ...currentDefinition.definition.issues];
    if (!profileAssociationUnique) data.issues.push(...currentDefinition.runtimeIssues);
    if (currentDefinition.definition.error) data.issues.push({ message: currentDefinition.definition.error.message });
  }
  if (data) {
    data.operation = operation;
    if (!observationCurrent) data.runtime = { ...data.runtime, presence: 'unavailable', activity: null, observedAt: null, ...(!profileAssociationUnique ? { containerCount: 0, runningContainerCount: 0 } : {}) };
  }
  return { ...inventory, data, containers, observationCurrent: Boolean(observationCurrent), isLoading: inventory.isLoading || details.isLoading, isError: inventory.isError || details.isError, inventoryError: inventory.error, definitionError: currentDefinition?.definition.error ?? details.error };
}
