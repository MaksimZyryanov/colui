export const runtimeKeys = {
  all: ['runtime'] as const,
  state: () => [...runtimeKeys.all, 'state'] as const,
};
