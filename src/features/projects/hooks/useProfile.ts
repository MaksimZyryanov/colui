import { useQuery } from '@tanstack/react-query';
import { getProfile } from '../../../ipc/commands';
import { projectKeys } from '../query-keys';
export function useProfile(profileId: string | undefined) { return useQuery({ queryKey: projectKeys.detail(profileId ?? ''), queryFn: () => getProfile(profileId!), enabled: Boolean(profileId) }); }
