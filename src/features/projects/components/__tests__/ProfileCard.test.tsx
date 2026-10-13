// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider, type QueryObserverResult } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ProfileCard } from '../ProfileCard';
import { mockBackend } from '../../../../ipc/mock-backend';
import { projectKeys } from '../../query-keys';
import { runtimeKeys } from '../../../runtime/query-keys';
import type { ProjectDetailsResponse, RuntimeInventory } from '../../../../ipc/types';
import { projectStatusFromInventory } from '../../hooks/useProjectStatus';
import { getDiagnostics } from '../../../../ipc/commands';
import { diagnosticsKeys } from '../../../diagnostics/query-keys';
import { useDiagnostics } from '../../../diagnostics/hooks';

const sessionId = '00000000-0000-0000-0000-000000000099';
const profile = { id: '00000000-0000-0000-0000-000000000001', revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp/demo', registrationOrigin: 'manual' as const };
const container = (id: string, name: string) => ({ id, name, image: 'alpine', state: 'running' as const, statusText: 'Up', serviceName: name, publishedPorts: [] });
const containers = [container('z', 'demo-zeta-1'), container('a', 'demo-alpha-1')];
const observation: RuntimeInventory = {
  generation: 4, hasSnapshot: true, runtimeSessionId: sessionId,
  daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'linux', architecture: 'aarch64' },
  observedAt: '2026-09-03T00:00:00Z', lastSuccessfulObservedAt: '2026-09-03T00:00:00Z', freshness: 'fresh', error: null,
  containers, standaloneContainers: [],
  projects: [{ composeProjectName: 'demo', workingDirectory: '/tmp/demo', configFiles: ['/tmp/demo/compose.yml'], containers }],
  composeObservationGroups: [{ composeProjectName: 'demo', workingDirectory: '/tmp/demo', configFiles: ['/tmp/demo/compose.yml'], containerIds: ['z', 'a'] }],
};
const details: ProjectDetailsResponse = {
  profile: { profile, composeFiles: ['compose.yml'], environmentFiles: [] },
  definition: { profileId: profile.id, state: 'valid', definitionRevision: '1', services: [{ name: 'alpha', declaredPorts: [] }], issues: [], error: null },
  runtime: { presence: 'present', activity: 'all-running', containerCount: 2, runningContainerCount: 2, observedAt: observation.observedAt },
  runtimeIssues: [], profileAssociationUnique: true,
};
let client: QueryClient;
function renderCard(inventory = observation) {
  const sharedInventory = { data: inventory, isLoading: false, isError: false, error: null } as QueryObserverResult<RuntimeInventory>;
  return render(<QueryClientProvider client={client}><ProfileCard profile={profile} inventory={sharedInventory} /></QueryClientProvider>);
}
beforeEach(() => {
  mockBackend.reset();
  client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
  client.setQueryData(runtimeKeys.state(), { state: 'ready', context: { sessionId, endpoint: 'mock://runtime', daemonFingerprint: observation.daemonFingerprint, connectedAt: observation.observedAt } });
  client.setQueryData(projectKeys.definition(profile.id), details);
});
afterEach(() => { cleanup(); client.clear(); vi.useRealTimers(); });

