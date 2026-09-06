import type { DiagnosticsSnapshot, RuntimeState } from '../../ipc/types';
import { Alert } from '../../ui/components/Alert';
import * as RadixAlertDialog from '@radix-ui/react-alert-dialog';
import { useRef } from 'react';
import { Button } from '../../ui/components/Button';
import { Card } from '../../ui/components/Card';
import { useConnectRuntime, useDisconnectRuntime, useReconnectRuntime } from '../runtime/hooks/useRuntimeSession';
import { useDiagnostics, useRegistryRecovery } from './hooks';
import { OperationFeedback } from '../../ui/components/OperationFeedback';

const title = (value: string) => value.replace(/([A-Z])/g, ' $1').replace(/_/g, ' ').replace(/^./, letter => letter.toUpperCase());
const fingerprint = (value: NonNullable<DiagnosticsSnapshot['runtime']['apiFingerprint']>) => `${value.daemonId} · ${value.serverVersion} · ${value.osType}/${value.architecture}`;
function runtimeLabel(state: RuntimeState) { return title(state.state); }
function errorDetails(error: { code: string; message: string } | null | undefined) {
  return error ? <p role="alert"><code>{error.code}</code>: {error.message}</p> : null;
}
function subjectLabel(subject: { kind: string; id: string } | null | undefined) {
  return subject ? `${subject.kind}: ${subject.id}` : 'No subject';
}
function RecoveryDialog({ trigger, title: dialogTitle, description, confirmLabel, destructive = false, onConfirm }: { trigger: React.ReactNode; title: string; description: string; confirmLabel: string; destructive?: boolean; onConfirm: () => void }) {
  const cancel = useRef<HTMLButtonElement>(null);
  return <RadixAlertDialog.Root><RadixAlertDialog.Trigger asChild>{trigger}</RadixAlertDialog.Trigger><RadixAlertDialog.Portal><RadixAlertDialog.Overlay className="ui-overlay" /><RadixAlertDialog.Content className="ui-dialog" onOpenAutoFocus={event => { if (destructive) { event.preventDefault(); cancel.current?.focus(); } }}><RadixAlertDialog.Title>{dialogTitle}</RadixAlertDialog.Title><RadixAlertDialog.Description>{description}</RadixAlertDialog.Description><div className="ui-dialog-actions"><RadixAlertDialog.Cancel asChild><Button ref={cancel}>Cancel</Button></RadixAlertDialog.Cancel><RadixAlertDialog.Action asChild><Button destructive={destructive} onClick={onConfirm}>{confirmLabel}</Button></RadixAlertDialog.Action></div></RadixAlertDialog.Content></RadixAlertDialog.Portal></RadixAlertDialog.Root>;
}

