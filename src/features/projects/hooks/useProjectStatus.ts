import { useQuery } from '@tanstack/react-query';
import { getProjectStatus } from '../../../ipc/commands';
import { projectKeys } from '../query-keys';
export function useProjectStatus(profileId: string) { return useQuery({ queryKey: projectKeys.status(profileId), queryFn: () => getProjectStatus(profileId) }); }
