import * as RadixAlertDialog from '@radix-ui/react-alert-dialog';
import type { ReactNode } from 'react';
import { Button } from './Button';
import { useRef } from 'react';

export function AlertDialog({ trigger, title, description, children, confirmLabel = 'Confirm', destructive = false }: { trigger: ReactNode; title: string; description: string; children: ReactNode; confirmLabel?: string; destructive?: boolean }) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  return <RadixAlertDialog.Root><RadixAlertDialog.Trigger asChild>{trigger}</RadixAlertDialog.Trigger><RadixAlertDialog.Portal><RadixAlertDialog.Overlay className="ui-overlay" /><RadixAlertDialog.Content className="ui-dialog" onOpenAutoFocus={event => { if (destructive) { event.preventDefault(); cancelRef.current?.focus(); } }}><RadixAlertDialog.Title>{title}</RadixAlertDialog.Title><RadixAlertDialog.Description>{description}</RadixAlertDialog.Description><div>{children}</div><div className="ui-dialog-actions"><RadixAlertDialog.Cancel asChild><Button ref={cancelRef}>Cancel</Button></RadixAlertDialog.Cancel><RadixAlertDialog.Action asChild><Button destructive={destructive}>{confirmLabel}</Button></RadixAlertDialog.Action></div></RadixAlertDialog.Content></RadixAlertDialog.Portal></RadixAlertDialog.Root>;
}
