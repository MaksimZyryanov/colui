import { QueryClient } from '@tanstack/react-query';
import { AppErrorException } from '../ipc/errors';

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: (failureCount, error) => error instanceof AppErrorException && error.retryable && failureCount < 2,
      refetchOnWindowFocus: false,
      refetchInterval: false,
    },
    mutations: { retry: false },
  },
});
