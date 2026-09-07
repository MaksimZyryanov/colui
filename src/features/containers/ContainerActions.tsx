import type { RuntimeInventory } from '../../ipc/types';
import { Button } from '../../ui/components/Button';
import { useContainerAction } from './hooks';

type Container = RuntimeInventory['containers'][number];

export function ContainerActions({ containers, runtimeSessionId, resourceName }: { containers: Container[]; runtimeSessionId: string; resourceName: string }) {
  const action = useContainerAction();
  const run = async (kind: 'start' | 'stop' | 'restart') => {
    const targets = kind === 'start' ? containers.filter(container => container.state !== 'running') : kind === 'stop' ? containers.filter(container => container.state === 'running') : containers;
    for (const container of targets) await action.mutateAsync({ containerId: container.id, runtimeSessionId, action: kind });
  };
  const pending = action.isPending;
  return <>
    <Button iconOnly aria-label={`Start ${resourceName}`} title="Start" disabled={pending || containers.every(container => container.state === 'running')} onClick={() => void run('start')}>▷</Button>
    <Button iconOnly aria-label={`Stop ${resourceName}`} title="Stop" disabled={pending || containers.every(container => container.state !== 'running')} onClick={() => void run('stop')}>□</Button>
    <Button iconOnly aria-label={`Restart ${resourceName}`} title="Restart" disabled={pending || containers.length === 0} onClick={() => void run('restart')}>↻</Button>
    {action.isError ? <span role="alert">{action.error instanceof Error ? action.error.message : 'Container action failed'}</span> : null}
  </>;
}
