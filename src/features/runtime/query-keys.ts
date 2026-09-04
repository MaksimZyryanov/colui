export const runtimeKeys = {
  all: ['runtime'] as const,
  state: () => [...runtimeKeys.all, 'state'] as const,
};
export const inventoryKeys = {
  all: () => ['inventory'] as const,
  snapshot: () => ['inventory', 'snapshot'] as const,
};
