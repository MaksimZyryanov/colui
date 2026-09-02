// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ProjectsView } from '../ProjectsView';
import { StatusDetails } from '../components/StatusDetails';
import { mockBackend } from '../../../ipc/mock-backend';

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
const renderProjects = () => render(<QueryClientProvider client={client}><ProjectsView /></QueryClientProvider>);

describe('ProjectsView', () => {
  beforeEach(() => { mockBackend.reset(); client.clear(); });
  afterEach(cleanup);

  it('renders explicit empty state and opens Add Project form', async () => {
    const user = userEvent.setup();
    renderProjects();
    expect(await screen.findByText('No projects yet')).toBeVisible();
    await user.click(screen.getByRole('button', { name: /add project/i }));
    expect(screen.getByRole('dialog', { name: /add project/i })).toBeVisible();
  });

  it('renders running as one label and expands independent details', async () => {
    const draft = { displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', composeFiles: ['compose.yml'], environmentFiles: [] };
    mockBackend.setResponseOverride('list_profiles', [{ id: '00000000-0000-0000-0000-000000000001', revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }]);
    mockBackend.setResponseOverride('get_project_status', { profileId: '00000000-0000-0000-0000-000000000001', runtime: { presence: 'present', activity: 'all-running', containerCount: 3, runningContainerCount: 3, observedAt: '2026-09-02T00:00:00.000Z' }, definition: { state: 'valid', revision: '1', serviceCount: 3 }, operation: null, issues: [] });
    void draft;
    const user = userEvent.setup();
    renderProjects();
    expect(await screen.findByText('Running')).toBeVisible();
    expect(screen.queryByText(/runtime:/i)).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /show status details/i }));
    expect(screen.getByText(/3\/3 running/i)).toBeVisible();
  });

  it('shows query errors without collapsing project route', async () => {
    mockBackend.setErrorOverride('list_profiles', new Error('offline'));
    renderProjects();
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('offline'));
    expect(screen.getByRole('main')).toBeVisible();
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

  it('renders accessible status and profile loading skeletons', async () => {
    mockBackend.setResponseOverride('list_profiles', [{ id: '00000000-0000-0000-0000-000000000001', revision: 1, displayName: 'Demo', composeProjectName: 'demo', workingDirectory: '/tmp', registrationOrigin: 'manual' }]);
    mockBackend.setResponseOverride('get_project_status', new Promise(() => {}));
    renderProjects();
    expect(await screen.findByRole('status', { name: /loading project status/i })).toBeVisible();
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
