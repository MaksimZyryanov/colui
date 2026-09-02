import type { ReactNode } from 'react';
export function Alert({ children, variant = 'info' }: { children: ReactNode; variant?: 'info' | 'destructive' }) { return <div role="alert" className={`ui-alert ui-alert-${variant}`}>{children}</div>; }
