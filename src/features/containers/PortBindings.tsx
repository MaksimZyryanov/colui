import type { RuntimeInventory } from '../../ipc/types';
import { Button } from '../../ui/components/Button';
import { useOpenContainerPort } from './hooks';
import { useState } from 'react';
import { OperationFeedback } from '../../ui/components/OperationFeedback';

type Container = RuntimeInventory['standaloneContainers'][number];
export function PortBindings({ container, runtimeSessionId }: { container: Container; runtimeSessionId: string }) {
  const open = useOpenContainerPort();
  const [copyResult, setCopyResult] = useState<{ error: unknown; success: boolean }>({ error: null, success: false });
  const copy = async (value: string) => { try { await navigator.clipboard.writeText(value); setCopyResult({ error: null, success: true }); } catch (error) { setCopyResult({ error, success: false }); } };
  if (!container.publishedPorts.length) return <p>No published ports</p>;
  return <><ul className="port-list" aria-label={`Published ports for ${container.name}`}>{container.publishedPorts.map((binding, index) => <li key={`${binding.action.copy}-${index}`}><code>{binding.action.copy}</code><span><Button aria-label={`Copy ${binding.action.copy}`} onClick={() => { void copy(binding.action.copy); }}>Copy</Button>{binding.action.url ? <Button aria-label={`Open ${binding.action.copy} in browser`} disabled={open.isPending} onClick={() => open.mutate({ containerId: container.id, runtimeSessionId, bindingIndex: index })}>Open</Button> : null}</span></li>)}</ul><OperationFeedback action="Copy port" error={copyResult.error} success={copyResult.success} /><OperationFeedback action="Open port" error={open.error} success={open.isSuccess} /></>;
}
