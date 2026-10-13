import type { RuntimeInventory } from '../../ipc/types';
import { Button } from '../../ui/components/Button';
import { useContainerActions, useContainerActionsPending } from './hooks';
import { Icon } from '../../ui/components/Icon';
import { AppErrorException } from '../../ipc/errors';

type Container = RuntimeInventory['containers'][number];

export function ContainerActions({ containers, runtimeSessionId, resourceName, disabled = false }: { containers: Container[]; runtimeSessionId: string; resourceName: string; disabled?: boolean }) {
  const action = useContainerActions();
  const sharedPending = useContainerActionsPending(containers.map(container => container.id), runtimeSessionId);
  const run = (kind: 'start' | 'stop' | 'restart') => {
    const targets = kind === 'start' ? containers.filter(container => container.state !== 'running') : kind === 'stop' ? containers.filter(container => container.state === 'running') : containers;
    action.mutate({ containerIds: targets.map(container => container.id), runtimeSessionId, action: kind });
  };
  const pending = disabled || action.isPending || sharedPending;
  return <>
    <Button iconOnly aria-label={`Start ${resourceName}`} title="Start" disabled={pending || containers.every(container => container.state === 'running')} onClick={() => void run('start')}><Icon name="play" /></Button>
    <Button iconOnly aria-label={`Stop ${resourceName}`} title="Stop" disabled={pending || containers.every(container => container.state !== 'running')} onClick={() => void run('stop')}><Icon name="square" /></Button>
    <Button iconOnly aria-label={`Restart ${resourceName}`} title="Restart" disabled={pending || containers.length === 0} onClick={() => void run('restart')}><Icon name="rotate-cw" /></Button>
    {action.isError ? <span role="alert">{action.error instanceof Error ? action.error.message : 'Container action failed'}{action.error instanceof AppErrorException && action.error.details ? `: ${action.error.details}` : ''}</span> : null}
  </>;
}
