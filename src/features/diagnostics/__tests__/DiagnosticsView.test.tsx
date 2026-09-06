// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mockBackend } from '../../../ipc/mock-backend';
import { DiagnosticsView } from '../DiagnosticsView';
import { useDiagnostics } from '../hooks';
import { diagnosticsKeys } from '../query-keys';
import { AppErrorException } from '../../../ipc/errors';

function TestShell() { useDiagnostics(); return <DiagnosticsView />; }

describe('DiagnosticsView', () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  beforeEach(() => { client.clear(); mockBackend.reset(); });
  afterEach(cleanup);

  it('keeps registry diagnostics and recovery available while runtime is offline', async () => {
    const user = userEvent.setup();
    client.setQueryData(diagnosticsKeys.snapshot(), { runtime: { state: { state: 'disconnected' }, resolvedEndpoint: null, apiFingerprint: null, cliFingerprint: null, sessionId: null, connectedAt: null }, registry: { registryPath: '/data/registry.json', backupPath: '/data/registry.json.bak', backup: { exists: true, modifiedAt: '2026-09-02T00:00:00.000Z', state: 'valid', error: null }, revision: 3, health: { state: 'corrupt', identity: null, error: null, lastOperationAt: null, lastFailureAt: null }, lockTimeoutMs: 500, lastRecoveryResult: null }, import: { sourcePath: '/legacy.json', importedCount: 0, sourcePreserved: true, error: null }, operations: { generation: 0, active: [] }, inventory: { generation: 0, hasSnapshot: false, observedAt: null, runtimeSessionId: null, daemonFingerprint: null, freshness: 'unavailable', lastSuccessfulObservedAt: null, containers: [], projects: [], composeObservationGroups: [], standaloneContainers: [], error: null }, definitions: { generation: 0, profiles: [] }, journal: { entries: [] } });
    render(<QueryClientProvider client={client}><DiagnosticsView /></QueryClientProvider>);
    expect(await screen.findByText('Disconnected')).toBeVisible();
    expect(screen.getByText('Corrupt')).toBeVisible();
    const restore = screen.getByRole('button', { name: /restore backup/i });
    await user.click(restore);
    expect(screen.getByRole('alertdialog')).toHaveTextContent('/data/registry.json.bak');
    expect(screen.getByRole('alertdialog')).toHaveTextContent('/data/registry.json');
    expect(screen.getByRole('button', { name: 'Cancel' })).toHaveFocus();
    await user.keyboard('{Escape}');
    expect(restore).toHaveFocus();
  });

  it('renders retained import, definition failures, and journal metadata accessibly offline', async () => {
    const base = await mockBackend.invoke('get_diagnostics') as Record<string, unknown>;
    client.setQueryData(diagnosticsKeys.snapshot(), {
      ...base,
      runtime: { state: { state: 'disconnected' }, resolvedEndpoint: null, apiFingerprint: null, cliFingerprint: null, sessionId: null, connectedAt: null },
      import: { sourcePath: '/legacy/projects.json', importedCount: 2, sourcePreserved: true, error: { code: 'registry_corrupt', operation: 'import_registry', subject: { kind: 'registry', id: 'legacy' }, message: 'Import failed', retryable: false } },
      definitions: { generation: 4, profiles: [{ profileId: '00000000-0000-0000-0000-000000000001', definition: { profileId: '00000000-0000-0000-0000-000000000001', definitionRevision: 'abc', loadedAt: '2026-09-05T10:00:00.000Z', state: 'stale', services: [], issues: [] }, error: { code: 'definition_failed', operation: 'definition', subject: { kind: 'profile', id: '00000000-0000-0000-0000-000000000001' }, message: 'Definition failed', retryable: true } }] },
      journal: { entries: [{ sequence: 7, timestamp: '2026-09-05T11:00:00.000Z', runtimeSessionId: null, kind: 'restore_failed', severity: 'error', subject: { kind: 'registry', id: 'canonical' }, errorCode: 'recovery_conflict', message: 'Registry restore failed' }] },
    });

    render(<QueryClientProvider client={client}><DiagnosticsView /></QueryClientProvider>);

    expect(await screen.findByRole('heading', { name: 'Import' })).toBeVisible();
    expect(screen.getByText('/legacy/projects.json')).toBeVisible();
    expect(screen.getByText('registry_corrupt')).toBeVisible();
    expect(screen.getByRole('heading', { name: 'Definitions' })).toBeVisible();
    expect(screen.getByText('00000000-0000-0000-0000-000000000001').closest('li')).toHaveTextContent('Stale');
    expect(screen.getByText('definition_failed')).toBeVisible();
    expect(screen.getByRole('heading', { name: 'Journal' })).toBeVisible();
    expect(screen.getByText('2026-09-05T11:00:00.000Z')).toBeVisible();
    expect(screen.getByText(/registry: canonical/i)).toBeVisible();
    expect(screen.getByText('recovery_conflict')).toBeVisible();
  });

  it('explains context mismatch with endpoint and both fingerprints', async () => {
    const base = await mockBackend.invoke('get_diagnostics') as Record<string, unknown>;
    client.setQueryData(diagnosticsKeys.snapshot(), { ...base, runtime: { state: { state: 'contextMismatch', details: { endpoint: 'unix:///api.sock', apiFingerprint: { daemonId: 'api-id', serverVersion: '27', osType: 'linux', architecture: 'arm64' }, cliFingerprint: { daemonId: 'cli-id', serverVersion: '26', osType: 'linux', architecture: 'amd64' } } }, resolvedEndpoint: 'unix:///api.sock', apiFingerprint: { daemonId: 'api-id', serverVersion: '27', osType: 'linux', architecture: 'arm64' }, cliFingerprint: { daemonId: 'cli-id', serverVersion: '26', osType: 'linux', architecture: 'amd64' }, sessionId: null, connectedAt: null } });
    render(<QueryClientProvider client={client}><DiagnosticsView /></QueryClientProvider>);
    expect(await screen.findByRole('alert')).toHaveTextContent('Compose operations are blocked');
    expect(screen.getByText('unix:///api.sock')).toBeVisible();
    expect(screen.getByText(/api-id.*27.*arm64/i)).toBeVisible();
    expect(screen.getByText(/cli-id.*26.*amd64/i)).toBeVisible();
  });

  it('offers connect, disconnect, and reconnect for runtime recovery', async () => {
    render(<QueryClientProvider client={client}><TestShell /></QueryClientProvider>);
    expect(await screen.findByRole('heading', { name: 'Runtime' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Connect' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Disconnect' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Reconnect' })).toBeVisible();
  });

  it.each([
    ['Connect', 'connect_runtime'],
    ['Disconnect', 'disconnect_runtime'],
    ['Reconnect', 'reconnect_runtime'],
  ])('shows typed %s failure and sanitized expandable details', async (label, command) => {
    const user = userEvent.setup();
    mockBackend.setErrorOverride(command, new AppErrorException({ code: 'runtime_connection_failed', operation: command, subject: null, message: 'Runtime unavailable', details: 'Socket access denied', retryable: true }));
    render(<QueryClientProvider client={client}><TestShell /></QueryClientProvider>);
    await user.click(await screen.findByRole('button', { name: label }));
    const alert = await screen.findByRole('alert', { name: `${label} failed` });
    expect(alert).toHaveTextContent('runtime_connection_failed');
    expect(alert).toHaveTextContent('Runtime unavailable');
    expect(screen.getByText('Socket access denied').closest('details')).not.toHaveAttribute('open');
    await user.click(screen.getByText('Technical details'));
    expect(screen.getByText('Socket access denied').closest('details')).toHaveAttribute('open');
    expect(alert).toHaveTextContent('Socket access denied');
  });

  it('announces runtime and registry recovery successes', async () => {
    const user = userEvent.setup();
    render(<QueryClientProvider client={client}><TestShell /></QueryClientProvider>);
    await user.click(await screen.findByRole('button', { name: 'Connect' }));
    expect(await screen.findByRole('status', { name: 'Connect succeeded' })).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Disconnect' }));
    expect(await screen.findByRole('status', { name: 'Disconnect succeeded' })).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Reconnect' }));
    expect(await screen.findByRole('status', { name: 'Reconnect succeeded' })).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Create or replace backup' }));
    await user.click(screen.getByRole('button', { name: 'Create backup' }));
    expect(await screen.findByRole('status', { name: 'Backup succeeded' })).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Restore backup' }));
    await user.click(screen.getByRole('button', { name: 'Restore backup' }));
    expect(await screen.findByRole('status', { name: 'Restore succeeded' })).toBeVisible();
  });

  it.each([
    ['Create or replace backup', 'Create backup', 'create_registry_backup', 'Backup'],
    ['Restore backup', 'Restore backup', 'restore_registry_backup', 'Restore'],
  ])('shows typed %s failure', async (trigger, confirm, command, action) => {
    const user = userEvent.setup();
    mockBackend.setErrorOverride(command, new AppErrorException({ code: 'recovery_conflict', operation: command, subject: { kind: 'registry', id: 'canonical' }, message: 'Recovery blocked', details: 'Active operation', retryable: true }));
    render(<QueryClientProvider client={client}><TestShell /></QueryClientProvider>);
    await user.click(await screen.findByRole('button', { name: trigger }));
    await user.click(screen.getByRole('button', { name: confirm }));
    expect(await screen.findByRole('alert', { name: `${action} failed` })).toHaveTextContent('recovery_conflict');
  });
});
