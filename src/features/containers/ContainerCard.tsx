import type { RuntimeInventory } from '../../ipc/types';
import { Button } from '../../ui/components/Button';
import { Card } from '../../ui/components/Card';
import { useContainerAction } from './hooks';
import { ContainerLogsDialog } from './ContainerLogsDialog';
import { PortBindings } from './PortBindings';
import { AppErrorException } from '../../ipc/errors';

type Container = RuntimeInventory['standaloneContainers'][number];
export function ContainerCard({ container, runtimeSessionId }: { container: Container; runtimeSessionId: string }) {
  const action = useContainerAction();
  const pending = action.isPending && action.variables?.containerId === container.id;
  const run = (kind: 'start' | 'stop' | 'restart') => action.mutate({ containerId: container.id, runtimeSessionId, action: kind });
  const typedError = action.error instanceof AppErrorException ? action.error : null;
  const actionName = action.variables ? `${action.variables.action[0].toUpperCase()}${action.variables.action.slice(1)} ${container.name}` : 'Container action';
  const recovery = typedError?.code === 'operation_conflict' ? 'Another action is already running. Wait and retry.' : typedError?.code === 'runtime_unavailable' || typedError?.code === 'runtime_connection_failed' || typedError?.code === 'runtime_context_mismatch' || /session/i.test(`${typedError?.message ?? ''} ${typedError?.details ?? ''}`) ? 'Runtime session changed. Reconnect and retry.' : typedError?.code === 'container_operation_failed' && typedError.retryable ? 'Docker daemon rejected the action. Check runtime health and retry.' : 'Container disappeared or Docker rejected the action. Refresh inventory before retrying.';
  return <Card aria-labelledby={`container-${container.id}`}><header><div><h3 id={`container-${container.id}`}>{container.name}</h3><p>{container.image}</p></div><span className="status-chip">{container.state}</span></header>
    <PortBindings container={container} runtimeSessionId={runtimeSessionId} />
    <div className="card-actions"><Button disabled={pending || container.state === 'running'} onClick={() => run('start')}>Start {container.name}</Button><Button disabled={pending || container.state !== 'running'} onClick={() => run('stop')}>Stop {container.name}</Button><Button disabled={pending} onClick={() => run('restart')}>Restart {container.name}</Button><ContainerLogsDialog containerId={container.id} containerName={container.name} runtimeSessionId={runtimeSessionId} /></div>
    {action.isError ? <div role="alert" aria-label={`${actionName} failed`} className="ui-alert ui-alert-destructive"><strong>{typedError?.code ?? 'container_operation_failed'}</strong>: {typedError?.message ?? 'Container action failed'}<p>{recovery}</p>{typedError?.details ? <details><summary>Technical details</summary><p>{typedError.details}</p></details> : null}</div> : null}
  </Card>;
}
