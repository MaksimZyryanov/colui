// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
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
    mockBackend.setResponseOverride('list_profiles', [{ id: '00000000-0000-0000-0000-000000000001', revision: 1, displayName: 'Zulu', composeProjectName: 'zulu', workingDirectory: '/tmp/zulu', registrationOrigin: 'manual' }]);
    mockBackend.setResponseOverride('get_inventory', { generation: 4, hasSnapshot: true, observedAt: '2026-09-02T00:00:00.000Z', runtimeSessionId: session, daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, freshness: 'fresh', lastSuccessfulObservedAt: '2026-09-02T00:00:00.000Z', containers: [child, standalone], projects: [{ composeProjectName: 'zulu', workingDirectory: '/tmp/zulu', configFiles: ['compose.yml'], containers: [child] }], composeObservationGroups: [], standaloneContainers: [standalone], error: null });
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
    const draft = { displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', composeFiles: ['compose.yml'], environmentFiles: [] };
    mockBackend.setResponseOverride('list_profiles', [{ id: '00000000-0000-0000-0000-000000000001', revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }]);
    mockBackend.setResponseOverride('get_inventory', { generation: 1, hasSnapshot: true, observedAt: '2026-09-02T00:00:00.000Z', runtimeSessionId: '00000000-0000-0000-0000-000000000099', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, freshness: 'fresh', lastSuccessfulObservedAt: '2026-09-02T00:00:00.000Z', containers: [{ id: 'one', name: 'demo-web-1', image: 'mock', state: 'running', statusText: 'Up', serviceName: 'web', publishedPorts: [] }, { id: 'two', name: 'demo-web-2', image: 'mock', state: 'running', statusText: 'Up', serviceName: 'web', publishedPorts: [] }, { id: 'three', name: 'demo-web-3', image: 'mock', state: 'running', statusText: 'Up', serviceName: 'web', publishedPorts: [] }], projects: [{ composeProjectName: 'demo', workingDirectory: '/tmp', configFiles: ['compose.yml'], containers: [{ id: 'one', name: 'demo-web-1', image: 'mock', state: 'running', statusText: 'Up', serviceName: 'web', publishedPorts: [] }, { id: 'two', name: 'demo-web-2', image: 'mock', state: 'running', statusText: 'Up', serviceName: 'web', publishedPorts: [] }, { id: 'three', name: 'demo-web-3', image: 'mock', state: 'running', statusText: 'Up', serviceName: 'web', publishedPorts: [] }]}], composeObservationGroups: [], standaloneContainers: [], error: null });
    void draft;
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
    mockBackend.setResponseOverride('list_profiles', profiles);
    mockBackend.setResponseOverride('get_inventory', {
      generation: 1, hasSnapshot: true, observedAt: '2026-09-02T00:00:00.000Z', runtimeSessionId: '00000000-0000-0000-0000-000000000099', daemonFingerprint: { daemonId: 'mock', serverVersion: '1', osType: 'test', architecture: 'test' }, freshness: 'fresh', lastSuccessfulObservedAt: '2026-09-02T00:00:00.000Z', containers: [], composeObservationGroups: [], standaloneContainers: [], error: null,
      projects: profiles.map(profile => ({ composeProjectName: profile.composeProjectName, workingDirectory: profile.workingDirectory, configFiles: ['compose.yml'], containers: [{ id: `${profile.composeProjectName}-web`, name: `${profile.composeProjectName}-web-1`, image: 'nginx', state: 'running', statusText: 'Up', serviceName: 'web', publishedPorts: [] }] })),
    });
    for (const [profile, state] of [[profiles[0], 'invalid'], [profiles[1], 'valid']] as const) {
      acceptanceClient.setQueryData(projectKeys.definition(profile.id), {
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
    const profileId = '00000000-0000-0000-0000-000000000001';
    mockBackend.setResponseOverride('list_profiles', [{ id: profileId, revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }]);
    mockBackend.setResponseOverride('get_project_details', { profile: { profile: { id: profileId, revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }, composeFiles: ['compose.yml'], environmentFiles: [] }, definition: { profileId, definitionRevision: 'retained', loadedAt: '2026-09-03T00:00:00Z', state: 'stale', services: [{ name: 'web', image: 'nginx', buildContext: null, declaredPorts: [] }], issues: [], error: { code: 'definition_failed', operation: 'definition', subject: { kind: 'profile', id: profileId }, message: 'compose config failed', details: null, retryable: false } }, runtime: { presence: 'unavailable', activity: null, containerCount: 0, runningContainerCount: 0, observedAt: null } });
    renderProjects();

    expect(await screen.findByText('Demo')).toBeVisible();
    expect(await screen.findByRole('alert')).toHaveTextContent('Unable to load project definition');
    expect(screen.getByRole('button', { name: /edit demo/i })).toBeVisible();
    expect(client.getQueryData(['projects', 'definition', profileId])).toMatchObject({ definition: { services: [{ name: 'web' }], error: expect.objectContaining({ code: 'definition_failed' }) } });

    mockBackend.setResponseOverride('get_project_details', { profile: { profile: { id: profileId, revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }, composeFiles: ['compose.yml'], environmentFiles: [] }, definition: { profileId, definitionRevision: 'fresh', loadedAt: '2026-09-03T00:01:00Z', state: 'valid', services: [{ name: 'web', image: 'nginx', buildContext: null, declaredPorts: [] }], issues: [], error: null }, runtime: { presence: 'unavailable', activity: null, containerCount: 0, runningContainerCount: 0, observedAt: null } });
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

  it('renders accessible status and profile loading skeletons', async () => {
    mockBackend.setResponseOverride('list_profiles', [{ id: '00000000-0000-0000-0000-000000000001', revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }]);
    mockBackend.setResponseOverride('get_project_status', new Promise(() => {}));
    renderProjects();
    await waitFor(() => expect(screen.getByRole('status', { name: /loading project status/i })).toBeVisible());
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
