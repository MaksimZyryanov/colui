import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { autoRegisterCandidates, configureAutoRegistration, getAutoRegistrationConfiguration, ignoreCandidate, listDiscoveryCandidates, registerCandidate } from '../../ipc/commands';
import type { RegistryHealth } from '../../ipc/types';
import { discoveryKeys } from './query-keys';
import { invalidateApplicationState } from '../runtime/useApplicationStateEvents';

export function useDiscovery(runtimeSessionId: string | null, inventoryGeneration: number, health: RegistryHealth | null) {
  return useQuery({ queryKey: discoveryKeys.list(runtimeSessionId, inventoryGeneration, health), queryFn: listDiscoveryCandidates, enabled: runtimeSessionId !== null, placeholderData: keepPreviousData });
}

export function useAutoRegistrationConfiguration() {
  return useQuery({ queryKey: discoveryKeys.configuration(), queryFn: getAutoRegistrationConfiguration });
}

export function useDiscoveryMutations() {
  const client = useQueryClient();
  const registered = () => invalidateApplicationState(client, ['profiles', 'discovery', 'diagnostics']);
  const register = useMutation({ mutationFn: registerCandidate, onSuccess: registered });
  const autoRegister = useMutation({ mutationFn: autoRegisterCandidates, onSuccess: registered });
  const ignore = useMutation({ mutationFn: ignoreCandidate, onSuccess: () => invalidateApplicationState(client, ['discovery']) });
  const configure = useMutation({ mutationFn: configureAutoRegistration, onSuccess: () => invalidateApplicationState(client, ['discovery', 'diagnostics']) });
  return { register, autoRegister, ignore, configure };
}
