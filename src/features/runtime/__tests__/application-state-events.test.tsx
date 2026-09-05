// @vitest-environment jsdom
import { act, cleanup, renderHook } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';
import { mockBackend } from '../../../ipc/mock-backend';
import { useApplicationStateEvents } from '../useApplicationStateEvents';
import { inventoryKeys, runtimeKeys } from '../query-keys';
import { discoveryKeys } from '../../discovery/query-keys';
import { diagnosticsKeys, logKeys } from '../../diagnostics/query-keys';
import { projectKeys } from '../../projects/query-keys';
import { useDiagnostics, useRegistryRecovery, useContainerLogs } from '../../diagnostics/hooks';
import { useDiscovery, useDiscoveryMutations } from '../../discovery/hooks';
import { useContainerAction } from '../../containers/hooks';
import { useConnectRuntime, useDisconnectRuntime, useReconnectRuntime, useRuntimeState } from '../hooks/useRuntimeSession';
import { useProfileMutations } from '../../projects/hooks/useProfileMutations';
import type { DiagnosticsSnapshot, RuntimeInventory } from '../../../ipc/types';

const transport = vi.hoisted(() => ({ listener: undefined as undefined | ((event: { payload: unknown }) => void), unlisten: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async (_name, listener) => { transport.listener = listener; return transport.unlisten; }) }));
const session = '00000000-0000-0000-0000-000000000001';
const nextSession = '00000000-0000-0000-0000-000000000002';
const fingerprint = { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' };
const ready = (sessionId = session) => ({ state: 'ready' as const, context: { sessionId, endpoint: 'mock://runtime', daemonFingerprint: fingerprint, connectedAt: '2026-09-04T00:00:00Z' } });
const inventory = (generation = 1): RuntimeInventory => ({ generation, hasSnapshot: true, runtimeSessionId: session, daemonFingerprint: fingerprint, observedAt: '2026-09-04T00:00:00Z', lastSuccessfulObservedAt: '2026-09-04T00:00:00Z', freshness: 'fresh', containers: [], projects: [], composeObservationGroups: [], standaloneContainers: [], error: null });
const identity = { registryRevision: 1, canonicalContentSha256: 'a'.repeat(64) };
const health = { state: 'healthy' as const, identity, error: null, lastOperationAt: null, lastFailureAt: null };
const diagnostics = (): DiagnosticsSnapshot => ({ runtime: { state: ready(), sessionId: session, connectedAt: '2026-09-04T00:00:00Z', resolvedEndpoint: 'mock://runtime', apiFingerprint: fingerprint, cliFingerprint: fingerprint }, registry: { registryPath: '/registry', backupPath: '/backup', backup: { exists: false, modifiedAt: null, state: 'missing', error: null }, revision: 1, health, lockTimeoutMs: 500, lastRecoveryResult: null }, import: { sourcePath: '/source', importedCount: 0, sourcePreserved: true, error: null }, operations: { generation: 0, active: [] }, inventory: inventory(), definitions: { generation: 0, profiles: [] }, journal: { entries: [] } });
const draft = { displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', composeFiles: ['compose.yml'], environmentFiles: [] };
const profile = { id: session, revision: 1, ...draft, registrationOrigin: 'discovered' };
const candidateRequest = { candidateId: 'a'.repeat(64), runtimeSessionId: session, inventoryGeneration: 1, composeProjectName: 'demo', workingDirectory: '/tmp', configFiles: ['compose.yml'] };
let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
const flush = () => act(async () => { await vi.advanceTimersByTimeAsync(1); });
const discoveryKey = () => discoveryKeys.list(session, 1, health);
function seed() {
  client.setQueryData(projectKeys.list(), []);
  client.setQueryData(discoveryKey(), { candidates: [], autoRegistrationEnabled: false });
  client.setQueryData(diagnosticsKeys.snapshot(), diagnostics());
  client.setQueryData(logKeys.snapshot({ runtimeSessionId: session, containerId: 'container' }), { text: 'old logs' });
  client.setQueryData(runtimeKeys.state(), ready());
}

beforeEach(() => {
  vi.useFakeTimers(); mockBackend.reset(); transport.listener = undefined; transport.unlisten.mockClear();
  client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity }, mutations: { retry: false } } });
  Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });
  mockBackend.setResponseOverride('get_diagnostics', diagnostics());
});
afterEach(() => { cleanup(); client.clear(); vi.useRealTimers(); vi.unstubAllEnvs(); });

