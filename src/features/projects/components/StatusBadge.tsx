import type { ProjectStatus } from '../../../ipc/types';
export function projectStatusLabel(status: ProjectStatus): string {
  if (status.operation?.phase === 'running' || status.operation?.phase === 'queued') {
    const labels = { apply: 'Applying', stop: 'Stopping', 'tear-down': 'Tearing down', restart: 'Restarting' };
    return labels[status.operation.kind];
  }
  if (status.runtime.presence !== 'present') return 'Runtime unavailable';
  if (status.definition.state === 'invalid') return 'Invalid definition';
  if (status.runtime.activity === 'all-running') return 'Running';
  if (status.runtime.activity === 'mixed') return 'Partially running';
  if (status.runtime.activity === 'none-running') return 'Stopped';
  return 'Unchecked';
}
export function StatusBadge({ status }: { status: ProjectStatus }) { return <strong aria-label="Project status">{projectStatusLabel(status)}</strong>; }