export function DiagnosticsView() {
  const diagnostics = useDiagnostics({ owner: false });
  const connect = useConnectRuntime();
  const disconnect = useDisconnectRuntime();
  const reconnect = useReconnectRuntime();
  const recovery = useRegistryRecovery();
  const data = diagnostics.data;
  return <main className="page"><header className="page-heading"><div><p className="eyebrow">System health</p><h1>Diagnostics</h1><p>Runtime, inventory, registry, and recovery status.</p></div></header>
    {diagnostics.isLoading ? <div role="status" aria-label="Loading diagnostics">Loading diagnostics...</div> : diagnostics.isError || !data ? <Alert variant="destructive">Diagnostics unavailable. Registry-backed Projects remain available offline.</Alert> : <>
      <section aria-labelledby="runtime-heading"><h2 id="runtime-heading">Runtime</h2><Card><div className="diagnostic-title"><strong>{runtimeLabel(data.runtime.state)}</strong><span>{data.runtime.resolvedEndpoint ?? 'No endpoint connected'}</span></div>{data.runtime.state.state === 'contextMismatch' ? <Alert variant="destructive">Docker API and Compose CLI resolve different daemons. Compose operations are blocked until contexts match.</Alert> : null}<dl className="diagnostic-grid"><dt>API fingerprint</dt><dd>{data.runtime.apiFingerprint ? fingerprint(data.runtime.apiFingerprint) : 'Unavailable'}</dd><dt>CLI fingerprint</dt><dd>{data.runtime.cliFingerprint ? fingerprint(data.runtime.cliFingerprint) : 'Unavailable'}</dd><dt>Session</dt><dd>{data.runtime.sessionId ?? 'None'}</dd></dl><div className="card-actions"><Button disabled={connect.isPending} onClick={() => connect.mutate()}>Connect</Button><Button disabled={disconnect.isPending} onClick={() => disconnect.mutate()}>Disconnect</Button><Button disabled={reconnect.isPending} onClick={() => reconnect.mutate()}>Reconnect</Button></div><OperationFeedback action="Connect" error={connect.error} success={connect.isSuccess} /><OperationFeedback action="Disconnect" error={disconnect.error} success={disconnect.isSuccess} /><OperationFeedback action="Reconnect" error={reconnect.error} success={reconnect.isSuccess} /></Card></section>
      <section aria-labelledby="registry-heading"><h2 id="registry-heading">Registry</h2><Card><div className="diagnostic-title"><strong>{title(data.registry.health.state)}</strong><span>Revision {data.registry.revision ?? 'unavailable'}</span></div><dl className="diagnostic-grid"><dt>Registry</dt><dd><code>{data.registry.registryPath}</code></dd><dt>Backup</dt><dd><code>{data.registry.backupPath}</code> · {title(data.registry.backup.state)}</dd><dt>Identity</dt><dd>{data.registry.health.identity?.canonicalContentSha256 ?? 'Unavailable'}</dd></dl><div className="card-actions"><RecoveryDialog trigger={<Button>Create or replace backup</Button>} title="Create registry backup?" description={`Copy validated ${data.registry.registryPath} to ${data.registry.backupPath}.`} confirmLabel="Create backup" onConfirm={() => recovery.backup.mutate()} /><RecoveryDialog trigger={<Button destructive>Restore backup</Button>} title="Restore registry backup?" description={`Replace ${data.registry.registryPath} with validated ${data.registry.backupPath}. Current registry bytes will be preserved before replacement.`} confirmLabel="Restore backup" destructive onConfirm={() => recovery.restore.mutate()} /></div><OperationFeedback action="Backup" error={recovery.backup.error} success={recovery.backup.isSuccess} /><OperationFeedback action="Restore" error={recovery.restore.error} success={recovery.restore.isSuccess} /></Card></section>
      <section aria-labelledby="inventory-heading"><h2 id="inventory-heading">Inventory</h2><Card><p>Generation {data.inventory.generation} · {title(data.inventory.freshness)}</p><p>{data.inventory.observedAt ?? 'Never observed'}</p></Card></section>
      <section aria-labelledby="operations-heading"><h2 id="operations-heading">Active operations</h2><Card>{data.operations.active.length ? <ul>{data.operations.active.map(operation => <li key={`${operation.kind}-${operation.subjectId}`}>{operation.kind}: {operation.subjectId} ({operation.phase})</li>)}</ul> : <p>No active operations.</p>}</Card></section>
      <section aria-labelledby="import-heading"><h2 id="import-heading">Import</h2><Card><dl className="diagnostic-grid"><dt>Source</dt><dd><code>{data.import.sourcePath}</code></dd><dt>Imported profiles</dt><dd>{data.import.importedCount}</dd><dt>Source preserved</dt><dd>{data.import.sourcePreserved ? 'Yes' : 'No'}</dd></dl>{errorDetails(data.import.error)}</Card></section>
      <section aria-labelledby="definitions-heading"><h2 id="definitions-heading">Definitions</h2><Card><p>Generation {data.definitions.generation}</p>{data.definitions.profiles.length ? <ul>{data.definitions.profiles.map(profile => <li key={profile.profileId}><strong>{profile.profileId}</strong>: {profile.definition ? title(profile.definition.state) : 'Unchecked'}{errorDetails(profile.error)}</li>)}</ul> : <p>No retained definitions.</p>}</Card></section>
      <section aria-labelledby="journal-heading"><h2 id="journal-heading">Journal</h2><Card>{data.journal.entries.length ? <ol>{data.journal.entries.map(entry => <li key={entry.sequence}><time dateTime={entry.timestamp}>{entry.timestamp}</time> · {title(entry.kind)} · {subjectLabel(entry.subject)}{entry.errorCode ? <> · <code>{entry.errorCode}</code></> : null}<p>{entry.message}</p></li>)}</ol> : <p>No journal entries.</p>}</Card></section>
    </>}
  </main>;
}
