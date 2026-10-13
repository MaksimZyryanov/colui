// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ProjectsView } from '../ProjectsView';
import { StatusDetails } from '../components/StatusDetails';
import { mockBackend } from '../../../ipc/mock-backend';
import { useInventory } from '../../runtime/hooks/useInventory';
import type { ReactNode } from 'react';
import { AppShell } from '../../../app/AppShell';
import { projectKeys } from '../query-keys';

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
function TestShell({ children }: { children: ReactNode }) { useInventory(); return children; }
const renderProjects = () => render(<QueryClientProvider client={client}><TestShell><ProjectsView /></TestShell></QueryClientProvider>);

describe('ProjectsView', () => {
  beforeEach(() => {
    mockBackend.reset(); client.clear();
    const values = new Map<string, string>();
    Object.defineProperty(window, 'localStorage', { configurable: true, value: { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => values.set(key, value), removeItem: (key: string) => values.delete(key) } });
  });
  afterEach(cleanup);

  it('sorts all resource kinds together and filters project and child container names', async () => {
    const session = '00000000-0000-0000-0000-000000000099';
    const child = { id: 'child', name: 'zulu-web', image: 'nginx', state: 'running', statusText: 'Up', serviceName: 'web', publishedPorts: [] };
    const standalone = { ...child, id: 'standalone', name: 'Alpha', serviceName: null };
    const profile = { id: '00000000-0000-0000-0000-000000000001', revision: 1, displayName: 'Zulu', composeProjectName: 'zulu', workingDirectory: '/tmp/zulu', registrationOrigin: 'manual' as const };
    mockBackend.setResponseOverride('get_runtime_state', { state: 'ready', context: { sessionId: session, endpoint: 'mock://runtime', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, connectedAt: '2026-09-02T00:00:00.000Z' } });
    mockBackend.setResponseOverride('list_profiles', [profile]);
    mockBackend.setResponseOverride('get_project_details', { runtimeIssues: [], profileAssociationUnique: true, profile: { profile, composeFiles: ['compose.yml'], environmentFiles: [] }, definition: { profileId: profile.id, definitionRevision: 'valid', loadedAt: '2026-09-02T00:00:00.000Z', state: 'valid', services: [], issues: [], error: null }, runtime: { presence: 'present', activity: 'all-running', containerCount: 1, runningContainerCount: 1, observedAt: '2026-09-02T00:00:00.000Z' } });
    mockBackend.setResponseOverride('get_inventory', { generation: 4, hasSnapshot: true, observedAt: '2026-09-02T00:00:00.000Z', runtimeSessionId: session, daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, freshness: 'fresh', lastSuccessfulObservedAt: '2026-09-02T00:00:00.000Z', containers: [child, standalone], projects: [{ composeProjectName: 'zulu', workingDirectory: '/tmp/zulu', configFiles: ['/tmp/zulu/compose.yml'], containers: [child] }], composeObservationGroups: [{ composeProjectName: 'zulu', workingDirectory: '/tmp/zulu', configFiles: ['/tmp/zulu/compose.yml'], containerIds: [child.id] }], standaloneContainers: [standalone], error: null });
    mockBackend.setResponseOverride('list_discovery_candidates', { candidates: [{ candidateId: 'a'.repeat(64), runtimeSessionId: session, inventoryGeneration: 4, composeProjectName: 'Middle', workingDirectory: '/tmp/middle', configFiles: ['compose.yml'], containerCount: 1, classification: 'new_unambiguous', conflicts: [], ignored: false }], autoRegistrationEnabled: false });
    const user = userEvent.setup();
    renderProjects();
    await screen.findByRole('button', { name: 'Register Middle' });
    const list = screen.getByRole('list', { name: 'Resources' });
    expect(within(list).getAllByRole('heading').map(heading => heading.textContent)).toEqual(['Alpha', 'Middle', 'Zulu']);
    expect(screen.queryByRole('heading', { name: 'Registered projects' })).not.toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Discovered projects' })).not.toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Other containers' })).not.toBeInTheDocument();
    const search = screen.getByRole('searchbox', { name: 'Search resources' });
    for (const [query, name] of [[' ALP ', 'Alpha'], ['midd', 'Middle'], ['zulu-web', 'Zulu']]) {
      await user.clear(search);
      await user.type(search, query);
      expect(within(list).getAllByRole('heading').map(heading => heading.textContent)).toEqual([name]);
    }
    const expand = screen.getByRole('button', { name: 'Expand Zulu' });
    expand.focus();
    await user.keyboard('{Enter}');
    expect(screen.getByText('zulu-web')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'View logs for zulu-web' }));
    expect(await screen.findByRole('dialog', { name: 'Logs: zulu-web' })).toBeVisible();
    await user.keyboard('{Escape}');
    expect(screen.getByRole('button', { name: 'View logs for zulu-web' })).toHaveFocus();
    await user.clear(search);
    await user.type(search, 'nothing-matches');
    expect(screen.getByText('No matching resources')).toBeVisible();
    expect(screen.queryByText('No projects yet')).not.toBeInTheDocument();
    await user.clear(search);
    expect(screen.getByText('zulu-web')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Collapse Zulu' }));
    expect(screen.queryByText('zulu-web')).not.toBeInTheDocument();
  });

  it.each(['/tmp/demo', '/tmp/demo/.'])('renders an already registered discovery observation only as its profile row (%s)', async workingDirectory => {
    const session = '00000000-0000-0000-0000-000000000099';
    const profile = { id: '00000000-0000-0000-0000-000000000001', revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory, registrationOrigin: 'manual' as const };
    mockBackend.setResponseOverride('get_runtime_state', { state: 'ready', context: { sessionId: session, endpoint: 'mock://runtime', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, connectedAt: '2026-09-02T00:00:00.000Z' } });
    mockBackend.setResponseOverride('list_profiles', [profile]);
    mockBackend.setResponseOverride('get_inventory', { generation: 4, hasSnapshot: true, observedAt: '2026-09-02T00:00:00.000Z', runtimeSessionId: session, daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, freshness: 'fresh', lastSuccessfulObservedAt: '2026-09-02T00:00:00.000Z', containers: [], projects: [], composeObservationGroups: [], standaloneContainers: [], error: null });
    mockBackend.setResponseOverride('list_discovery_candidates', { candidates: [{ candidateId: 'a'.repeat(64), runtimeSessionId: session, inventoryGeneration: 4, composeProjectName: 'demo', workingDirectory: '/tmp/demo', configFiles: ['/tmp/demo/compose.yml'], containerCount: 1, classification: 'already_registered', conflicts: [], ignored: false }], autoRegistrationEnabled: false });

    renderProjects();

    expect(await screen.findByRole('heading', { name: 'Demo' })).toBeVisible();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Expand Demo' }));
    expect(screen.getAllByRole('heading', { name: /demo/i })).toHaveLength(1);
    expect(screen.queryByText('Already registered')).not.toBeInTheDocument();
  });

  it.each(['manual', 'automatic'])('hands an expanded discovered row to its registered profile before background queries finish (%s)', async mode => {
    const session = '00000000-0000-0000-0000-000000000099';
    const profile = { id: '00000000-0000-0000-0000-000000000001', revision: 1, displayName: 'demo', composeProjectName: 'demo', workingDirectory: '/tmp/demo', registrationOrigin: 'discovered' as const };
    const candidate = { candidateId: 'a'.repeat(64), runtimeSessionId: session, inventoryGeneration: 4, composeProjectName: 'demo', workingDirectory: '/tmp/demo', configFiles: ['/tmp/demo/compose.yml'], containerCount: 1, classification: 'new_unambiguous' as const, conflicts: [], ignored: false };
    const container = { id: 'web', name: 'demo-web-1', image: 'nginx', state: 'running' as const, statusText: 'Up', serviceName: 'web', publishedPorts: [] };
    mockBackend.setResponseOverride('list_profiles', []);
    mockBackend.setResponseOverride('get_runtime_state', { state: 'ready', context: { sessionId: session, endpoint: 'mock://runtime', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, connectedAt: '2026-09-02T00:00:00.000Z' } });
    mockBackend.setResponseOverride('get_inventory', { generation: 4, hasSnapshot: true, observedAt: '2026-09-02T00:00:00.000Z', runtimeSessionId: session, daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, freshness: 'fresh', lastSuccessfulObservedAt: '2026-09-02T00:00:00.000Z', containers: [container], projects: [{ composeProjectName: 'demo', workingDirectory: '/tmp/demo', configFiles: candidate.configFiles, containers: [container] }], composeObservationGroups: [{ composeProjectName: 'demo', workingDirectory: '/tmp/demo', configFiles: candidate.configFiles, containerIds: [container.id] }], standaloneContainers: [], error: null });
    mockBackend.setResponseOverride('list_discovery_candidates', { candidates: [candidate], autoRegistrationEnabled: false });
    mockBackend.setResponseOverride('register_candidate', profile);
    mockBackend.setResponseOverride('auto_register_candidates', { profiles: [profile], errors: [] });
    mockBackend.setResponseOverride('get_project_details', new Promise(() => {}));
    const user = userEvent.setup();
    renderProjects();
    await user.click(await screen.findByRole('button', { name: 'Expand demo' }));
    expect(screen.getByText('demo-web-1')).toBeVisible();
    // Neither refetch is allowed to finish: the mutation response owns the handoff.
    mockBackend.setResponseOverride('list_profiles', new Promise(() => {}));
    mockBackend.setResponseOverride('list_discovery_candidates', new Promise(() => {}));
    await user.click(screen.getByRole('button', { name: mode === 'manual' ? 'Register demo' : 'Register eligible now' }));

    await waitFor(() => expect(screen.getByText('registered')).toBeVisible());
    expect(screen.getAllByRole('heading', { name: 'demo' })).toHaveLength(1);
    expect(screen.getByRole('button', { name: 'Collapse demo' })).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText('demo-web-1')).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Register demo' })).not.toBeInTheDocument();
    expect(screen.getByRole('status', { name: mode === 'manual' ? /Registration demo .* succeeded/ : 'Automatic registration succeeded' }).closest('.sr-only')).not.toBeNull();
    // A late pre-registration discovery snapshot must not duplicate the profile.
    await act(async () => { client.setQueriesData({ queryKey: ['discovery', 'list'] }, { candidates: [candidate], autoRegistrationEnabled: false }); });
    expect(screen.getAllByRole('heading', { name: 'demo' })).toHaveLength(1);
    // Removing the profile must make the observation eligible for display again.
    await act(async () => { client.setQueryData(projectKeys.list(), []); });
    expect(await screen.findByRole('button', { name: 'Register demo' })).toBeVisible();
  });

  it('does not discover or enable registration from a retained disconnected inventory session', async () => {
    mockBackend.setResponseOverride('get_inventory', { generation: 4, hasSnapshot: true, observedAt: '2026-09-02T00:00:00.000Z', runtimeSessionId: '00000000-0000-0000-0000-000000000099', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, freshness: 'stale', lastSuccessfulObservedAt: '2026-09-02T00:00:00.000Z', containers: [], projects: [], composeObservationGroups: [], standaloneContainers: [], error: null });
    renderProjects();
    await screen.findByText('Container observations are stale. Showing the last snapshot.');
    expect(screen.getByRole('switch', { name: 'Automatic registration' })).toBeDisabled();
    expect(mockBackend.getInvocations().filter(call => call.command === 'list_discovery_candidates')).toHaveLength(0);
  });

  it.each(['profiles-first', 'discovery-first'])('retains one expanded row when the automatic scheduler publishes caches in either order (%s)', async order => {
    const session = '00000000-0000-0000-0000-000000000099';
    const candidate = { candidateId: 'a'.repeat(64), runtimeSessionId: session, inventoryGeneration: 4, composeProjectName: 'scheduled-app', workingDirectory: '/tmp/scheduled', configFiles: ['/tmp/scheduled/compose.yml'], containerCount: 1, classification: 'new_unambiguous' as const, conflicts: [], ignored: false };
    const profile = { id: '00000000-0000-0000-0000-000000000001', revision: 1, displayName: 'scheduled-app', composeProjectName: 'scheduled-app', workingDirectory: '/tmp/scheduled', registrationOrigin: 'discovered' as const };
    mockBackend.setResponseOverride('list_profiles', []);
    mockBackend.setResponseOverride('get_runtime_state', { state: 'ready', context: { sessionId: session, endpoint: 'mock://runtime', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, connectedAt: '2026-09-02T00:00:00.000Z' } });
    mockBackend.setResponseOverride('get_inventory', { generation: 4, hasSnapshot: true, observedAt: '2026-09-02T00:00:00.000Z', runtimeSessionId: session, daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, freshness: 'fresh', lastSuccessfulObservedAt: '2026-09-02T00:00:00.000Z', containers: [], projects: [], composeObservationGroups: [], standaloneContainers: [], error: null });
    mockBackend.setResponseOverride('list_discovery_candidates', { candidates: [candidate], autoRegistrationEnabled: true });
    const user = userEvent.setup();
    renderProjects();
    await user.click(await screen.findByRole('button', { name: 'Expand scheduled-app' }));
    const publishProfiles = async () => { await act(async () => { client.setQueryData(projectKeys.list(), [profile]); }); };
    const publishDiscovery = async () => { await act(async () => { client.setQueriesData({ queryKey: ['discovery', 'list'] }, { candidates: [{ ...candidate, classification: 'already_registered' }], autoRegistrationEnabled: true }); }); };
    await (order === 'profiles-first' ? publishProfiles() : publishDiscovery());
    await waitFor(() => expect(screen.getAllByRole('heading', { name: 'scheduled-app' })).toHaveLength(1));
    expect(screen.getByRole('button', { name: 'Collapse scheduled-app' })).toBeVisible();
    await (order === 'profiles-first' ? publishDiscovery() : publishProfiles());
    await waitFor(() => expect(screen.getByText('registered')).toBeVisible());
    expect(screen.getAllByRole('heading', { name: 'scheduled-app' })).toHaveLength(1);
    expect(screen.getByRole('button', { name: 'Collapse scheduled-app' })).toHaveAttribute('aria-expanded', 'true');
  });

  it('waits for a ready runtime before loading project definitions', async () => {
    const profileId = '00000000-0000-0000-0000-000000000001';
    let resolveRuntime!: (value: unknown) => void;
    mockBackend.setResponseOverride('list_profiles', [{ id: profileId, revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp/demo', registrationOrigin: 'manual' }]);
    mockBackend.setResponseOverride('get_runtime_state', new Promise(resolve => { resolveRuntime = resolve; }));
    renderProjects();

    expect(await screen.findByRole('heading', { name: 'Demo' })).toBeVisible();
    expect(mockBackend.getInvocations().filter(call => call.command === 'get_project_details')).toHaveLength(0);

    resolveRuntime({ state: 'ready', context: { sessionId: '00000000-0000-0000-0000-000000000099', endpoint: 'mock://runtime', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, connectedAt: '2026-09-02T00:00:00.000Z' } });
    await waitFor(() => expect(mockBackend.getInvocations().filter(call => call.command === 'get_project_details')).toHaveLength(1));
    expect(screen.queryByText('Unable to load project definition')).not.toBeInTheDocument();
  });

  it('controls a project and its child containers when Compose definition cannot load', async () => {
    const user = userEvent.setup();
    const profileId = '00000000-0000-0000-0000-000000000001';
    const session = '00000000-0000-0000-0000-000000000099';
    const profile = { id: profileId, revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp/demo', registrationOrigin: 'discovered' as const };
    const container = { id: 'demo-web', name: 'demo-web-1', image: 'nginx', state: 'running' as const, statusText: 'Up', serviceName: 'web', publishedPorts: [] };
    const inventory = { generation: 4, hasSnapshot: true, observedAt: '2026-09-02T00:00:00.000Z', runtimeSessionId: session, daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, freshness: 'fresh' as const, lastSuccessfulObservedAt: '2026-09-02T00:00:00.000Z', containers: [container], projects: [{ composeProjectName: 'demo', workingDirectory: '/tmp/demo', configFiles: ['/tmp/demo/compose.yml'], containers: [container] }], composeObservationGroups: [{ composeProjectName: 'demo', workingDirectory: '/tmp/demo', configFiles: ['/tmp/demo/compose.yml'], containerIds: [container.id] }], standaloneContainers: [], error: null };
    mockBackend.setResponseOverride('list_profiles', [profile]);
    mockBackend.setResponseOverride('get_runtime_state', { state: 'ready', context: { sessionId: session, endpoint: 'mock://runtime', daemonFingerprint: inventory.daemonFingerprint, connectedAt: inventory.observedAt } });
    mockBackend.setResponseOverride('get_inventory', inventory);
    mockBackend.setResponseOverride('get_project_details', { runtimeIssues: [], profileAssociationUnique: true, profile: { profile, composeFiles: ['/tmp/demo/compose.yml'], environmentFiles: [] }, definition: { profileId, definitionRevision: null, loadedAt: null, state: 'unchecked', services: [], issues: [], error: { code: 'definition_failed', operation: 'definition', subject: { kind: 'profile', id: profileId }, message: 'Required variables are unavailable', details: null, retryable: false } }, runtime: { presence: 'present', activity: 'all-running', containerCount: 1, runningContainerCount: 1, observedAt: inventory.observedAt } });
    mockBackend.setResponseOverride('run_container_action', { containerId: container.id, action: 'stop', observation: 'confirmed_in_session', inventory });

    renderProjects();
    expect(await screen.findByRole('button', { name: 'Stop Demo' })).toBeEnabled();
    expect(screen.queryByText('Unable to load project definition')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Stop Demo' }));
    await waitFor(() => expect(mockBackend.getInvocations().find(call => call.command === 'run_container_action')?.args).toEqual({ request: { containerId: 'demo-web', runtimeSessionId: session, action: 'stop' } }));

    await user.click(screen.getByRole('button', { name: 'Expand Demo' }));
    expect(screen.getByRole('button', { name: 'Restart demo-web-1' })).toBeEnabled();
    await user.click(screen.getByRole('button', { name: 'Restart demo-web-1' }));
    await waitFor(() => expect(mockBackend.getInvocations().filter(call => call.command === 'run_container_action')).toHaveLength(2));
  });

  it('renders explicit empty state and opens Add Project form', async () => {
    const user = userEvent.setup();
    renderProjects();
    expect(await screen.findByText('No projects yet')).toBeVisible();
    await user.click(screen.getByRole('button', { name: /add project/i }));
    expect(screen.getByRole('dialog', { name: /add project/i })).toBeVisible();
  });

  it('does not claim an empty resource list when discovery failed', async () => {
    mockBackend.setResponseOverride('get_runtime_state', { state: 'ready', context: { sessionId: '00000000-0000-0000-0000-000000000099', endpoint: 'mock://runtime', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, connectedAt: '2026-09-02T00:00:00.000Z' } });
    mockBackend.setErrorOverride('list_discovery_candidates', new Error('discovery offline'));
    renderProjects();
    expect(await screen.findByText('Unable to load discovery candidates')).toBeVisible();
    expect(screen.queryByText('No projects yet')).not.toBeInTheDocument();
    expect(screen.getByText('Resources could not be fully loaded.')).toBeVisible();
  });

  it('provides working desktop and mobile Projects/Diagnostics navigation with one main landmark', async () => {
    const user = userEvent.setup();
    render(<QueryClientProvider client={client}><AppShell /></QueryClientProvider>);
    expect(await screen.findByRole('heading', { name: 'Projects' })).toBeVisible();
    expect(screen.getAllByRole('link', { name: 'Projects' })).toHaveLength(2);
    expect(screen.getAllByRole('link', { name: 'Diagnostics' })).toHaveLength(2);
    await user.click(screen.getAllByRole('link', { name: 'Diagnostics' })[0]);
    expect(await screen.findByRole('heading', { name: 'Diagnostics' })).toBeVisible();
    expect(screen.getAllByRole('main')).toHaveLength(1);
  });

  it('follows browser Back and Forward hash changes and removes its listener', async () => {
    location.hash = '#projects';
    const remove = vi.spyOn(window, 'removeEventListener');
    const { unmount } = render(<QueryClientProvider client={client}><AppShell /></QueryClientProvider>);
    location.hash = '#diagnostics';
    window.dispatchEvent(new HashChangeEvent('hashchange'));
    expect(await screen.findByRole('heading', { name: 'Diagnostics' })).toBeVisible();
    location.hash = '#projects';
    window.dispatchEvent(new HashChangeEvent('hashchange'));
    expect(await screen.findByRole('heading', { name: 'Projects' })).toBeVisible();
    unmount();
    expect(remove).toHaveBeenCalledWith('hashchange', expect.any(Function));
  });

  it('renders running as one label and expands independent details', async () => {
    const session = '00000000-0000-0000-0000-000000000099';
    const profile = { id: '00000000-0000-0000-0000-000000000001', revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' as const };
    const containers = ['one', 'two', 'three'].map((id, index) => ({ id, name: `demo-web-${index + 1}`, image: 'mock', state: 'running', statusText: 'Up', serviceName: 'web', publishedPorts: [] }));
    mockBackend.setResponseOverride('list_profiles', [profile]);
    mockBackend.setResponseOverride('get_runtime_state', { state: 'ready', context: { sessionId: session, endpoint: 'mock://runtime', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, connectedAt: '2026-09-02T00:00:00.000Z' } });
    mockBackend.setResponseOverride('get_project_details', { runtimeIssues: [], profileAssociationUnique: true, profile: { profile, composeFiles: ['compose.yml'], environmentFiles: [] }, definition: { profileId: profile.id, definitionRevision: 'valid', loadedAt: '2026-09-02T00:00:00.000Z', state: 'valid', services: [], issues: [], error: null }, runtime: { presence: 'present', activity: 'all-running', containerCount: 3, runningContainerCount: 3, observedAt: '2026-09-02T00:00:00.000Z' } });
    mockBackend.setResponseOverride('get_inventory', { generation: 1, hasSnapshot: true, observedAt: '2026-09-02T00:00:00.000Z', runtimeSessionId: session, daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, freshness: 'fresh', lastSuccessfulObservedAt: '2026-09-02T00:00:00.000Z', containers, projects: [{ composeProjectName: 'demo', workingDirectory: '/tmp', configFiles: ['/tmp/compose.yml'], containers }], composeObservationGroups: [{ composeProjectName: 'demo', workingDirectory: '/tmp', configFiles: ['/tmp/compose.yml'], containerIds: containers.map(container => container.id) }], standaloneContainers: [], error: null });
    const user = userEvent.setup();
    renderProjects();
    expect(await screen.findByText('Running')).toBeVisible();
    expect(screen.queryByText(/runtime:/i)).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /show status details/i }));
    expect(screen.getByText(/3\/3 running/i)).toBeVisible();
  });

  it('owns one inventory poll and visibility listener for multiple profiles', async () => {
    mockBackend.setResponseOverride('list_profiles', [
      { id: '00000000-0000-0000-0000-000000000001', revision: 1, displayName: 'One', composeProjectName: 'one', workingDirectory: '/tmp', registrationOrigin: 'manual' },
      { id: '00000000-0000-0000-0000-000000000002', revision: 1, displayName: 'Two', composeProjectName: 'two', workingDirectory: '/tmp', registrationOrigin: 'manual' },
    ]);
    const addEventListener = vi.spyOn(document, 'addEventListener');
    renderProjects();
    await waitFor(() => expect(screen.getByText('One')).toBeVisible());
    expect(mockBackend.getInvocations().filter(invocation => invocation.command === 'get_inventory')).toHaveLength(1);
    expect(addEventListener.mock.calls.filter(([type]) => type === 'visibilitychange')).toHaveLength(1);
  });

  it('keeps valid profile visible beside invalid definition in same projection', async () => {
    const acceptanceClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
    const invalidId = '00000000-0000-0000-0000-000000000001';
    const validId = '00000000-0000-0000-0000-000000000002';
    const profiles = [
      { id: invalidId, revision: 1, displayName: 'Invalid Project', composeProjectName: 'invalid', workingDirectory: '/tmp/invalid', registrationOrigin: 'manual' as const },
      { id: validId, revision: 1, displayName: 'Valid Project', composeProjectName: 'valid', workingDirectory: '/tmp/valid', registrationOrigin: 'manual' as const },
    ];
    const containers = profiles.map(profile => ({ id: `${profile.composeProjectName}-web`, name: `${profile.composeProjectName}-web-1`, image: 'nginx', state: 'running', statusText: 'Up', serviceName: 'web', publishedPorts: [] }));
    mockBackend.setResponseOverride('list_profiles', profiles);
    mockBackend.setResponseOverride('get_runtime_state', { state: 'ready', context: { sessionId: '00000000-0000-0000-0000-000000000099', endpoint: 'mock://runtime', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, connectedAt: '2026-09-02T00:00:00.000Z' } });
    mockBackend.setResponseOverride('get_inventory', {
      generation: 1, hasSnapshot: true, observedAt: '2026-09-02T00:00:00.000Z', runtimeSessionId: '00000000-0000-0000-0000-000000000099', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, freshness: 'fresh', lastSuccessfulObservedAt: '2026-09-02T00:00:00.000Z', containers, composeObservationGroups: profiles.map((profile, index) => ({ composeProjectName: profile.composeProjectName, workingDirectory: profile.workingDirectory, configFiles: [`${profile.workingDirectory}/compose.yml`], containerIds: [containers[index].id] })), standaloneContainers: [], error: null,
      projects: profiles.map((profile, index) => ({ composeProjectName: profile.composeProjectName, workingDirectory: profile.workingDirectory, configFiles: [`${profile.workingDirectory}/compose.yml`], containers: [containers[index]] })),
    });
    for (const [profile, state] of [[profiles[0], 'invalid'], [profiles[1], 'valid']] as const) {
      acceptanceClient.setQueryData(projectKeys.definition(profile.id), {
        runtimeIssues: [], profileAssociationUnique: true,
        profile: { profile, composeFiles: ['compose.yml'], environmentFiles: [] },
        definition: { profileId: profile.id, definitionRevision: `${state}-revision`, loadedAt: '2026-09-02T00:00:00.000Z', state, services: state === 'valid' ? [{ name: 'web', image: 'nginx', buildContext: null, declaredPorts: [] }] : [], issues: state === 'invalid' ? [{ field: 'services.web', message: 'invalid service' }] : [], error: null },
        runtime: { presence: 'present', activity: 'all-running', containerCount: 1, runningContainerCount: 1, observedAt: '2026-09-02T00:00:00.000Z' },
      });
    }

    render(<QueryClientProvider client={acceptanceClient}><TestShell><ProjectsView /></TestShell></QueryClientProvider>);

    const invalidCard = (await screen.findByRole('heading', { name: 'Invalid Project' })).closest('section')!;
    const validCard = screen.getByRole('heading', { name: 'Valid Project' }).closest('section')!;
    expect(within(invalidCard).getByLabelText('Project status')).toHaveTextContent('Invalid definition');
    expect(within(validCard).getByLabelText('Project status')).toHaveTextContent('Running');
  });

  it('shows query errors without collapsing project route', async () => {
    mockBackend.setErrorOverride('list_profiles', new Error('offline'));
    renderProjects();
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('offline'));
    expect(screen.getByRole('main')).toBeVisible();
  });

  it('keeps card and retained definition visible beside separate definition error', async () => {
    const user = userEvent.setup();
    const profileId = '00000000-0000-0000-0000-000000000001';
    mockBackend.setResponseOverride('get_runtime_state', { state: 'ready', context: { sessionId: '00000000-0000-0000-0000-000000000099', endpoint: 'mock://runtime', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, connectedAt: '2026-09-02T00:00:00.000Z' } });
    mockBackend.setResponseOverride('list_profiles', [{ id: profileId, revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }]);
    mockBackend.setResponseOverride('get_project_details', { runtimeIssues: [], profileAssociationUnique: true, profile: { profile: { id: profileId, revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }, composeFiles: ['compose.yml'], environmentFiles: [] }, definition: { profileId, definitionRevision: 'retained', loadedAt: '2026-09-03T00:00:00Z', state: 'stale', services: [{ name: 'web', image: 'nginx', buildContext: null, declaredPorts: [] }], issues: [], error: { code: 'definition_failed', operation: 'definition', subject: { kind: 'profile', id: profileId }, message: 'compose config failed', details: null, retryable: false } }, runtime: { presence: 'unavailable', activity: null, containerCount: 0, runningContainerCount: 0, observedAt: null } });
    renderProjects();

    expect(await screen.findByText('Demo')).toBeVisible();
    await user.click(await screen.findByRole('button', { name: /show status details/i }));
    expect(await screen.findByRole('alert', { name: 'Status issues' })).toHaveTextContent('compose config failed');
    expect(screen.getByRole('button', { name: /edit demo/i })).toBeVisible();
    expect(client.getQueryData(['projects', 'definition', profileId])).toMatchObject({ definition: { services: [{ name: 'web' }], error: expect.objectContaining({ code: 'definition_failed' }) } });

    mockBackend.setResponseOverride('get_project_details', { runtimeIssues: [], profileAssociationUnique: true, profile: { profile: { id: profileId, revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }, composeFiles: ['compose.yml'], environmentFiles: [] }, definition: { profileId, definitionRevision: 'fresh', loadedAt: '2026-09-03T00:01:00Z', state: 'valid', services: [{ name: 'web', image: 'nginx', buildContext: null, declaredPorts: [] }], issues: [], error: null }, runtime: { presence: 'unavailable', activity: null, containerCount: 0, runningContainerCount: 0, observedAt: null } });
    await client.invalidateQueries({ queryKey: ['projects', 'definition', profileId] });
    await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument());
  });

  it('opens edit form after hydrating profile details', async () => {
    const profileId = '00000000-0000-0000-0000-000000000001';
    mockBackend.setResponseOverride('list_profiles', [{ id: profileId, revision: 2, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }]);
    mockBackend.setResponseOverride('get_profile', { profile: { id: profileId, revision: 2, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }, composeFiles: ['compose.yml'], environmentFiles: ['.env'] });
    const user = userEvent.setup();
    renderProjects();
    await user.click(await screen.findByRole('button', { name: /edit demo/i }));
    expect(await screen.findByRole('dialog', { name: /edit project/i })).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Next' }));
    expect(screen.getByDisplayValue('.env')).toBeVisible();
  });

  it('saves valid create drafts offline through accessible action', async () => {
    const user = userEvent.setup();
    renderProjects();
    await user.click(await screen.findByRole('button', { name: /add project/i }));
    await user.type(screen.getByLabelText('Display name'), 'Offline Project');
    await user.type(screen.getByLabelText('Compose name'), 'offline');
    await user.type(screen.getByLabelText('Working directory'), '/tmp/offline');
    await user.click(screen.getByRole('button', { name: /save offline/i }));

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(JSON.parse(window.localStorage.getItem('project-draft:new')!)).toMatchObject({ displayName: 'Offline Project', composeProjectName: 'offline' });
  });

  it('saves valid edit drafts offline under profile id', async () => {
    const profileId = '00000000-0000-0000-0000-000000000001';
    mockBackend.setResponseOverride('list_profiles', [{ id: profileId, revision: 2, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }]);
    mockBackend.setResponseOverride('get_profile', { profile: { id: profileId, revision: 2, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }, composeFiles: ['compose.yml'], environmentFiles: [] });
    const user = userEvent.setup();
    renderProjects();
    await user.click(await screen.findByRole('button', { name: /edit demo/i }));
    await user.click(screen.getByRole('button', { name: /save offline/i }));

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(JSON.parse(window.localStorage.getItem(`project-draft:${profileId}`)!)).toMatchObject({ displayName: 'Demo', composeProjectName: 'demo' });
  });

  it('keeps offline form open and renders validation errors', async () => {
    const user = userEvent.setup();
    renderProjects();
    await user.click(await screen.findByRole('button', { name: /add project/i }));
    await user.click(screen.getByRole('button', { name: /save offline/i }));

    expect(screen.getByRole('dialog')).toBeVisible();
    expect(screen.getByText('Display name is required')).toBeVisible();
  });

  it('keeps initial loading status in the row without adding a loading footer', async () => {
    mockBackend.setResponseOverride('list_profiles', [{ id: '00000000-0000-0000-0000-000000000001', revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }]);
    mockBackend.setResponseOverride('get_runtime_state', { state: 'ready', context: { sessionId: '00000000-0000-0000-0000-000000000099', endpoint: 'mock://runtime', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, connectedAt: '2026-09-02T00:00:00.000Z' } });
    mockBackend.setResponseOverride('get_project_details', new Promise(() => {}));
    mockBackend.setResponseOverride('get_inventory', new Promise(() => {}));
    renderProjects();
    await waitFor(() => expect(screen.getByLabelText('Project status')).toHaveTextContent('Loading status'));
    expect(screen.queryByRole('status', { name: /loading project status/i })).not.toBeInTheDocument();
  });

  it('keeps user edits while fresh profile details replace stale hydration', async () => {
    const profileId = '00000000-0000-0000-0000-000000000001';
    mockBackend.setResponseOverride('list_profiles', [{ id: profileId, revision: 2, displayName: 'Cached', composeProjectName: 'cached', workingDirectory: '/cached', registrationOrigin: 'manual' }]);
    mockBackend.setResponseOverride('get_profile', { profile: { id: profileId, revision: 3, displayName: 'Fresh', composeProjectName: 'fresh', workingDirectory: '/fresh', registrationOrigin: 'manual' }, composeFiles: ['fresh.yml'], environmentFiles: [] });
    client.setQueryData(['projects', 'detail', profileId], { profile: { id: profileId, revision: 2, displayName: 'Cached', composeProjectName: 'cached', workingDirectory: '/cached', registrationOrigin: 'manual' }, composeFiles: ['cached.yml'], environmentFiles: [] });
    const user = userEvent.setup();
    renderProjects();
    await user.click(await screen.findByRole('button', { name: /edit cached/i }));
    const displayName = await screen.findByDisplayValue('Fresh');
    await user.clear(displayName);
    await user.type(displayName, 'User edit');
    await user.click(screen.getByRole('button', { name: 'Next' }));
    expect(screen.getByDisplayValue('fresh.yml')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Back' }));
    expect(screen.getByDisplayValue('User edit')).toBeVisible();
  });

  it('preserves edits made while deferred profile details refresh', async () => {
    const profileId = '00000000-0000-0000-0000-000000000001';
    let resolveDetails!: (details: unknown) => void;
    mockBackend.setResponseOverride('list_profiles', [{ id: profileId, revision: 2, displayName: 'Cached', composeProjectName: 'cached', workingDirectory: '/cached', registrationOrigin: 'manual' }]);
    mockBackend.setResponseOverride('get_profile', new Promise(resolve => { resolveDetails = resolve; }));
    client.setQueryData(['projects', 'detail', profileId], { profile: { id: profileId, revision: 2, displayName: 'Cached', composeProjectName: 'cached', workingDirectory: '/cached', registrationOrigin: 'manual' }, composeFiles: ['cached.yml'], environmentFiles: ['cached.env'] });
    const user = userEvent.setup();
    renderProjects();

    await user.click(await screen.findByRole('button', { name: /edit cached/i }));
    await waitFor(() => expect(mockBackend.getInvocations().filter(invocation => invocation.command === 'get_profile')).toHaveLength(1));
    const displayName = await screen.findByDisplayValue('Cached');
    await user.clear(displayName);
    await user.type(displayName, 'User edit');
    resolveDetails({ profile: { id: profileId, revision: 3, displayName: 'Fresh', composeProjectName: 'fresh', workingDirectory: '/fresh', registrationOrigin: 'manual' }, composeFiles: ['fresh.yml'], environmentFiles: ['fresh.env'] });

    await waitFor(() => {
      expect(screen.getByDisplayValue('User edit')).toBeVisible();
      expect(screen.getByDisplayValue('fresh')).toBeVisible();
      expect(screen.getByDisplayValue('/fresh')).toBeVisible();
    });
    await user.click(screen.getByRole('button', { name: 'Next' }));
    expect(screen.getByDisplayValue('fresh.yml')).toBeVisible();
    expect(screen.getByDisplayValue('fresh.env')).toBeVisible();
  });

  it('renders unknown backend issue fields in accessible form-level summary', async () => {
    const profileId = '00000000-0000-0000-0000-000000000001';
    mockBackend.setResponseOverride('list_profiles', [{ id: profileId, revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }]);
    mockBackend.setResponseOverride('get_profile', { profile: { id: profileId, revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }, composeFiles: ['compose.yml'], environmentFiles: [] });
    mockBackend.setResponseOverride('inspect_profile_draft', { valid: false, issues: [{ field: 'futureField', message: 'Future field is invalid' }, { field: 'futureField', message: 'Future field is invalid' }] });
    const user = userEvent.setup();
    renderProjects();
    await user.click(await screen.findByRole('button', { name: /edit demo/i }));
    await user.click(screen.getByRole('button', { name: 'Next' }));
    await user.click(screen.getByText('Save', { selector: 'button' }));
    expect(await screen.findByRole('list', { name: 'Profile form errors' })).toHaveTextContent('Future field is invalid');
    expect(screen.getAllByText('Future field is invalid')).toHaveLength(2);
  });

  it('renders duplicate status issues without duplicate React keys', () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    render(<StatusDetails status={{ profileId: '00000000-0000-0000-0000-000000000001', runtime: { presence: 'unavailable', activity: null, containerCount: 0, runningContainerCount: 0, observedAt: null }, definition: { state: 'unchecked', revision: null, serviceCount: null }, operation: null, issues: [{ message: 'Same issue' }, { message: 'Same issue' }] }} />);
    expect(screen.getAllByText('Same issue')).toHaveLength(2);
    expect(error).not.toHaveBeenCalledWith(expect.stringContaining('Each child in a list should have a unique'));
    error.mockRestore();
  });
});
