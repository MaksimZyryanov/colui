export const projectKeys = {
  all: () => ['projects'] as const,
  list: () => ['projects', 'list'] as const,
  detail: (profileId: string) => ['projects', 'detail', profileId] as const,
  status: (profileId: string) => ['projects', 'status', profileId] as const,
};
