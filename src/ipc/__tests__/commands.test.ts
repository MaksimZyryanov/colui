import { describe, expect, it } from 'vitest';
import { applyProject, createProfile, getProfile, listProfiles } from '../commands';
import { mockBackend } from '../mock-backend';
import { AppErrorException, normalizeError } from '../errors';

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

  it('rejects malformed command arguments before dispatch', async () => {
    mockBackend.reset();
    await expect(getProfile('not-a-uuid')).rejects.toMatchObject({ code: 'profile_invalid', operation: 'get_profile' });
    await expect(listProfiles()).resolves.toEqual([]);
  });

  it('parses serialized Tauri AppError and preserves optional fields', () => {
    const error = normalizeError(JSON.stringify({ code: 'profile_not_found', operation: 'get_profile', message: 'Missing', retryable: false }), 'transport');
    expect(error).toMatchObject({ code: 'profile_not_found', operation: 'get_profile', message: 'Missing', subjectId: undefined, details: undefined, retryable: false });
  });

  it('bounds object transport details', () => {
    const error = normalizeError({ message: 'x'.repeat(1000), code: 'not-valid' }, 'get_profile');
    expect(error.details?.length).toBeLessThanOrEqual(500);
    expect(normalizeError(new Error('transport detail'), 'get_profile').details).toBe('transport detail');
  });

  it('returns meaningful typed unknown-command errors', async () => {
    await expect(mockBackend.invoke('wat')).rejects.toMatchObject({ code: 'protocol_mismatch', operation: 'wat' });
  });
});
