// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ProfileCard } from '../ProfileCard';
import { mockBackend } from '../../../../ipc/mock-backend';

const profileId = '00000000-0000-0000-0000-000000000001';
const profile = { id: profileId, revision: 2, displayName: 'Test Project', composeProjectName: 'test', workingDirectory: '/tmp', registrationOrigin: 'manual' as const };
const status = { profileId, runtime: { presence: 'present' as const, activity: 'all-running' as const, containerCount: 1, runningContainerCount: 1, observedAt: '2026-09-02T00:00:00.000Z' }, definition: { state: 'valid' as const, revision: '2', serviceCount: 1 }, operation: null, issues: [] };
const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
const renderRunningCard = () => render(<QueryClientProvider client={client}><ProfileCard profile={profile} initialStatus={status} runtimeReady /></QueryClientProvider>);

describe('ActionMenu', () => {
  beforeEach(() => { mockBackend.reset(); client.clear(); });
  afterEach(cleanup);

  it('sends only profileId for lifecycle actions', async () => {
    const user = userEvent.setup();
    renderRunningCard();
    await user.click(screen.getByRole('button', { name: /stop/i }));
    await waitFor(() => expect(mockBackend.getInvocations().find(invocation => invocation.command === 'stop_project')?.args).toEqual({ profileId }));
  });

  it('requires explicit confirmation and retains profile after tear down', async () => {
    const user = userEvent.setup();
    renderRunningCard();
    await user.click(screen.getByRole('button', { name: /more actions/i }));
    await user.click(screen.getByRole('menuitem', { name: /tear down/i }));
    expect(screen.getByRole('alertdialog', { name: /tear down project/i })).toBeVisible();
    expect(screen.getByRole('alertdialog')).toHaveTextContent(/containers and networks.*removed/i);
    await user.click(screen.getByRole('button', { name: /^tear down$/i }));
    await waitFor(() => expect(screen.getByText('Test Project')).toBeVisible());
    expect(mockBackend.getInvocations().find(invocation => invocation.command === 'tear_down_project')?.args).toEqual({ profileId });
  });

  it('cancels destructive confirmation without IPC', async () => {
    const user = userEvent.setup();
    renderRunningCard();
    await user.click(screen.getByRole('button', { name: /more actions/i }));
    await user.click(screen.getByRole('menuitem', { name: /remove profile/i }));
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(mockBackend.getInvocations().filter(invocation => invocation.command === 'remove_profile')).toHaveLength(0);
    expect(screen.getByText('Test Project')).toBeVisible();
  });
});
