import { useEffect } from 'react';
import { keepPreviousData, notifyManager, useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';
import { autoRegisterCandidates, configureAutoRegistration, getAutoRegistrationConfiguration, ignoreCandidate, listDiscoveryCandidates, registerCandidate } from '../../ipc/commands';
import type { DiscoveryList, ProfileDetails, ProfileSummary, RegistryHealth, RegisterCandidateRequest } from '../../ipc/types';
import { discoveryKeys } from './query-keys';
import { invalidateApplicationState } from '../runtime/useApplicationStateEvents';
import { projectKeys } from '../projects/query-keys';
import { normalizeWorkingDirectory } from './identity';

type RegistrationHandoff = { candidateId: string; profileId: string };
export function candidateResourceId(runtimeSessionId: string, candidateId: string) { return `candidate:${runtimeSessionId}:${candidateId}`; }

function seedDiscoveredProfile(client: QueryClient, profile: ProfileSummary, configFiles: string[]) {
  if (profile.registrationOrigin !== 'discovered' || profile.revision !== 1) return;
  // Discovery creates revision 1 with precisely these Compose files and no env files.
  // This metadata can associate containers before Compose definition loading finishes.
  client.setQueryData<ProfileDetails>(projectKeys.detail(profile.id), previous => previous && previous.profile.revision >= profile.revision ? previous : { profile, composeFiles: configFiles, environmentFiles: [] });
}

export function useRegistrationMetadata(profiles: ProfileSummary[] | undefined, runtimeSessionId: string | null, discovery: DiscoveryList | undefined) {
  const client = useQueryClient();
  useEffect(() => {
    if (!runtimeSessionId) return;
    for (const candidate of discovery?.candidates ?? []) {
      if (candidate.runtimeSessionId !== runtimeSessionId || candidate.classification !== 'already_registered') continue;
      const matches = profiles?.filter(profile => profile.composeProjectName === candidate.composeProjectName) ?? [];
      if (matches.length === 1 && normalizeWorkingDirectory(matches[0].workingDirectory) === candidate.workingDirectory) seedDiscoveredProfile(client, matches[0], candidate.configFiles);
    }
  }, [client, profiles, runtimeSessionId, discovery]);
}

export function useRegistrationHandoffs(runtimeSessionId: string | null) {
  return useQuery<RegistrationHandoff[]>({ queryKey: discoveryKeys.handoffs(runtimeSessionId), queryFn: () => [], enabled: false, initialData: [] });
}

export function useDiscovery(runtimeSessionId: string | null, inventoryGeneration: number, health: RegistryHealth | null) {
  return useQuery({ queryKey: discoveryKeys.list(runtimeSessionId, inventoryGeneration, health), queryFn: listDiscoveryCandidates, enabled: runtimeSessionId !== null, placeholderData: keepPreviousData });
}

export function useAutoRegistrationConfiguration() {
  return useQuery({ queryKey: discoveryKeys.configuration(), queryFn: getAutoRegistrationConfiguration });
}

export function useDiscoveryMutations() {
  const client = useQueryClient();
  const registered = () => invalidateApplicationState(client, ['profiles', 'discovery', 'diagnostics']);
  const publishProfiles = (profiles: ProfileSummary[]) => client.setQueryData<ProfileSummary[]>(projectKeys.list(), previous => {
    const updated = new Map((previous ?? []).map(profile => [profile.id, profile]));
    for (const profile of profiles) if ((updated.get(profile.id)?.revision ?? 0) <= profile.revision) updated.set(profile.id, profile);
    return [...updated.values()];
  });
  const publishHandoff = (profile: ProfileSummary, request: Pick<RegisterCandidateRequest, 'runtimeSessionId' | 'candidateId' | 'configFiles'>) => {
    seedDiscoveredProfile(client, profile, request.configFiles);
    client.setQueryData<RegistrationHandoff[]>(discoveryKeys.handoffs(request.runtimeSessionId), previous => [...(previous ?? []).filter(handoff => handoff.candidateId !== request.candidateId), { candidateId: request.candidateId, profileId: profile.id }]);
    client.setQueriesData<DiscoveryList>({ queryKey: ['discovery', 'list', request.runtimeSessionId] }, previous => previous ? { ...previous, candidates: previous.candidates.filter(candidate => candidate.candidateId !== request.candidateId) } : previous);
  };
  const register = useMutation({ mutationFn: registerCandidate, onSuccess: async (profile, request) => {
    // A list read begun before registration must not erase the returned profile.
    await client.cancelQueries({ queryKey: projectKeys.list() });
    notifyManager.batch(() => { publishProfiles([profile]); publishHandoff(profile, request); });
    registered();
  } });
  const autoRegister = useMutation({ mutationFn: autoRegisterCandidates, onMutate: request => {
    const candidates = client.getQueriesData<DiscoveryList>({ queryKey: ['discovery', 'list', request.runtimeSessionId] }).flatMap(([, list]) => list?.candidates ?? []).filter(candidate => candidate.inventoryGeneration === request.inventoryGeneration && candidate.classification === 'new_unambiguous' && !candidate.ignored);
    return [...new Map(candidates.map(candidate => [candidate.candidateId, candidate])).values()];
  }, onSuccess: async (result, _request, candidates) => {
    await client.cancelQueries({ queryKey: projectKeys.list() });
    notifyManager.batch(() => {
      publishProfiles(result.profiles);
      for (const profile of result.profiles) {
        const matches = (candidates ?? []).filter(candidate => candidate.composeProjectName === profile.composeProjectName && candidate.workingDirectory === normalizeWorkingDirectory(profile.workingDirectory));
        // Only a uniquely eligible observation can be handed to an auto-created profile.
        if (matches.length === 1) publishHandoff(profile, matches[0]);
      }
    });
    registered();
  } });
  const ignore = useMutation({ mutationFn: ignoreCandidate, onSuccess: () => invalidateApplicationState(client, ['discovery']) });
  const configure = useMutation({ mutationFn: configureAutoRegistration, onSuccess: () => invalidateApplicationState(client, ['discovery', 'diagnostics']) });
  return { register, autoRegister, ignore, configure };
}
