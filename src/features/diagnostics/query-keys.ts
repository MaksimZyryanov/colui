import type { ContainerLogsRequest } from '../../ipc/types';

export const diagnosticsKeys = {
  all: () => ['diagnostics'] as const,
  snapshot: () => ['diagnostics', 'snapshot'] as const,
};
export const logKeys = {
  all: () => ['container-logs'] as const,
  snapshot: (request: ContainerLogsRequest) => ['container-logs', request.runtimeSessionId, request.containerId, { ...request }] as const,
};
