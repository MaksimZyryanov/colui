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

  it('invokes inventory and definition commands with typed responses', async () => {
    mockBackend.reset();
    const id = '00000000-0000-0000-0000-000000000001';
    await createProfile({ displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', composeFiles: ['compose.yml'], environmentFiles: [] });
    expect((await getInventory()).hasSnapshot).toBe(false);
    const refreshed = await refreshInventory();
    expect(refreshed).toMatchObject({ generation: 1, hasSnapshot: true, freshness: 'fresh' });
    expect(refreshed.runtimeSessionId).toMatch(/^[0-9a-f-]{36}$/);
    expect(refreshed.daemonFingerprint).toMatchObject({ daemonId: 'mock' });
    expect((await getProjectDetails(id)).profile.profile.id).toBe(id);
    expect((await refreshProjectDefinition(id)).profileId).toBe(id);
    const lifecycle = await applyProject(id);
    expect(lifecycle.inventoryGeneration).toBe(lifecycle.inventory.generation);
    expect(lifecycle.inventory.generation).toBe(2);
    expect(lifecycle.inventory.projects[0]).toMatchObject({ composeProjectName: 'demo', containers: [{ state: 'running', serviceName: 'web' }] });
    expect(mockBackend.getInvocations().slice(1).map(({ command, args }) => ({ command, args }))).toEqual([
      { command: 'get_inventory', args: undefined }, { command: 'refresh_inventory', args: undefined },
      { command: 'get_project_details', args: { profileId: id } }, { command: 'refresh_project_definition', args: { profileId: id } }, { command: 'apply_project', args: { profileId: id } },
    ]);
  });

  it('decodes malformed inventory responses as protocol mismatch', async () => {
    mockBackend.reset();
    mockBackend.setResponseOverride('refresh_inventory', { generation: 1 });
    await expect(refreshInventory()).rejects.toMatchObject({ code: 'protocol_mismatch', operation: 'refresh_inventory' });
  });

  it('rejects invalid profile IDs for new profile-scoped commands before dispatch', async () => {
    mockBackend.reset();
    await expect(getProjectDetails('not-a-uuid')).rejects.toMatchObject({ code: 'profile_invalid', operation: 'get_project_details' });
    await expect(refreshProjectDefinition('not-a-uuid')).rejects.toMatchObject({ code: 'profile_invalid', operation: 'refresh_project_definition' });
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
