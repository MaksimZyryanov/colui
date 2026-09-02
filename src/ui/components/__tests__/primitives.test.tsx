// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { render, screen, cleanup } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { AlertDialog } from '../AlertDialog';
import { Button } from '../Button';
import { Dialog } from '../Dialog';
import { DropdownMenu } from '../DropdownMenu';
import { ErrorBoundary } from '../ErrorBoundary';
import { AppErrorException } from '../../../ipc/errors';

afterEach(cleanup);

function ConfirmationFixture() {
  return <AlertDialog trigger={<Button>Remove profile</Button>} title="Remove profile" description="This cannot be undone." confirmLabel="Remove" destructive>
    Profile removal confirmation
  </AlertDialog>;
}

function ThrowProtocolMismatch(): never {
  throw new AppErrorException({ code: 'protocol_mismatch', operation: 'load', message: 'unexpected payload', details: 'x'.repeat(1000), retryable: true });
}

describe('accessible primitives', () => {
  it('returns focus to AlertDialog trigger after Escape', async () => {
    const user = userEvent.setup();
    render(<ConfirmationFixture />);
    const trigger = screen.getByRole('button', { name: /remove profile/i });
    await user.click(trigger);
    expect(screen.getByRole('alertdialog', { name: 'Remove profile' })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Cancel' })).toHaveFocus();
    await user.keyboard('{Escape}');
    expect(trigger).toHaveFocus();
  });

  it('labels regular dialogs and exposes descriptions', async () => {
    const user = userEvent.setup();
    render(<Dialog trigger={<Button>Open details</Button>} title="Details" description="Useful details">Content</Dialog>);
    await user.click(screen.getByRole('button', { name: 'Open details' }));
    const dialog = screen.getByRole('dialog', { name: 'Details' });
    expect(dialog).toHaveAccessibleDescription('Useful details');
  });

  it('gives icon-only buttons accessible names', () => {
    render(<Button iconOnly aria-label="Close panel">X</Button>);
    expect(screen.getByRole('button', { name: 'Close panel' })).toBeVisible();
  });

  it('supports keyboard navigation in dropdown menus', async () => {
    const user = userEvent.setup();
    render(<DropdownMenu trigger={<Button>Actions</Button>} items={[{ label: 'Edit', onSelect: vi.fn() }, { label: 'Delete', onSelect: vi.fn(), destructive: true }]} />);
    await user.click(screen.getByRole('button', { name: 'Actions' }));
    await user.keyboard('{ArrowDown}');
    expect(screen.getByRole('menuitem', { name: 'Edit' })).toHaveFocus();
  });

  it('shows bounded protocol mismatch recovery in boundary fallback', () => {
    render(<ErrorBoundary><ThrowProtocolMismatch /></ErrorBoundary>);
    expect(screen.getByText(/response format mismatch/i)).toBeVisible();
    expect(screen.getByRole('button', { name: /retry/i })).toBeVisible();
    expect(screen.queryByText('x'.repeat(501))).not.toBeInTheDocument();
    expect(screen.queryByText(/diagnostics/i)).not.toBeInTheDocument();
  });

  it('resets boundary after retry', async () => {
    const user = userEvent.setup();
    let failed = true;
    function Flaky() { if (failed) throw new Error('boom'); return <p>Recovered</p>; }
    function Fixture() { return <ErrorBoundary><Flaky /></ErrorBoundary>; }
    render(<Fixture />);
    failed = false;
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    expect(screen.getByText('Recovered')).toBeVisible();
  });
});
