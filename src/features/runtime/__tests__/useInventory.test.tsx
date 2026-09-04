// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
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
});
