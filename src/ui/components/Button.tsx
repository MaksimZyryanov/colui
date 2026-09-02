import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from 'react';

export const Button = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & { children: ReactNode; iconOnly?: boolean; destructive?: boolean }>(function Button({ children, iconOnly = false, destructive = false, className = '', ...props }, ref) {
  return <button ref={ref} {...props} className={`ui-button${iconOnly ? ' ui-button-icon' : ''}${destructive ? ' ui-button-destructive' : ''} ${className}`}>{children}</button>;
});
