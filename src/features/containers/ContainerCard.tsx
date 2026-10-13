import type { RuntimeInventory } from '../../ipc/types';
import { Button } from '../../ui/components/Button';
import { ResourceRow, ResourceStatus } from '../projects/components/ResourceRow';
import { useContainerAction, useContainerActionsPending } from './hooks';
import { ContainerLogsDialog } from './ContainerLogsDialog';
import { PortBindings } from './PortBindings';
import { AppErrorException } from '../../ipc/errors';
import { Icon } from '../../ui/components/Icon';
import { useRuntimeState } from '../runtime/hooks/useRuntimeSession';
import { useInventory } from '../runtime/hooks/useInventory';

type Container = RuntimeInventory['standaloneContainers'][number];
export function ContainerCard({ container, runtimeSessionId }: { container: Container; runtimeSessionId: string }) {
  const action = useContainerAction();
  const runtime = useRuntimeState();
  const observedInventory = useInventory({ owner: false });
  const snapshot = observedInventory.data;
  const currentObservation = !runtime.isError && !observedInventory.isError && runtime.data?.state === 'ready' && runtime.data.context.sessionId === runtimeSessionId && snapshot?.runtimeSessionId === runtimeSessionId && snapshot.freshness === 'fresh';
  const sharedPending = useContainerActionsPending([container.id], runtimeSessionId);
  const pending = sharedPending || (action.isPending && action.variables?.containerId === container.id);
  const disabled = !currentObservation || pending;
  const run = (kind: 'start' | 'stop' | 'restart') => action.mutate({ containerId: container.id, runtimeSessionId, action: kind });
  const typedError = action.error instanceof AppErrorException ? action.error : null;
  const actionName = action.variables ? `${action.variables.action[0].toUpperCase()}${action.variables.action.slice(1)} ${container.name}` : 'Container action';
  const recovery = typedError?.code === 'operation_conflict' ? 'Another action is already running. Wait and retry.' : typedError?.code === 'runtime_context_mismatch' ? 'Align Docker CLI context with the Docker API endpoint, then retry.' : typedError?.code === 'runtime_unavailable' || typedError?.code === 'runtime_connection_failed' || /session/i.test(`${typedError?.message ?? ''} ${typedError?.details ?? ''}`) ? 'Runtime session changed. Reconnect and retry.' : typedError?.code === 'container_operation_failed' && typedError.retryable ? 'Docker daemon rejected the action. Check runtime health and retry.' : 'Container disappeared or Docker rejected the action. Refresh inventory before retrying.';
  return <ResourceRow name={container.name} kind="standalone" status={<ResourceStatus label={currentObservation ? container.state : 'Stale runtime observation'} tone={currentObservation ? container.state : 'unknown'} />}
    actions={<><Button iconOnly aria-label={`Start ${container.name}`} title="Start" disabled={disabled || container.state === 'running'} onClick={() => run('start')}><Icon name="play" /></Button><Button iconOnly aria-label={`Stop ${container.name}`} title="Stop" disabled={disabled || container.state !== 'running'} onClick={() => run('stop')}><Icon name="square" /></Button><Button iconOnly aria-label={`Restart ${container.name}`} title="Restart" disabled={disabled} onClick={() => run('restart')}><Icon name="rotate-cw" /></Button><ContainerLogsDialog containerId={container.id} containerName={container.name} runtimeSessionId={runtimeSessionId} compact disabled={!currentObservation} /></>}
    footer={<>
    {container.publishedPorts.length ? <details className="resource-ports"><summary>Ports · {container.publishedPorts.length}</summary><PortBindings container={container} runtimeSessionId={runtimeSessionId} /></details> : null}
    {action.isError ? <div role="alert" aria-label={`${actionName} failed`} className="ui-alert ui-alert-destructive"><strong>{typedError?.code ?? 'container_operation_failed'}</strong>: {typedError?.message ?? 'Container action failed'}<p>{recovery}</p>{typedError?.details ? <details><summary>Technical details</summary><p>{typedError.details}</p></details> : null}</div> : null}
  </>} />;
}
