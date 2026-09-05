// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mockBackend } from '../../../ipc/mock-backend';
import type { RuntimeInventory } from '../../../ipc/types';
import { OtherContainers } from '../OtherContainers';
import { AppErrorException } from '../../../ipc/errors';

const session = '00000000-0000-0000-0000-000000000099';
const container = (id: string, name: string) => ({ id, name, image: 'nginx:latest', state: 'running' as const, statusText: 'Up', serviceName: null, publishedPorts: [{ hostIp: '0.0.0.0', hostPort: 8080, containerPort: 80, protocol: 'tcp', action: { copy: '0.0.0.0:8080 -> 80/tcp', url: 'http://127.0.0.1:8080' } }, { hostIp: '::', hostPort: null, containerPort: 53, protocol: 'udp', action: { copy: '[::]:? -> 53/udp', url: null } }] });
const inventory = { generation: 4, hasSnapshot: true, observedAt: '2026-09-02T00:00:00.000Z', runtimeSessionId: session, daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, freshness: 'fresh', lastSuccessfulObservedAt: '2026-09-02T00:00:00.000Z', containers: [], projects: [], composeObservationGroups: [], standaloneContainers: [container('one', 'web-one'), container('two', 'web-two')], error: null } satisfies RuntimeInventory;

describe('OtherContainers', () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  beforeEach(() => { client.clear(); mockBackend.reset(); Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async () => undefined } }); });
  afterEach(cleanup);

  it('isolates pending action by immutable container id and recovers when it disappears', async () => {
    let resolve!: (value: unknown) => void;
    mockBackend.setResponseOverride('run_container_action', new Promise(done => { resolve = done; }));
    const user = userEvent.setup();
    const { rerender } = render(<QueryClientProvider client={client}><OtherContainers inventory={inventory} /></QueryClientProvider>);
    await user.click(screen.getByRole('button', { name: /stop web-one/i }));
    expect(screen.getByRole('button', { name: /stop web-one/i })).toBeDisabled();
    expect(screen.getByRole('button', { name: /stop web-two/i })).toBeEnabled();
    rerender(<QueryClientProvider client={client}><OtherContainers inventory={{ ...inventory, generation: 5, standaloneContainers: [container('two', 'web-two')] }} /></QueryClientProvider>);
    expect(screen.queryByText('web-one')).not.toBeInTheDocument();
    resolve({});
  });

  it('shows selectable bounded logs, truncation notice, and restores focus on Escape', async () => {
    const user = userEvent.setup();
    mockBackend.setResponseOverride('get_container_logs', { containerId: 'one', text: 'latest line \ufffd', retainedBytes: 262144, truncated: true, observedAt: '2026-09-02T00:00:00.000Z' });
    render(<QueryClientProvider client={client}><OtherContainers inventory={inventory} /></QueryClientProvider>);
    const trigger = screen.getByRole('button', { name: /view logs for web-one/i });
    await user.click(trigger);
    expect(await screen.findByText('latest line \ufffd')).toHaveClass('container-logs');
    expect(screen.getByRole('status', { name: /logs truncated/i })).toBeVisible();
    await user.keyboard('{Escape}');
    expect(trigger).toHaveFocus();
  });

  it('copies exact backend text and opens only eligible bindings', async () => {
    const user = userEvent.setup();
    render(<QueryClientProvider client={client}><OtherContainers inventory={inventory} /></QueryClientProvider>);
    await user.click(screen.getAllByRole('button', { name: /copy 0\.0\.0\.0:8080/i })[0]);
    expect(await navigator.clipboard.readText()).toBe('0.0.0.0:8080 -> 80/tcp');
    expect(screen.getAllByRole('button', { name: /open .*browser/i })).toHaveLength(2);
    await user.click(screen.getAllByRole('button', { name: /open .*browser/i })[0]);
    await waitFor(() => expect(mockBackend.getInvocations().some(call => call.command === 'open_container_port')).toBe(true));
    expect(await screen.findAllByRole('status', { name: 'Open port succeeded' })).toHaveLength(1);
  });

  it('announces clipboard and port open success and failure', async () => {
    const user = userEvent.setup();
    render(<QueryClientProvider client={client}><OtherContainers inventory={{ ...inventory, standaloneContainers: [container('one', 'web-one')] }} /></QueryClientProvider>);
    await user.click(screen.getByRole('button', { name: /copy 0\.0\.0\.0:8080/i }));
    expect(await screen.findByRole('status', { name: 'Copy port succeeded' })).toBeVisible();
    mockBackend.setErrorOverride('open_container_port', new AppErrorException({ code: 'container_operation_failed', operation: 'open_container_port', subject: { kind: 'container', id: 'one' }, message: 'Port unavailable', details: 'Binding changed', retryable: false }));
    await user.click(screen.getByRole('button', { name: /open .*browser/i }));
    expect(await screen.findByRole('alert', { name: 'Open port failed' })).toHaveTextContent('container_operation_failed');
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async () => { throw new Error('Permission denied'); } } });
    await user.click(screen.getByRole('button', { name: /copy 0\.0\.0\.0:8080/i }));
    expect(await screen.findByRole('alert', { name: 'Copy port failed' })).toHaveTextContent('Permission denied');
  });

  it.each([
    ['operation_conflict', 'Action failed', true, 'Another action is already running'],
    ['runtime_unavailable', 'Action failed', true, 'Runtime session changed'],
    ['container_operation_failed', 'Runtime session is stale', false, 'Runtime session changed'],
    ['container_operation_failed', 'Action failed', true, 'Docker daemon rejected the action'],
    ['container_operation_failed', 'Action failed', false, 'Container disappeared or Docker rejected the action'],
  ] as const)('renders typed %s container action recovery', async (code, message, retryable, recovery) => {
    mockBackend.setErrorOverride('run_container_action', new AppErrorException({ code, operation: 'run_container_action', subject: { kind: 'container', id: 'one' }, message, details: 'Safe technical detail', retryable }));
    const user = userEvent.setup();
    render(<QueryClientProvider client={client}><OtherContainers inventory={{ ...inventory, standaloneContainers: [container('one', 'web-one')] }} /></QueryClientProvider>);
    await user.click(screen.getByRole('button', { name: /stop web-one/i }));
    const alert = await screen.findByRole('alert', { name: 'Stop web-one failed' });
    expect(alert).toHaveTextContent(code);
    expect(alert).toHaveTextContent(recovery);
    expect(screen.getByText('Safe technical detail').closest('details')).not.toHaveAttribute('open');
    await user.click(screen.getByText('Technical details'));
    expect(screen.getByText('Safe technical detail').closest('details')).toHaveAttribute('open');
    expect(alert).toHaveTextContent('Safe technical detail');
  });
});
