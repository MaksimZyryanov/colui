// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { describe, expect, it } from 'vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, vi } from 'vitest';
import { mockBackend } from '../../../ipc/mock-backend';
import { AppErrorException } from '../../../ipc/errors';
import { useInventory } from '../hooks/useInventory';
import { inventoryKeys } from '../query-keys';
import { inventoryStructuralSharing } from '../hooks/useInventory';
import type { RuntimeInventory } from '../../../ipc/types';

const publishedInventory = (generation: number): RuntimeInventory => ({
  generation,
  hasSnapshot: true,
  observedAt: '2026-09-03T00:00:00.000Z',
  runtimeSessionId: '00000000-0000-0000-0000-000000000099',
  daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' },
  freshness: 'fresh',
  lastSuccessfulObservedAt: '2026-09-03T00:00:00.000Z',
  containers: [],
  projects: [],
  standaloneContainers: [],
  error: null,
});

const preObservation = (): RuntimeInventory => ({
  generation: 0,
  hasSnapshot: false,
  observedAt: null,
  runtimeSessionId: null,
  daemonFingerprint: null,
  freshness: 'unavailable',
  lastSuccessfulObservedAt: null,
  containers: [],
  projects: [],
  standaloneContainers: [],
  error: null,
});

describe('inventory structural sharing', () => {
  it('keeps cache reference for equal or lower generation', () => {
    const oldData = publishedInventory(4);
    expect(inventoryStructuralSharing(oldData, publishedInventory(4))).toBe(oldData);
    expect(inventoryStructuralSharing(oldData, publishedInventory(3))).toBe(oldData);
  });

  it('accepts first snapshot despite pre-observation generation zero', () => {
    expect(inventoryStructuralSharing(preObservation(), publishedInventory(1))).toEqual(publishedInventory(1));
  });

  it('retains published snapshot when generation-zero unavailable response arrives', () => {
    const oldData = publishedInventory(4);
    expect(inventoryStructuralSharing(oldData, preObservation())).toBe(oldData);
  });
});

function InventoryProbe() {
  const query = useInventory();
  return <output data-testid="inventory">{query.data?.generation ?? 'loading'}:{query.error instanceof Error ? query.error.message : 'ok'}</output>;
}

const renderInventory = () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}><InventoryProbe /></QueryClientProvider>);
  return client;
};

describe('useInventory polling lifecycle', () => {
  const published = publishedInventory(1);

  beforeEach(() => {
    vi.useFakeTimers();
    mockBackend.reset();
    mockBackend.setResponseOverride('refresh_inventory', published);
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });
  });
  afterEach(() => { cleanup(); vi.useRealTimers(); });

  it('polls once at each visible three-second interval', async () => {
    renderInventory();
    await act(async () => { await Promise.resolve(); });
    expect(mockBackend.getInvocations().filter(invocation => invocation.command === 'refresh_inventory')).toHaveLength(1);
    await act(async () => { vi.advanceTimersByTime(2999); });
    expect(mockBackend.getInvocations().filter(invocation => invocation.command === 'refresh_inventory')).toHaveLength(1);
    await act(async () => { vi.advanceTimersByTime(1); await Promise.resolve(); });
    expect(mockBackend.getInvocations().filter(invocation => invocation.command === 'refresh_inventory')).toHaveLength(2);
  });

  it('pauses hidden polling and refetches once when visible again', async () => {
    renderInventory();
    await act(async () => { await Promise.resolve(); });
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'hidden' });
    document.dispatchEvent(new Event('visibilitychange'));
    await act(async () => { vi.advanceTimersByTime(6000); });
    expect(mockBackend.getInvocations().filter(invocation => invocation.command === 'refresh_inventory')).toHaveLength(1);
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });
    await act(async () => { document.dispatchEvent(new Event('visibilitychange')); await Promise.resolve(); });
    expect(mockBackend.getInvocations().filter(invocation => invocation.command === 'refresh_inventory')).toHaveLength(2);
    await act(async () => { vi.advanceTimersByTime(3000); await Promise.resolve(); });
    expect(mockBackend.getInvocations().filter(invocation => invocation.command === 'refresh_inventory')).toHaveLength(3);
  });

  it('retains published inventory while exposing refresh failure', async () => {
    const client = renderInventory();
    await act(async () => { await Promise.resolve(); });
    const retained = client.getQueryData(inventoryKeys.snapshot());
    mockBackend.setErrorOverride('refresh_inventory', new AppErrorException({ code: 'runtime_unavailable', operation: 'refresh_inventory', subjectId: null, message: 'offline', details: null, retryable: false }));
    await act(async () => { await vi.advanceTimersByTimeAsync(3000); });
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });
    expect(screen.getByTestId('inventory')).toHaveTextContent('1:offline');
    expect(client.getQueryData(inventoryKeys.snapshot())).toBe(retained);
  });
});
