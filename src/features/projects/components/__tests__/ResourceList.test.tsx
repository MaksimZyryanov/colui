// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { useState } from 'react';
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it } from 'vitest';
import { ResourceList } from '../ResourceList';
import { ResourceRow } from '../ResourceRow';

afterEach(cleanup);

function LocalDetails() {
  const [open, setOpen] = useState(false);
  return <ResourceRow name="Demo" kind="registered" status="Running" actions={<button onClick={() => setOpen(true)}>Open details</button>} footer={open ? <p>Details remain open</p> : null} />;
}

it('keeps profile details mounted after the registration alias is cleared on a session change', async () => {
  const resource = { id: 'profile:one', name: 'Demo', content: <LocalDetails /> };
  const view = render(<ResourceList resources={[{ ...resource, previousId: 'candidate:old-session:one' }]} />);
  await userEvent.setup().click(screen.getByRole('button', { name: 'Open details' }));
  expect(screen.getByText('Details remain open')).toBeVisible();
  view.rerender(<ResourceList resources={[resource]} />);
  expect(screen.getByText('Details remain open')).toBeVisible();
});
