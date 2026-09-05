import type { RuntimeInventory } from '../../ipc/types';
import { Button } from '../../ui/components/Button';
import { useOpenContainerPort } from './hooks';

type Container = RuntimeInventory['standaloneContainers'][number];
export function PortBindings({ container, runtimeSessionId }: { container: Container; runtimeSessionId: string }) {
  const open = useOpenContainerPort();
  if (!container.publishedPorts.length) return <p>No published ports</p>;
  return <ul className="port-list" aria-label={`Published ports for ${container.name}`}>{container.publishedPorts.map((binding, index) => <li key={`${binding.action.copy}-${index}`}><code>{binding.action.copy}</code><span><Button aria-label={`Copy ${binding.action.copy}`} onClick={() => { void navigator.clipboard.writeText(binding.action.copy); }}>Copy</Button>{binding.action.url ? <Button aria-label={`Open ${binding.action.copy} in browser`} disabled={open.isPending} onClick={() => open.mutate({ containerId: container.id, runtimeSessionId, bindingIndex: index })}>Open</Button> : null}</span></li>)}</ul>;
}
