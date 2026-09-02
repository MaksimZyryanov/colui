import { describe, expect, it } from 'vitest';
import { createProfile, listProfiles, removeProfile, updateProfile } from '../commands';
import { mockBackend } from '../mock-backend';

const draft = {
  displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp',
  composeFiles: ['compose.yml'], environmentFiles: [],
};

describe('browser mock backend', () => {
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
});
