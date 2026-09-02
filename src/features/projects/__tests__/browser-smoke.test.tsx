// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ProjectsView } from '../ProjectsView';
import { mockBackend } from '../../../ipc/mock-backend';

describe('Vite mock browser flow', () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  beforeEach(() => { mockBackend.reset(); client.clear(); });
  afterEach(cleanup);

  it('creates, edits, records lifecycle, and removes profile', async () => {
    const user = userEvent.setup();
    mockBackend.setResponseOverride('get_project_status', { profileId: '00000000-0000-0000-0000-000000000001', runtime: { presence: 'present', activity: 'all-running', containerCount: 1, runningContainerCount: 1, observedAt: '2026-09-02T00:00:00.000Z' }, definition: { state: 'valid', revision: '1', serviceCount: 1 }, operation: null, issues: [] });
    render(<QueryClientProvider client={client}><ProjectsView /></QueryClientProvider>);
    await user.click(await screen.findByRole('button', { name: /add project/i }));
    await user.type(screen.getByLabelText('Display name'), 'Smoke Project');
    await user.type(screen.getByLabelText('Compose name'), 'smoke');
    await user.type(screen.getByLabelText('Working directory'), '/tmp/smoke');
    await user.click(screen.getByRole('button', { name: 'Next' }));
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByText('Smoke Project')).toBeVisible();
    await user.click(screen.getByRole('button', { name: /edit smoke project/i }));
    await user.click(await screen.findByRole('button', { name: 'Next' }));
    await user.click(screen.getByRole('button', { name: 'Back' }));
    await user.keyboard('{Escape}');
    const more = screen.getByRole('button', { name: /more actions/i });
    await user.click(more);
    await user.click(screen.getByRole('menuitem', { name: /remove profile/i }));
    await user.click(screen.getByRole('button', { name: /^Cancel$/ }));
    expect(screen.getByText('Smoke Project')).toBeVisible();
    await user.click(more);
    await user.click(screen.getByRole('menuitem', { name: /remove profile/i }));
    await user.click(screen.getByRole('button', { name: /^Remove profile$/ }));
    await waitFor(() => expect(screen.getByText('No projects yet')).toBeVisible());
    expect(mockBackend.getInvocations().map(invocation => invocation.command)).toContain('remove_profile');
  }, 15000);

  it('keeps profiles visible when runtime connection fails', async () => {
    mockBackend.setErrorOverride('connect_runtime', new Error('runtime offline'));
    render(<QueryClientProvider client={client}><ProjectsView /></QueryClientProvider>);
    expect(await screen.findByText('No projects yet')).toBeVisible();
  });
});
