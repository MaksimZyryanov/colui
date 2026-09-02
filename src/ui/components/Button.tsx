import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from 'react';

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & { children: ReactNode; destructive?: boolean } & (
  | { iconOnly?: false }
  | { iconOnly: true; 'aria-label': string }
  | { iconOnly: true; 'aria-labelledby': string }
);

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button({ children, iconOnly = false, destructive = false, className = '', ...props }, ref) {
  if (iconOnly && !props['aria-label'] && !props['aria-labelledby']) {
    throw new Error('Icon-only buttons require an accessible name');
  }
  return <button ref={ref} {...props} className={`ui-button${iconOnly ? ' ui-button-icon' : ''}${destructive ? ' ui-button-destructive' : ''} ${className}`}>{children}</button>;
});
