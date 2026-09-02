import * as RadixMenu from '@radix-ui/react-dropdown-menu';
import type { ReactNode } from 'react';

export function DropdownMenu({ trigger, items }: { trigger: ReactNode; items: Array<{ label: string; onSelect: () => void; destructive?: boolean; disabled?: boolean }> }) {
  return <RadixMenu.Root><RadixMenu.Trigger asChild>{trigger}</RadixMenu.Trigger><RadixMenu.Portal><RadixMenu.Content className="ui-menu">{items.map(item => <RadixMenu.Item key={item.label} disabled={item.disabled} className={`ui-menu-item${item.destructive ? ' ui-button-destructive' : ''}`} onSelect={item.onSelect}>{item.label}</RadixMenu.Item>)}</RadixMenu.Content></RadixMenu.Portal></RadixMenu.Root>;
}
