import { describe, expect, it } from 'vitest';
import { projectKeys } from '../query-keys';
import { projectStatusLabel } from '../components/StatusBadge';
import { validateProfileDraft } from '../components/ProfileFormDialog';
import { shouldPublishLifecycleInventory } from '../hooks/useLifecycleActions';
import { projectStatusFromInventory } from '../hooks/useProjectStatus';

describe('projects foundations', () => {
  it('keys every query by profile id', () => {
    expect(projectKeys.list()).toEqual(['projects', 'list']);
    expect(projectKeys.detail('id-1')).toEqual(['projects', 'detail', 'id-1']);
    expect(projectKeys.status('id-1')).toEqual(['projects', 'status', 'id-1']);
  });

  it('projects status precedence favors active operation and invalid definition', () => {
    const base = { runtime: { presence: 'present', activity: 'all-running', containerCount: 1, runningContainerCount: 1 }, definition: { state: 'valid', serviceCount: 1 }, issues: [] } as any;
    expect(projectStatusLabel({ ...base, operation: { kind: 'apply', phase: 'running', startedAt: '2026-09-02T00:00:00Z' } })).toBe('Applying');
    expect(projectStatusLabel({ ...base, operation: null, definition: { state: 'invalid' } })).toBe('Invalid definition');
  });

  it('validates required draft fields and duplicate ordered paths', () => {
    expect(validateProfileDraft({ displayName: '', composeProjectName: 'Bad Name', workingDirectory: '', composeFiles: ['a.yml', 'a.yml'], environmentFiles: [] })).toEqual(expect.objectContaining({ displayName: expect.any(String), composeProjectName: expect.any(String), workingDirectory: expect.any(String), composeFiles: expect.any(String) }));
  });

  it('only publishes inventory for successful lifecycle results', () => {
    const inventory = { generation: 0, hasSnapshot: false, observedAt: null, runtimeSessionId: null, daemonFingerprint: null, freshness: 'unavailable' as const, lastSuccessfulObservedAt: null, containers: [], projects: [], composeObservationGroups: [], standaloneContainers: [], error: null };
    expect(shouldPublishLifecycleInventory({ profileId: 'id-1', success: true, inventoryGeneration: 0, inventory })).toBe(true);
    expect(shouldPublishLifecycleInventory({ profileId: 'id-1', success: false, inventoryGeneration: 0, inventory })).toBe(false);
  });

  it('maps backend issue fields to form error associations', () => {
    expect(validateProfileDraft({ displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', composeFiles: ['compose.yml'], environmentFiles: [] })).toBeNull();
  });

  it('maps published inventory without matching project to absent runtime', () => {
    const profile = { id: '00000000-0000-0000-0000-000000000001', revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' as const };
    const inventory = { generation: 1, hasSnapshot: true, observedAt: '2026-09-03T00:00:00.000Z', runtimeSessionId: null, daemonFingerprint: null, freshness: 'fresh' as const, lastSuccessfulObservedAt: null, containers: [], projects: [], composeObservationGroups: [], standaloneContainers: [], error: null };
    expect(projectStatusFromInventory(profile, inventory).runtime).toMatchObject({ presence: 'absent', activity: null });
  });

});
