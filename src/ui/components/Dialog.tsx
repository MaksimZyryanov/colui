import * as RadixDialog from '@radix-ui/react-dialog';
import type { ReactNode } from 'react';
import { Button } from './Button';

export function Dialog({ trigger, title, description, children }: { trigger: ReactNode; title: string; description: string; children: ReactNode }) {
  return <RadixDialog.Root><RadixDialog.Trigger asChild>{trigger}</RadixDialog.Trigger><RadixDialog.Portal><RadixDialog.Overlay className="ui-overlay" /><RadixDialog.Content className="ui-dialog"><RadixDialog.Title>{title}</RadixDialog.Title><RadixDialog.Description>{description}</RadixDialog.Description><div>{children}</div><RadixDialog.Close asChild><Button aria-label="Close dialog">Close</Button></RadixDialog.Close></RadixDialog.Content></RadixDialog.Portal></RadixDialog.Root>;
}
