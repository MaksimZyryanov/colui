import { describe, expect, it } from 'vitest';
import { projectKeys } from '../query-keys';
import { projectStatusLabel } from '../components/StatusBadge';
import { validateProfileDraft } from '../components/ProfileFormDialog';

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
});
