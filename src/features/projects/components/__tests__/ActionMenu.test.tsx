// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ProfileCard } from '../ProfileCard';
import { ActionMenu } from '../ActionMenu';
import { mockBackend } from '../../../../ipc/mock-backend';
import { inventoryKeys } from '../../../runtime/query-keys';

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

  it('sends exact Apply payload', async () => {
    const user = userEvent.setup();
    renderRunningCard();
    await user.click(screen.getByRole('button', { name: /more actions/i }));
    await user.click(screen.getByRole('menuitem', { name: 'Apply' }));
    await waitFor(() => expect(mockBackend.getInvocations().find(invocation => invocation.command === 'apply_project')?.args).toEqual({ profileId }));
  });

  it('publishes lifecycle inventory without refresh, invalidation, or refetch', async () => {
    const oldInventory = { generation: 4, hasSnapshot: true, observedAt: '2026-09-03T00:00:00.000Z', runtimeSessionId: '00000000-0000-0000-0000-000000000099', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, freshness: 'fresh' as const, lastSuccessfulObservedAt: '2026-09-03T00:00:00.000Z', containers: [], projects: [], standaloneContainers: [], error: null };
    const newer = { ...oldInventory, generation: 5 };
    client.setQueryData(inventoryKeys.snapshot(), oldInventory);
    mockBackend.setResponseOverride('apply_project', { profileId, success: true, inventoryGeneration: 4, inventory: { ...oldInventory } });
    const invalidate = vi.spyOn(client, 'invalidateQueries');
    const refetch = vi.spyOn(client, 'refetchQueries');
    const user = userEvent.setup();
    renderRunningCard();
    await user.click(screen.getByRole('button', { name: /more actions/i }));
    await user.click(screen.getByRole('menuitem', { name: 'Apply' }));
    await waitFor(() => expect(client.getQueryData(inventoryKeys.snapshot())).toBe(oldInventory));
    mockBackend.setResponseOverride('restart_project', { profileId, success: true, inventoryGeneration: 5, inventory: newer });
    await user.click(screen.getByRole('button', { name: 'Restart' }));
    await waitFor(() => expect(client.getQueryData(inventoryKeys.snapshot())).toEqual(newer));
    expect(mockBackend.getInvocations().filter(({ command }) => command === 'refresh_inventory')).toHaveLength(0);
    expect(invalidate).not.toHaveBeenCalled();
    expect(refetch).not.toHaveBeenCalled();
  });

  it('sends exact Restart payload', async () => {
    const user = userEvent.setup();
    renderRunningCard();
    await user.click(screen.getByRole('button', { name: /restart/i }));
    await waitFor(() => expect(mockBackend.getInvocations().find(invocation => invocation.command === 'restart_project')?.args).toEqual({ profileId }));
  });

  it('sends exact Remove profile payload', async () => {
    const user = userEvent.setup();
    renderRunningCard();
    await user.click(screen.getByRole('button', { name: /more actions/i }));
    await user.click(screen.getByRole('menuitem', { name: 'Remove profile' }));
    expect(screen.getByRole('alertdialog', { name: 'Remove profile' })).toHaveTextContent('Remove this profile. Docker is untouched.');
    await user.click(screen.getByRole('button', { name: /^Remove profile$/ }));
    await waitFor(() => expect(mockBackend.getInvocations().find(invocation => invocation.command === 'remove_profile')?.args).toEqual({ profileId, expectedRevision: 2 }));
  });

  it('keeps Remove available when status is unavailable and disables Compose actions', async () => {
    const user = userEvent.setup();
    render(<QueryClientProvider client={client}><ProfileCard profile={profile} initialStatus={{ profileId, runtime: { presence: 'unavailable', activity: null, containerCount: 0, runningContainerCount: 0, observedAt: null }, definition: { state: 'unchecked', revision: null, serviceCount: null }, operation: null, issues: [] }} runtimeReady={false} /></QueryClientProvider>);
    expect(screen.getByRole('button', { name: 'Stop' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Restart' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: /more actions/i }));
    expect(screen.getByRole('menuitem', { name: 'Apply' })).toHaveAttribute('data-disabled');
    expect(screen.getByRole('menuitem', { name: 'Tear down' })).toHaveAttribute('data-disabled');
    expect(screen.getByRole('menuitem', { name: 'Remove profile' })).not.toHaveAttribute('data-disabled');
  });

  it('isolates pending state between mounted profile cards', async () => {
    const secondProfileId = '00000000-0000-0000-0000-000000000002';
    let resolveApply!: (value: unknown) => void;
    mockBackend.setResponseOverride('apply_project', new Promise(resolve => { resolveApply = resolve; }));
    mockBackend.setResponseOverride('restart_project', { profileId: secondProfileId, success: true });
    const user = userEvent.setup();
    render(<QueryClientProvider client={client}><ActionMenu profileId={profileId} revision={2} status={status} runtimeReady /><ActionMenu profileId={secondProfileId} revision={1} status={{ ...status, profileId: secondProfileId }} runtimeReady /></QueryClientProvider>);
    const applyMenu = screen.getAllByRole('button', { name: /more actions/i })[0];
    await user.click(applyMenu);
    await user.click(screen.getByRole('menuitem', { name: 'Apply' }));
    await waitFor(() => expect(screen.getAllByRole('button', { name: 'Restart' })[0]).toBeDisabled());
    expect(screen.getAllByRole('button', { name: 'Restart' })[1]).toBeEnabled();
    await user.click(screen.getAllByRole('button', { name: 'Restart' })[1]);
    await waitFor(() => expect(mockBackend.getInvocations().find(invocation => invocation.command === 'restart_project')?.args).toEqual({ profileId: secondProfileId }));
    resolveApply({ profileId, success: true });
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

  it('focuses Cancel, closes on Escape, and returns focus to More actions', async () => {
    const user = userEvent.setup();
    renderRunningCard();
    const more = screen.getByRole('button', { name: /more actions/i });
    await user.click(more);
    await user.click(screen.getByRole('menuitem', { name: /tear down/i }));
    expect(screen.getByRole('button', { name: 'Cancel' })).toHaveFocus();
    await user.keyboard('{Escape}');
    await waitFor(() => expect(more).toHaveFocus());
  });

  it('retains dialog and typed error when destructive action rejects', async () => {
    const user = userEvent.setup();
    mockBackend.setErrorOverride('remove_profile', new Error('registry locked'));
    renderRunningCard();
    await user.click(screen.getByRole('button', { name: /more actions/i }));
    await user.click(screen.getByRole('menuitem', { name: /remove profile/i }));
    await user.click(screen.getByRole('button', { name: /^Remove profile$/ }));
    expect(await screen.findByRole('alert')).toHaveTextContent('registry locked');
    expect(screen.getByRole('alertdialog', { name: 'Remove profile' })).toBeVisible();
  });
});
