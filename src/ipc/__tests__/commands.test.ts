import { describe, expect, it } from 'vitest';
import { applyProject, createProfile } from '../commands';
import { mockBackend } from '../mock-backend';

describe('typed commands', () => {
  it('rejects lifecycle arguments containing backend authority', () => {
    expect(() => applyProject({ profileId: 'id-1', workingDirectory: '/tmp' } as never)).toThrow();
  });

  it('accepts only profileId for lifecycle calls', async () => {
    mockBackend.reset();
    await createProfile({ displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', composeFiles: ['compose.yml'], environmentFiles: [] });
    const result = await applyProject('00000000-0000-0000-0000-000000000001');
    expect(result.profileId).toBe('00000000-0000-0000-0000-000000000001');
  });
});