describe('registered project observations', () => {
  it('keeps nested containers in name order when Docker returns a different order', async () => {
    const user = userEvent.setup();
    const view = renderCard();
    await user.click(screen.getByRole('button', { name: 'Expand Demo' }));
    const names = () => within(view.container.querySelector('.resource-children') as HTMLElement).getAllByRole('heading', { level: 3 }).map(heading => heading.textContent);
    expect(names()).toEqual(['demo-alpha-1', 'demo-zeta-1']);
    const reordered = { ...observation, containers: [...containers].reverse(), projects: [{ ...observation.projects[0], containers: [...containers].reverse() }] };
    view.rerender(<QueryClientProvider client={client}><ProfileCard profile={profile} inventory={{ data: reordered, isLoading: false, isError: false, error: null } as unknown as QueryObserverResult<RuntimeInventory>} /></QueryClientProvider>);
    expect(names()).toEqual(['demo-alpha-1', 'demo-zeta-1']);
  });

  it('does not attach a same-name stack from another directory or Compose file set', async () => {
    const user = userEvent.setup();
    renderCard({ ...observation, projects: [{ ...observation.projects[0], workingDirectory: '/tmp/other' }], composeObservationGroups: [{ ...observation.composeObservationGroups[0], workingDirectory: '/tmp/other' }] });
    await user.click(screen.getByRole('button', { name: 'Expand Demo' }));
    expect(screen.queryByRole('heading', { name: 'demo-alpha-1' })).not.toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'demo-zeta-1' })).not.toBeInTheDocument();
    expect(screen.getByLabelText('Project status')).not.toHaveTextContent('Running');
  });

  it('retains observed containers but disables actions as soon as the runtime session disconnects', async () => {
    const user = userEvent.setup();
    renderCard();
    await user.click(screen.getByRole('button', { name: 'Expand Demo' }));
    expect(screen.getByRole('button', { name: 'Stop demo-alpha-1' })).toBeEnabled();
    await act(async () => { client.setQueryData(runtimeKeys.state(), { state: 'disconnected' }); });
    await waitFor(() => expect(screen.getByLabelText('Project status')).toHaveTextContent('Runtime unavailable'));
    expect(screen.getByRole('heading', { name: 'demo-alpha-1' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Stop demo-alpha-1' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Restart demo-zeta-1' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'View logs for demo-alpha-1' })).toBeDisabled();
  });

  it('keeps Compose controls during a transient definition lock conflict', async () => {
    renderCard();
    expect(screen.getByRole('button', { name: 'Stop' })).toBeEnabled();
    await act(async () => { client.setQueryData(projectKeys.definition(profile.id), { ...details, definition: { ...details.definition, state: 'stale', error: { code: 'operation_conflict', operation: 'get_project_details', subject: null, message: 'Another operation is running', details: null, retryable: true } } }); });
    await waitFor(() => expect(screen.getByRole('button', { name: 'Stop' })).toBeInTheDocument());
    expect(screen.queryByRole('button', { name: 'Start Demo' })).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Loading project status')).not.toBeInTheDocument();
  });

  it('disables stale observations from a different ready session', async () => {
    const user = userEvent.setup();
    client.setQueryData(runtimeKeys.state(), { state: 'ready', context: { sessionId: '00000000-0000-0000-0000-000000000098', endpoint: 'mock://runtime', daemonFingerprint: observation.daemonFingerprint, connectedAt: observation.observedAt } });
    renderCard();
    await user.click(screen.getByRole('button', { name: 'Expand Demo' }));
    expect(screen.getByRole('button', { name: 'Stop demo-alpha-1' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Stop' })).toBeDisabled();
  });

  it('does not enable Apply using definition details from an older profile revision', async () => {
    const user = userEvent.setup();
    const shared = { data: observation, isLoading: false, isError: false, error: null } as QueryObserverResult<RuntimeInventory>;
    render(<QueryClientProvider client={client}><ProfileCard profile={{ ...profile, revision: 2 }} inventory={shared} /></QueryClientProvider>);
    await user.click(screen.getByRole('button', { name: 'More actions' }));
    expect(screen.getByRole('menuitem', { name: 'Apply' })).toHaveAttribute('data-disabled');
  });

  it('keeps container controls when an invalid definition is retained during a temporary lock conflict', async () => {
    const user = userEvent.setup();
    client.setQueryData(projectKeys.definition(profile.id), { ...details, definition: { ...details.definition, state: 'invalid', error: { code: 'definition_failed', operation: 'get_project_details', subject: null, message: 'Invalid Compose definition', details: null, retryable: false } } });
    renderCard();
    expect(screen.getByRole('button', { name: 'Restart Demo' })).toBeEnabled();
    await user.click(screen.getByRole('button', { name: 'Show status details' }));
    await act(async () => { client.setQueryData(projectKeys.definition(profile.id), { ...details, definition: { ...details.definition, state: 'stale', error: { code: 'operation_conflict', operation: 'get_project_details', subject: null, message: 'Another operation is running', details: null, retryable: true } } }); });
    expect(await screen.findByText('Another operation is running')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Restart Demo' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Restart' })).not.toBeInTheDocument();
  });

  it('distinguishes a ready runtime without containers from an unavailable runtime', async () => {
    renderCard({ ...observation, containers: [], projects: [], composeObservationGroups: [] });
    expect(screen.getByLabelText('Project status')).toHaveTextContent('No containers');
  });

  it('does not equate unknown association metadata with an empty stack', () => {
    expect(projectStatusFromInventory(profile, observation).runtime.presence).toBe('unavailable');
  });

  it('uses registered profile metadata while Compose definition is still loading', async () => {
    const user = userEvent.setup();
    client.removeQueries({ queryKey: projectKeys.definition(profile.id) });
    client.setQueryData(projectKeys.detail(profile.id), details.profile);
    mockBackend.setResponseOverride('get_project_details', new Promise(() => {}));
    renderCard();
    await user.click(screen.getByRole('button', { name: 'Expand Demo' }));
    expect(screen.getByRole('heading', { name: 'demo-alpha-1' })).toBeVisible();
  });

  it('honors the backend guard for duplicate registered profile tuples', async () => {
    const user = userEvent.setup();
    client.setQueryData(projectKeys.definition(profile.id), { ...details, profileAssociationUnique: false, runtimeIssues: [{ field: 'ambiguous_runtime_association', message: 'runtime association is ambiguous' }] });
    renderCard();
    await user.click(screen.getByRole('button', { name: 'Expand Demo' }));
    expect(screen.queryByRole('heading', { name: 'demo-alpha-1' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Stop' })).toBeDisabled();
    expect(screen.getByText('Container association unavailable.')).toBeVisible();
  });

  it('recovers container association when a new inventory removes an observed name conflict', async () => {
    const user = userEvent.setup();
    client.setQueryData(projectKeys.definition(profile.id), { ...details, runtimeIssues: [{ field: 'ambiguous_runtime_association', message: 'runtime association is ambiguous' }] });
    const conflicting = { ...observation, composeObservationGroups: [...observation.composeObservationGroups, { ...observation.composeObservationGroups[0], workingDirectory: '/tmp/other' }] };
    const view = renderCard(conflicting);
    await user.click(screen.getByRole('button', { name: 'Expand Demo' }));
    expect(screen.queryByRole('heading', { name: 'demo-alpha-1' })).not.toBeInTheDocument();
    expect(screen.getByLabelText('Project status')).toHaveTextContent('Runtime unavailable');
    view.rerender(<QueryClientProvider client={client}><ProfileCard profile={profile} inventory={{ data: { ...observation, generation: 5 }, isLoading: false, isError: false, error: null } as unknown as QueryObserverResult<RuntimeInventory>} /></QueryClientProvider>);
    expect(screen.getByRole('heading', { name: 'demo-alpha-1' })).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Show status details' }));
    expect(screen.queryByText('runtime association is ambiguous')).not.toBeInTheDocument();
  });

  it('repairs a transient unchecked definition when the diagnostics owner observes its completed load', async () => {
    const baseline = await getDiagnostics();
    vi.useFakeTimers();
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' });
    client.setQueryData(diagnosticsKeys.snapshot(), baseline);
    mockBackend.setResponseOverride('get_diagnostics', baseline);
    client.setQueryData(projectKeys.definition(profile.id), { ...details, definition: { ...details.definition, state: 'unchecked', services: [], error: { code: 'operation_conflict', operation: 'definition', subject: null, message: 'definition load already active', details: null, retryable: true } } });
    mockBackend.setResponseOverride('get_project_details', details);
    function ObservedCard() {
      useDiagnostics();
      return <ProfileCard profile={profile} inventory={{ data: observation, isLoading: false, isError: false, error: null } as QueryObserverResult<RuntimeInventory>} />;
    }
    render(<QueryClientProvider client={client}><ObservedCard /></QueryClientProvider>);
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(screen.getByRole('button', { name: 'Stop' })).toBeDisabled();
    mockBackend.setResponseOverride('get_diagnostics', { ...baseline, definitions: { generation: baseline.definitions.generation + 1, profiles: [{ profileId: profile.id, definition: details.definition, error: null }] } });
    await act(async () => { await vi.advanceTimersByTimeAsync(2100); });
    expect(screen.getByRole('button', { name: 'Stop' })).toBeEnabled();
    const reads = mockBackend.getInvocations().filter(invocation => invocation.command === 'get_project_details').length;
    await act(async () => { await vi.advanceTimersByTimeAsync(4000); });
    expect(mockBackend.getInvocations().filter(invocation => invocation.command === 'get_project_details')).toHaveLength(reads);
  });
});
