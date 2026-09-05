// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mockBackend } from '../../../ipc/mock-backend';
import { DiscoverySection } from '../DiscoverySection';
import { AppErrorException } from '../../../ipc/errors';

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

  it('tracks concurrent register and ignore by candidate without blocking another candidate', async () => {
    let finishRegister!: (value: unknown) => void;
    let finishIgnore!: (value: unknown) => void;
    mockBackend.setResponseOverride('list_discovery_candidates', { candidates: [candidate('1', 'new_unambiguous'), candidate('2', 'new_unambiguous')], autoRegistrationEnabled: false });
    mockBackend.setResponseOverride('register_candidate', new Promise(resolve => { finishRegister = resolve; }));
    mockBackend.setResponseOverride('ignore_candidate', new Promise(resolve => { finishIgnore = resolve; }));
    const user = userEvent.setup();
    render(<QueryClientProvider client={client}><DiscoverySection runtimeSessionId={session} inventoryGeneration={4} registryHealth={null} /></QueryClientProvider>);
    await user.click((await screen.findAllByRole('button', { name: /register new_unambiguous-app/i }))[0]);
    const ignoreButtons = screen.getAllByRole('button', { name: /ignore new_unambiguous-app/i });
    expect(ignoreButtons[0]).toBeDisabled();
    expect(ignoreButtons[1]).toBeEnabled();
    await user.click(ignoreButtons[1]);
    expect(screen.getAllByRole('button', { name: /ignore new_unambiguous-app/i })[1]).toBeDisabled();
    finishRegister({}); finishIgnore(false);
  });

  it('retains deterministic feedback for concurrent same-type candidate requests', async () => {
    let finishA!: (value: unknown) => void;
    let rejectB!: (error: unknown) => void;
    const a = { ...candidate('1', 'new_unambiguous'), composeProjectName: 'candidate-a' };
    const b = { ...candidate('2', 'new_unambiguous'), composeProjectName: 'candidate-b' };
    mockBackend.setResponseOverride('list_discovery_candidates', { candidates: [a, b], autoRegistrationEnabled: false });
    mockBackend.setResponseOverride('register_candidate', new Promise(resolve => { finishA = resolve; }));
    const user = userEvent.setup();
    render(<QueryClientProvider client={client}><DiscoverySection runtimeSessionId={session} inventoryGeneration={4} registryHealth={null} /></QueryClientProvider>);
    await user.click(await screen.findByRole('button', { name: 'Register candidate-a' }));
    await waitFor(() => expect(mockBackend.getInvocations().filter(call => call.command === 'register_candidate')).toHaveLength(1));
    mockBackend.setResponseOverride('register_candidate', new Promise((_resolve, reject) => { rejectB = reject; }));
    await user.click(screen.getByRole('button', { name: 'Register candidate-b' }));
    rejectB(new AppErrorException({ code: 'candidate_stale', operation: 'register_candidate', subject: { kind: 'candidate', id: b.candidateId }, message: 'Candidate B changed', details: 'B detail', retryable: false }));
    finishA({ id: '00000000-0000-0000-0000-000000000001', revision: 1, displayName: 'candidate-a', composeProjectName: 'candidate-a', workingDirectory: '/work/app', registrationOrigin: 'discovered' });
    expect(await screen.findByRole('alert', { name: 'Registration candidate-b failed' })).toHaveTextContent('candidate_stale');
    expect(await screen.findByRole('status', { name: 'Registration candidate-a succeeded' })).toBeVisible();
    expect(screen.getByRole('alert', { name: 'Registration candidate-b failed' })).toHaveTextContent('Candidate B changed');
  });

  it.each([
    ['ignore_candidate', /ignore new_unambiguous-app/i, 'Ignore'],
    ['register_candidate', /register new_unambiguous-app/i, 'Registration'],
    ['configure_auto_registration', /automatic registration/i, 'Automatic registration configuration'],
    ['auto_register_candidates', /register eligible now/i, 'Automatic registration'],
  ])('announces typed %s failure', async (command, buttonName, action) => {
    mockBackend.setResponseOverride('list_discovery_candidates', { candidates: [candidate('1', 'new_unambiguous')], autoRegistrationEnabled: false });
    mockBackend.setErrorOverride(command, new AppErrorException({ code: 'discovery_conflict', operation: command, subject: { kind: 'candidate', id: '1'.repeat(64) }, message: 'Candidate changed', details: 'Observed tuple changed', retryable: false }));
    const user = userEvent.setup();
    render(<QueryClientProvider client={client}><DiscoverySection runtimeSessionId={session} inventoryGeneration={4} registryHealth={null} /></QueryClientProvider>);
    await user.click(await screen.findByRole(command === 'configure_auto_registration' ? 'switch' : 'button', { name: buttonName }));
    const expectedAction = command === 'register_candidate' || command === 'ignore_candidate' ? `${action} new_unambiguous-app` : action;
    expect(await screen.findByRole('alert', { name: `${expectedAction} failed` })).toHaveTextContent('discovery_conflict');
  });

  it('announces discovery mutation successes', async () => {
    mockBackend.setResponseOverride('list_discovery_candidates', { candidates: [candidate('1', 'new_unambiguous')], autoRegistrationEnabled: false });
    const user = userEvent.setup();
    render(<QueryClientProvider client={client}><DiscoverySection runtimeSessionId={session} inventoryGeneration={4} registryHealth={null} /></QueryClientProvider>);
    await user.click(await screen.findByRole('button', { name: /register new_unambiguous-app/i }));
    expect(await screen.findByRole('status', { name: 'Registration new_unambiguous-app succeeded' })).toBeVisible();
    await user.click(screen.getByRole('button', { name: /ignore new_unambiguous-app/i }));
    expect(await screen.findByRole('status', { name: 'Ignore new_unambiguous-app succeeded' })).toBeVisible();
    await user.click(screen.getByRole('switch', { name: /automatic registration/i }));
    expect(await screen.findByRole('status', { name: 'Automatic registration configuration succeeded' })).toBeVisible();
    await user.click(screen.getByRole('button', { name: /register eligible now/i }));
    expect(await screen.findByRole('status', { name: 'Automatic registration succeeded' })).toBeVisible();
  });
});
