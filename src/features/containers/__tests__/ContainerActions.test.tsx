// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ContainerActions } from '../ContainerActions';
import { mockBackend } from '../../../ipc/mock-backend';
import type { RuntimeInventory } from '../../../ipc/types';

const session = '00000000-0000-0000-0000-000000000099';
const container = (id: string): RuntimeInventory['containers'][number] => ({ id, name: id, image: 'alpine:3.20', state: 'stopped', statusText: 'Exited', serviceName: null, publishedPorts: [] });
const inventory: RuntimeInventory = { generation: 1, hasSnapshot: true, runtimeSessionId: session, daemonFingerprint: null, observedAt: '2026-09-03T00:00:00.000Z', freshness: 'fresh', lastSuccessfulObservedAt: '2026-09-03T00:00:00.000Z', containers: [], projects: [], composeObservationGroups: [], standaloneContainers: [], error: null };
let client: QueryClient;
beforeEach(() => { mockBackend.reset(); client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } }); });
afterEach(() => { cleanup(); client.clear(); });

describe('ContainerActions', () => {
  it('reserves every batch target while an earlier container action is pending', async () => {
    let resolveFirst!: (value: unknown) => void;
    const first = new Promise(resolve => { resolveFirst = resolve; });
    mockBackend.setResponseOverride('run_container_action', first);
    const user = userEvent.setup();
    render(<QueryClientProvider client={client}><ContainerActions containers={[container('alpha'), container('zeta')]} runtimeSessionId={session} resourceName="stack" /><ContainerActions containers={[container('zeta')]} runtimeSessionId={session} resourceName="zeta" /></QueryClientProvider>);
    await user.click(screen.getByRole('button', { name: 'Start stack' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Start zeta' })).toBeDisabled());
    mockBackend.setResponseOverride('run_container_action', { containerId: 'zeta', action: 'start', observation: 'confirmed_in_session', inventory: { ...inventory, generation: 2 } });
    await act(async () => { resolveFirst({ containerId: 'alpha', action: 'start', observation: 'confirmed_in_session', inventory }); });
    await waitFor(() => expect(screen.getByRole('button', { name: 'Start zeta' })).toBeEnabled());
    expect(mockBackend.getInvocations().filter(value => value.command === 'run_container_action').map(value => value.args)).toEqual([{ request: { containerId: 'alpha', runtimeSessionId: session, action: 'start' } }, { request: { containerId: 'zeta', runtimeSessionId: session, action: 'start' } }]);
  });

  it('contains a batch rejection, stops subsequent requests, and exposes typed details', async () => {
    mockBackend.setErrorOverride('run_container_action', { code: 'container_operation_failed', operation: 'run_container_action', message: 'Container start failed', details: 'Permission denied by daemon', retryable: true });
    const user = userEvent.setup();
    render(<QueryClientProvider client={client}><ContainerActions containers={[container('alpha'), container('zeta')]} runtimeSessionId={session} resourceName="stack" /></QueryClientProvider>);
    await user.click(screen.getByRole('button', { name: 'Start stack' }));
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Container start failed');
    expect(alert).toHaveTextContent('Permission denied by daemon');
    expect(mockBackend.getInvocations().filter(value => value.command === 'run_container_action')).toHaveLength(1);
    expect(screen.getByRole('button', { name: 'Start stack' })).toBeEnabled();
  });

  it('stops the batch when an action cannot be confirmed in its runtime session', async () => {
    mockBackend.setResponseOverride('run_container_action', { containerId: 'alpha', action: 'start', observation: 'indeterminate_after_session_change', inventory: { ...inventory, freshness: 'stale' } });
    const user = userEvent.setup();
    render(<QueryClientProvider client={client}><ContainerActions containers={[container('alpha'), container('zeta')]} runtimeSessionId={session} resourceName="stack" /></QueryClientProvider>);
    await user.click(screen.getByRole('button', { name: 'Start stack' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Runtime session changed');
    expect(mockBackend.getInvocations().filter(value => value.command === 'run_container_action')).toHaveLength(1);
  });
});
