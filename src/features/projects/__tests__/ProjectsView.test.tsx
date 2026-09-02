// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ProjectsView } from '../ProjectsView';
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
});
