import { describe, expect, it } from 'vitest';
import { applyProject, connectRuntime, createProfile, getProfile, getProjectStatus, getRuntimeState, inspectProfileDraft, listProfiles, removeProfile, restartProject, stopProject, tearDownProject, updateProfile, getInventory, refreshInventory, getProjectDetails, refreshProjectDefinition } from '../commands';
import { mockBackend } from '../mock-backend';
import { AppErrorException, normalizeError } from '../errors';

describe('typed commands', () => {
  it('rejects lifecycle arguments containing backend authority', async () => {
    await expect(applyProject({ profileId: 'id-1', workingDirectory: '/tmp' } as never)).rejects.toThrow();
  });

  it('accepts only profileId for lifecycle calls', async () => {
    mockBackend.reset();
    await createProfile({ displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', composeFiles: ['compose.yml'], environmentFiles: [] });
    const result = await applyProject('00000000-0000-0000-0000-000000000001');
    expect(result.profileId).toBe('00000000-0000-0000-0000-000000000001');
  });

  it('uses exact command names, argument shapes, and unit convention', async () => {
    mockBackend.reset();
    const id = '00000000-0000-0000-0000-000000000001';
    const value = { displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', composeFiles: ['compose.yml'], environmentFiles: [] };
    await listProfiles();
    await inspectProfileDraft(value);
    const created = await createProfile(value);
    await getProfile(created.id);
    await updateProfile({ profileId: created.id, expectedRevision: 1, patch: value });
    await expect(removeProfile({ profileId: created.id, expectedRevision: 2 })).resolves.toBeUndefined();
    await getRuntimeState();
    await connectRuntime();
    await expect(getProjectStatus(id)).rejects.toMatchObject({ operation: 'get_project_status' });
    for (const operation of [applyProject, stopProject, tearDownProject, restartProject]) await expect(operation(id)).rejects.toMatchObject({ operation: expect.any(String) });
    expect(mockBackend.getInvocations().map(({ command }) => command)).toEqual(['list_profiles', 'inspect_profile_draft', 'create_profile', 'get_profile', 'update_profile', 'remove_profile', 'get_runtime_state', 'connect_runtime', 'get_project_status', 'apply_project', 'stop_project', 'tear_down_project', 'restart_project']);
    expect(mockBackend.getInvocations().slice(8).map(({ args }) => args)).toEqual([{ profileId: id }, { profileId: id }, { profileId: id }, { profileId: id }, { profileId: id }]);
  });

  it('rejects malformed command arguments before dispatch', async () => {
    mockBackend.reset();
    await expect(getProfile('not-a-uuid')).rejects.toMatchObject({ code: 'profile_invalid', operation: 'get_profile' });
    await expect(listProfiles()).resolves.toEqual([]);
  });

  it('validates lifecycle profileId as UUID before dispatch', async () => {
    await expect(applyProject('not-a-uuid')).rejects.toMatchObject({ code: 'profile_invalid', operation: 'apply_project' });
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
