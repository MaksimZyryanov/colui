import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { connectRuntime, getRuntimeState } from '../../../ipc/commands';
import { runtimeKeys } from '../query-keys';

export function useRuntimeState() {
  return useQuery({ queryKey: runtimeKeys.state(), queryFn: getRuntimeState });
}

export function useConnectRuntime() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: connectRuntime,
    onSuccess: state => {
      if (client.getQueryState(runtimeKeys.state())?.status !== 'error') client.setQueryData(runtimeKeys.state(), state);
    },
  });
}
