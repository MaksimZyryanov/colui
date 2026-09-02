import { QueryClientProvider } from '@tanstack/react-query';
import { queryClient } from './query-client';
import { AppRoutes } from './routes';
import { RuntimeInitializer } from '../features/runtime/RuntimeInitializer';

export function App() {
  return <QueryClientProvider client={queryClient}><RuntimeInitializer><AppRoutes /></RuntimeInitializer></QueryClientProvider>;
}
