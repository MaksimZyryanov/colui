// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, beforeEach } from 'vitest';
import { App } from '../../../app/App';
import { mockBackend } from '../../../ipc/mock-backend';
import { queryClient } from '../../../app/query-client';
import { AppErrorException } from '../../../ipc/errors';
import { runtimeKeys } from '../query-keys';

describe('RuntimeInitializer', () => {
  beforeEach(() => { mockBackend.reset(); queryClient.clear(); });
  afterEach(cleanup);

  it('connects once on first render without blocking profiles', async () => {
    render(<App />);
    await waitFor(() => expect(mockBackend.getInvocations().filter(({ command }) => command === 'connect_runtime')).toHaveLength(1));
    expect(screen.getByRole('main')).toBeVisible();
  });

  it('does not retry non-retryable protocol mismatch', async () => {
    mockBackend.setResponseOverride('get_runtime_state', { state: 'bad' });
    mockBackend.setErrorOverride('connect_runtime', new AppErrorException({
      code: 'runtime_connection_failed', operation: 'connect_runtime', subjectId: null,
      message: 'connection failed', details: null, retryable: false,
    }));
    render(<App />);
    await waitFor(() => expect(mockBackend.getInvocations().filter(({ command }) => command === 'get_runtime_state')).toHaveLength(1));
    await waitFor(() => expect(screen.getByText(/invalid response from get_runtime_state/i)).toBeVisible());
    expect(mockBackend.getInvocations().filter(({ command }) => command === 'get_runtime_state')).toHaveLength(1);
  });

  it('shows failed auto-connect without blocking the main route', async () => {
    mockBackend.setErrorOverride('connect_runtime', new AppErrorException({
      code: 'runtime_connection_failed',
      operation: 'connect_runtime',
      subjectId: null,
      message: 'Docker is unavailable',
      details: null,
      retryable: false,
    }));

    render(<App />);

    expect(screen.getByRole('main')).toBeVisible();
    expect(await screen.findByRole('alert')).toHaveTextContent('Docker is unavailable');
    expect(screen.getByRole('main')).toBeVisible();
  });

  it('caches successful connect after the runtime state query fails', async () => {
    mockBackend.setResponseOverride('get_runtime_state', { state: 'bad' });

    render(<App />);

    await waitFor(() => expect(queryClient.getQueryState(runtimeKeys.state())?.status).toBe('success'));
    expect(queryClient.getQueryData(runtimeKeys.state())).toMatchObject({ state: 'ready' });
  });
});
