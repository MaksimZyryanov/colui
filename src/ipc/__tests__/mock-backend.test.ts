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
    await removeProfile({ profileId: created.id, expectedRevision: 2 });
    expect(await listProfiles()).toEqual([]);
  });

  it('injects malformed responses through decoder boundary', async () => {
    mockBackend.reset();
    mockBackend.setResponseOverride('list_profiles', { invalid: true });
    await expect(listProfiles()).rejects.toMatchObject({ code: 'protocol_mismatch' });
  });
});
