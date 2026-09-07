import type { RuntimeInventory } from '../../ipc/types';
import { Button } from '../../ui/components/Button';
import { ResourceRow, ResourceStatus } from '../projects/components/ResourceRow';
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
  const recovery = typedError?.code === 'operation_conflict' ? 'Another action is already running. Wait and retry.' : typedError?.code === 'runtime_context_mismatch' ? 'Align Docker CLI context with the Docker API endpoint, then retry.' : typedError?.code === 'runtime_unavailable' || typedError?.code === 'runtime_connection_failed' || /session/i.test(`${typedError?.message ?? ''} ${typedError?.details ?? ''}`) ? 'Runtime session changed. Reconnect and retry.' : typedError?.code === 'container_operation_failed' && typedError.retryable ? 'Docker daemon rejected the action. Check runtime health and retry.' : 'Container disappeared or Docker rejected the action. Refresh inventory before retrying.';
  return <ResourceRow name={container.name} kind="standalone" status={<ResourceStatus label={container.state} tone={container.state} />}
    actions={<><Button iconOnly aria-label={`Start ${container.name}`} title="Start" disabled={pending || container.state === 'running'} onClick={() => run('start')}>▷</Button><Button iconOnly aria-label={`Stop ${container.name}`} title="Stop" disabled={pending || container.state !== 'running'} onClick={() => run('stop')}>□</Button><Button iconOnly aria-label={`Restart ${container.name}`} title="Restart" disabled={pending} onClick={() => run('restart')}>↻</Button><ContainerLogsDialog containerId={container.id} containerName={container.name} runtimeSessionId={runtimeSessionId} compact /></>}
    footer={<>
    {container.publishedPorts.length ? <details className="resource-ports"><summary>Ports · {container.publishedPorts.length}</summary><PortBindings container={container} runtimeSessionId={runtimeSessionId} /></details> : null}
    {action.isError ? <div role="alert" aria-label={`${actionName} failed`} className="ui-alert ui-alert-destructive"><strong>{typedError?.code ?? 'container_operation_failed'}</strong>: {typedError?.message ?? 'Container action failed'}<p>{recovery}</p>{typedError?.details ? <details><summary>Technical details</summary><p>{typedError.details}</p></details> : null}</div> : null}
  </>} />;
}
