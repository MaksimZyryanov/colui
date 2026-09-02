import type { HTMLAttributes, ReactNode } from 'react';
export function Card({ children, className = '', ...props }: HTMLAttributes<HTMLDivElement> & { children: ReactNode }) { return <section {...props} className={`ui-card ${className}`}>{children}</section>; }
