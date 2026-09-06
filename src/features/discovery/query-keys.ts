import type { RegistryHealth } from '../../ipc/types';

export const discoveryKeys = {
  all: () => ['discovery'] as const,
  configuration: () => ['discovery', 'configuration'] as const,
  list: (runtimeSessionId: string | null, inventoryGeneration: number, health: RegistryHealth | null) => ['discovery', 'list', runtimeSessionId, inventoryGeneration, {
    state: health?.state ?? null,
    identity: health?.identity ? { kind: 'snapshot', registryRevision: health.identity.registryRevision, canonicalContentSha256: health.identity.canonicalContentSha256 } : null,
  }] as const,
};
