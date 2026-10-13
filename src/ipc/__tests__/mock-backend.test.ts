import { describe, expect, it, vi } from 'vitest';
import { createProfile, listProfiles, removeProfile, updateProfile } from '../commands';
import { mockBackend } from '../mock-backend';

const draft = {
  displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp',
  composeFiles: ['compose.yml'], environmentFiles: [],
};

describe('browser mock backend', () => {
  it('rejects flat arguments that the native Tauri command handler cannot deserialize', async () => {
    const profileId = '00000000-0000-0000-0000-000000000001';
    const flatCalls: Array<readonly [string, unknown]> = [
      ['inspect_profile_draft', draft], ['create_profile', draft],
      ['get_profile', { profileId }],
      ['update_profile', { profileId, expectedRevision: 1, patch: draft }],
      ['remove_profile', { profileId, expectedRevision: 1 }],
      ...['get_project_status', 'get_project_details', 'refresh_project_definition', 'apply_project', 'stop_project', 'tear_down_project', 'restart_project'].map(command => [command, { profileId }] as const),
    ];
    for (const [command, args] of flatCalls) {
      mockBackend.reset();
      await expect(mockBackend.invoke(command, args), command).rejects.toMatchObject({ code: 'profile_invalid', operation: command });
    }
  });

  it('supports CRUD with monotonic revisions', async () => {
    mockBackend.reset();
    const created = await createProfile(draft);
    expect(created.revision).toBe(1);
    const updated = await updateProfile({ profileId: created.id, expectedRevision: 1, patch: { ...draft, displayName: 'Updated' } });
    expect(updated.revision).toBe(2);
    await expect(removeProfile({ profileId: created.id, expectedRevision: 2 })).resolves.toBeUndefined();
    expect(await listProfiles()).toEqual([]);
  });

  it('injects malformed responses through decoder boundary', async () => {
    mockBackend.reset();
    mockBackend.setResponseOverride('list_profiles', { invalid: true });
    await expect(listProfiles()).rejects.toMatchObject({ code: 'protocol_mismatch' });
  });

  it('applies domain draft validation and allows duplicate compose names', async () => {
    mockBackend.reset();
    await expect(createProfile({ ...draft, displayName: '' })).rejects.toMatchObject({ code: 'profile_invalid' });
    await expect(createProfile({ ...draft, composeProjectName: 'Demo' })).rejects.toMatchObject({ code: 'profile_invalid' });
    await expect(createProfile({ ...draft, composeProjectName: ' demo' })).rejects.toMatchObject({ code: 'profile_invalid' });
    const created = await createProfile(draft);
    const duplicate = await createProfile({ ...draft, displayName: 'Other' });
    expect(duplicate.composeProjectName).toBe('demo');
    await expect(updateProfile({ profileId: created.id, expectedRevision: 1, patch: { ...draft, displayName: 'Changed', composeProjectName: 'demo' } })).resolves.toMatchObject({ revision: 2 });
  });

  it('records lifecycle operation in project status', async () => {
    mockBackend.reset();
    const created = await createProfile(draft);
    await import('../commands').then(({ applyProject }) => applyProject(created.id));
    const status = await import('../commands').then(({ getProjectStatus }) => getProjectStatus(created.id));
    expect(status.operation).toMatchObject({ kind: 'apply', phase: 'succeeded' });
  });

  it('starts generated IDs above persisted profiles after reload', async () => {
    const values = new Map<string, string>();
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      value: {
        getItem: (key: string) => values.get(key) ?? null,
        setItem: (key: string, value: string) => values.set(key, value),
        removeItem: (key: string) => values.delete(key),
      },
    });

    mockBackend.reset();
    const first = await mockBackend.invoke('create_profile', { draft }) as { id: string };

    vi.resetModules();
    const { mockBackend: reloadedBackend } = await import('../mock-backend');
    const second = await reloadedBackend.invoke('create_profile', { draft }) as { id: string };

    expect(first.id).toBe('00000000-0000-0000-0000-000000000001');
    expect(second.id).toBe('00000000-0000-0000-0000-000000000002');
    expect(second.id).not.toBe(first.id);
  });
});