describe('application state coherence', () => {
  it('keys discovery by session, generation, full nullable identity and health, without sentinel collisions', () => {
    const keys = [discoveryKey(), discoveryKeys.list(nextSession, 1, health), discoveryKeys.list(session, 2, health), discoveryKeys.list(session, 1, { ...health, identity: { ...identity, canonicalContentSha256: 'b'.repeat(64) } }), discoveryKeys.list(session, 1, { ...health, identity: null }), discoveryKeys.list(session, 1, { ...health, state: 'corrupt', identity: null }), discoveryKeys.list(session, 1, { ...health, identity: { registryRevision: 0, canonicalContentSha256: '0'.repeat(64) } })];
    expect(new Set(keys.map(key => JSON.stringify(key))).size).toBe(7);
    expect(logKeys.snapshot({ runtimeSessionId: session, containerId: 'container' })).not.toEqual(logKeys.snapshot({ runtimeSessionId: nextSession, containerId: 'container' }));
  });

  it('subscribes once, decodes hints, ignores duplicate/lower sequences and never trusts event data', async () => {
    vi.stubEnv('MODE', 'production');
    seed();
    const { rerender, unmount } = renderHook(useApplicationStateEvents, { wrapper });
    await flush(); rerender();
    const { listen } = await import('@tauri-apps/api/event');
    expect(listen).toHaveBeenCalledTimes(1);
    const profiles = client.getQueryData(projectKeys.list());
    await act(async () => { transport.listener?.({ payload: { sequence: 2, scopes: ['profiles', 'discovery', 'diagnostics'], profiles: ['untrusted'] } }); });
    expect(client.getQueryData(projectKeys.list())).toBe(profiles);
    expect(client.getQueryState(projectKeys.list())?.isInvalidated).toBe(true);
    expect(client.getQueryState(discoveryKey())?.isInvalidated).toBe(true);
    client.setQueryData(projectKeys.list(), profiles);
    for (const sequence of [2, 1]) transport.listener?.({ payload: { sequence, scopes: ['profiles'] } });
    transport.listener?.({ payload: { sequence: 3, scopes: ['unknown'] } });
    expect(client.getQueryState(projectKeys.list())?.isInvalidated).toBe(false);
    unmount(); expect(transport.unlisten).toHaveBeenCalledTimes(1);
  });

  it('polls diagnostics every two visible seconds, repairs missed registry events, and does not poll inventory or loop on reads', async () => {
    seed();
    renderHook(() => { useApplicationStateEvents(); return useDiagnostics(); }, { wrapper });
    await flush();
    const changed = diagnostics(); changed.registry.health = { ...health, identity: { ...identity, canonicalContentSha256: 'b'.repeat(64) } };
    mockBackend.setResponseOverride('get_diagnostics', changed);
    await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
    expect(client.getQueryState(projectKeys.list())?.isInvalidated).toBe(true);
    expect(client.getQueryState(discoveryKey())?.isInvalidated).toBe(true);
    expect(mockBackend.getInvocations().filter(v => v.command === 'get_inventory')).toHaveLength(0);
    const reads = mockBackend.getInvocations().filter(v => v.command === 'get_diagnostics').length;
    await act(async () => { await vi.advanceTimersByTimeAsync(100); });
    expect(mockBackend.getInvocations().filter(v => v.command === 'get_diagnostics')).toHaveLength(reads);
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'hidden' });
    await act(async () => { document.dispatchEvent(new Event('visibilitychange')); await vi.advanceTimersByTimeAsync(6000); });
    expect(mockBackend.getInvocations().filter(v => v.command === 'get_diagnostics')).toHaveLength(reads);
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });
    await act(async () => { document.dispatchEvent(new Event('visibilitychange')); });
    expect(mockBackend.getInvocations().filter(v => v.command === 'get_diagnostics')).toHaveLength(reads + 1);
  });

  it('repairs missed auto-registration through journal sequence even when registry identity is unchanged', async () => {
    seed(); renderHook(useDiagnostics, { wrapper }); await flush();
    const changed = diagnostics(); changed.journal.entries = [{ sequence: 4, timestamp: '2026-09-04T00:00:00Z', runtimeSessionId: session, kind: 'auto_registration_succeeded', severity: 'info', subject: null, errorCode: null, message: 'Registered' }];
    mockBackend.setResponseOverride('get_diagnostics', changed);
    await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
    expect(client.getQueryState(projectKeys.list())?.isInvalidated).toBe(true);
    client.setQueryData(projectKeys.list(), []);
    await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
    expect(client.getQueryState(projectKeys.list())?.isInvalidated).toBe(false);
  });

  it.each(['create', 'update', 'remove', 'restore', 'register', 'auto'] as const)('%s invalidates profiles and discovery without relying on event delivery', async operation => {
    seed();
    mockBackend.setResponseOverride('create_profile', profile); mockBackend.setResponseOverride('update_profile', profile); mockBackend.setResponseOverride('remove_profile', null);
    mockBackend.setResponseOverride('restore_registry_backup', [profile]); mockBackend.setResponseOverride('register_candidate', profile); mockBackend.setResponseOverride('auto_register_candidates', { profiles: [profile], errors: [] });
    const { result } = renderHook(() => ({ profiles: useProfileMutations(), discovery: useDiscoveryMutations(), recovery: useRegistryRecovery() }), { wrapper });
    await act(async () => {
      if (operation === 'create') await result.current.profiles.create.mutateAsync(draft);
      if (operation === 'update') await result.current.profiles.update.mutateAsync({ profileId: session, expectedRevision: 1, patch: draft });
      if (operation === 'remove') await result.current.profiles.remove.mutateAsync({ profileId: session, expectedRevision: 1 });
      if (operation === 'restore') await result.current.recovery.restore.mutateAsync();
      if (operation === 'register') await result.current.discovery.register.mutateAsync(candidateRequest);
      if (operation === 'auto') await result.current.discovery.autoRegister.mutateAsync({ runtimeSessionId: session, inventoryGeneration: 1 });
    });
    expect(client.getQueryState(projectKeys.list())?.isInvalidated).toBe(true);
    expect(client.getQueryState(discoveryKey())?.isInvalidated).toBe(true);
  });

  it('ignore invalidates discovery without invalidating profiles', async () => {
    seed(); mockBackend.setResponseOverride('ignore_candidate', true);
    const { result } = renderHook(useDiscoveryMutations, { wrapper });
    await act(async () => { await result.current.ignore.mutateAsync({ candidateId: 'a'.repeat(64) }); });
    expect(client.getQueryState(discoveryKey())?.isInvalidated).toBe(true);
    expect(client.getQueryState(projectKeys.list())?.isInvalidated).toBe(false);
  });

  it('logs are explicit, nonretrying and never refetch automatically', async () => {
    const { result } = renderHook(() => useContainerLogs({ runtimeSessionId: session, containerId: 'container' }), { wrapper });
    await flush(); expect(mockBackend.getInvocations()).toHaveLength(0);
    mockBackend.setErrorOverride('get_container_logs', { code: 'runtime_unavailable', operation: 'get_container_logs', message: 'Offline', retryable: true });
    let response: Awaited<ReturnType<typeof result.current.refetch>> | undefined;
    await act(async () => { response = await result.current.refetch(); await vi.advanceTimersByTimeAsync(10000); });
    expect(response?.error).toMatchObject({ code: 'runtime_unavailable' });
    expect(mockBackend.getInvocations().filter(v => v.command === 'get_container_logs')).toHaveLength(1);
  });

  it('container actions publish embedded inventory through equal/lower generation guard without a second read', async () => {
    client.setQueryData(inventoryKeys.snapshot(), inventory(4));
    const retained = client.getQueryData(inventoryKeys.snapshot());
    const { result } = renderHook(useContainerAction, { wrapper });
    for (const generation of [3, 4, 5]) {
      mockBackend.setResponseOverride('run_container_action', { containerId: 'container', action: 'start', observation: 'confirmed_in_session', inventory: inventory(generation) });
      await act(async () => { await result.current.mutateAsync({ containerId: 'container', action: 'start', runtimeSessionId: session }); });
      if (generation <= 4) expect(client.getQueryData(inventoryKeys.snapshot())).toBe(retained);
    }
    expect(client.getQueryData(inventoryKeys.snapshot())).toMatchObject({ generation: 5 });
    expect(mockBackend.getInvocations().some(v => /^(get|refresh)_inventory$/.test(v.command))).toBe(false);
  });

  it.each(['connect', 'disconnect', 'reconnect', 'read'] as const)('%s clears old session caches and reconnect guards embedded inventory', async operation => {
    seed(); client.setQueryData(inventoryKeys.snapshot(), inventory(4)); const retained = client.getQueryData(inventoryKeys.snapshot());
    mockBackend.setResponseOverride('connect_runtime', ready(nextSession)); mockBackend.setResponseOverride('disconnect_runtime', { state: 'disconnected' });
    mockBackend.setResponseOverride('reconnect_runtime', { runtimeState: ready(nextSession), inventory: inventory(3) }); mockBackend.setResponseOverride('get_runtime_state', ready(nextSession));
    const { result } = renderHook(() => ({ connect: useConnectRuntime(), disconnect: useDisconnectRuntime(), reconnect: useReconnectRuntime(), read: useRuntimeState() }), { wrapper });
    await act(async () => {
      if (operation === 'read') await result.current.read.refetch();
      else await result.current[operation].mutateAsync();
    });
    expect(client.getQueriesData({ queryKey: discoveryKeys.all() })).toHaveLength(0);
    expect(client.getQueriesData({ queryKey: logKeys.all() })).toHaveLength(0);
    expect(client.getQueryData(inventoryKeys.snapshot())).toBe(retained);
    expect(client.getQueryData(runtimeKeys.state())).toMatchObject(operation === 'disconnect' ? { state: 'disconnected' } : ready(nextSession));
  });

  it('discovery observes keyed projection without starting another diagnostics or inventory observer', async () => {
    renderHook(() => useDiscovery(session, 1, health), { wrapper }); await flush();
    expect(mockBackend.getInvocations().map(v => v.command)).toEqual(['list_discovery_candidates']);
  });
});
