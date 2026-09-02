import { useQuery } from '@tanstack/react-query';
import { listProfiles } from '../../../ipc/commands';
import { projectKeys } from '../query-keys';
export function useProfiles() { return useQuery({ queryKey: projectKeys.list(), queryFn: listProfiles }); }
