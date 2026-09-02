import { QueryClientProvider } from '@tanstack/react-query';
import { queryClient } from './query-client';
import { AppRoutes } from './routes';
import { RuntimeInitializer } from '../features/runtime/RuntimeInitializer';
import { ErrorBoundary } from '../ui/components/ErrorBoundary';

export function App() {
  return <QueryClientProvider client={queryClient}><ErrorBoundary><RuntimeInitializer><AppRoutes /></RuntimeInitializer></ErrorBoundary></QueryClientProvider>;
}
