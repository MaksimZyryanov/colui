import type { RuntimeInventory } from '../../ipc/types';
import { Alert } from '../../ui/components/Alert';
import { Button } from '../../ui/components/Button';
import { Card } from '../../ui/components/Card';
import { useContainerAction } from './hooks';
import { ContainerLogsDialog } from './ContainerLogsDialog';
import { PortBindings } from './PortBindings';

type Container = RuntimeInventory['standaloneContainers'][number];
export function ContainerCard({ container, runtimeSessionId }: { container: Container; runtimeSessionId: string }) {
  const action = useContainerAction();
  const pending = action.isPending && action.variables?.containerId === container.id;
  const run = (kind: 'start' | 'stop' | 'restart') => action.mutate({ containerId: container.id, runtimeSessionId, action: kind });
  return <Card aria-labelledby={`container-${container.id}`}><header><div><h3 id={`container-${container.id}`}>{container.name}</h3><p>{container.image}</p></div><span className="status-chip">{container.state}</span></header>
    <PortBindings container={container} runtimeSessionId={runtimeSessionId} />
    <div className="card-actions"><Button disabled={pending || container.state === 'running'} onClick={() => run('start')}>Start {container.name}</Button><Button disabled={pending || container.state !== 'running'} onClick={() => run('stop')}>Stop {container.name}</Button><Button disabled={pending} onClick={() => run('restart')}>Restart {container.name}</Button><ContainerLogsDialog containerId={container.id} containerName={container.name} runtimeSessionId={runtimeSessionId} /></div>
    {action.isError ? <Alert variant="destructive">Container changed or disappeared. Refresh inventory and try again.</Alert> : null}
  </Card>;
}
