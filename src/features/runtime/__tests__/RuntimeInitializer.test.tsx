// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, beforeEach } from 'vitest';
import { App } from '../../../app/App';
import { mockBackend } from '../../../ipc/mock-backend';
import { queryClient } from '../../../app/query-client';

describe('RuntimeInitializer', () => {
  beforeEach(() => { mockBackend.reset(); queryClient.clear(); });

  it('connects once on first render without blocking profiles', async () => {
    render(<App />);
    await waitFor(() => expect(mockBackend.getInvocations().filter(({ command }) => command === 'connect_runtime')).toHaveLength(1));
    expect(screen.getByRole('main')).toBeVisible();
  });

  it('does not retry non-retryable protocol mismatch', async () => {
    mockBackend.setResponseOverride('get_runtime_state', { state: 'bad' });
    render(<App />);
    await waitFor(() => expect(mockBackend.getInvocations().filter(({ command }) => command === 'get_runtime_state')).toHaveLength(1));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent(/invalid response from get_runtime_state/i));
    expect(mockBackend.getInvocations().filter(({ command }) => command === 'get_runtime_state')).toHaveLength(1);
  });
});
