// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mockBackend } from '../../../ipc/mock-backend';
import { DiscoverySection } from '../DiscoverySection';

const session = '00000000-0000-0000-0000-000000000099';
const candidate = (id: string, classification: 'new_unambiguous' | 'name_conflict' | 'incomplete_metadata' | 'already_registered', ignored = false) => ({ candidateId: id.repeat(64), runtimeSessionId: session, inventoryGeneration: 4, composeProjectName: `${classification}-app`, workingDirectory: classification === 'incomplete_metadata' ? null : '/work/app', configFiles: classification === 'incomplete_metadata' ? [] : ['/work/app/compose.yml'], containerCount: 2, classification, conflicts: classification === 'name_conflict' ? [{ source: 'registered_profile', profileId: '00000000-0000-0000-0000-000000000001', workingDirectory: '/other', configFiles: ['/other/compose.yml'] }] : [], ignored });

describe('DiscoverySection', () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  beforeEach(() => { client.clear(); mockBackend.reset(); });
  afterEach(cleanup);

  it('labels every candidate class and exposes manual actions only when eligible', async () => {
    mockBackend.setResponseOverride('list_discovery_candidates', { candidates: [candidate('1', 'new_unambiguous'), candidate('2', 'name_conflict'), candidate('3', 'incomplete_metadata'), candidate('4', 'already_registered')], autoRegistrationEnabled: false });
    render(<QueryClientProvider client={client}><DiscoverySection runtimeSessionId={session} inventoryGeneration={4} registryHealth={null} /></QueryClientProvider>);
    expect(await screen.findByText('New project')).toBeVisible();
    expect(screen.getByText('Name conflict')).toBeVisible();
    expect(screen.getByText('Incomplete metadata')).toBeVisible();
    expect(screen.getByText('Already registered')).toBeVisible();
    expect(screen.getAllByRole('button', { name: /^Register .*app$/ })).toHaveLength(1);
  });

  it('supports ignore, manual registration, and explicit auto-registration', async () => {
    const user = userEvent.setup();
    mockBackend.setResponseOverride('list_discovery_candidates', { candidates: [candidate('1', 'new_unambiguous')], autoRegistrationEnabled: false });
    render(<QueryClientProvider client={client}><DiscoverySection runtimeSessionId={session} inventoryGeneration={4} registryHealth={null} /></QueryClientProvider>);
    await user.click(await screen.findByRole('button', { name: /ignore new_unambiguous-app/i }));
    expect(mockBackend.getInvocations().some(call => call.command === 'ignore_candidate')).toBe(true);
    await waitFor(() => expect(screen.getByRole('button', { name: /register new_unambiguous-app/i })).toBeEnabled());
    await user.click(screen.getByRole('button', { name: /register new_unambiguous-app/i }));
    await waitFor(() => expect(mockBackend.getInvocations().some(call => call.command === 'register_candidate')).toBe(true));
    await user.click(screen.getByRole('switch', { name: /automatic registration/i }));
    await waitFor(() => expect(mockBackend.getInvocations().some(call => call.command === 'configure_auto_registration')).toBe(true));
  });
});
