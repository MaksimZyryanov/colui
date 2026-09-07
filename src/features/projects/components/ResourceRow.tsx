import { useId, useState, type ReactNode } from 'react';
import { Button } from '../../../ui/components/Button';

// One presentation for groups, standalone containers, and nested containers.
// Action ownership and dialogs stay in the feature components.
export function ResourceRow({ name, kind, status, actions, primaryAction, notice, children, footer }: {
  name: string;
  kind: 'registered' | 'discovered' | 'standalone' | 'container';
  status: ReactNode;
  actions?: ReactNode;
  primaryAction?: ReactNode;
  notice?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
}) {
  const id = useId();
  const [expanded, setExpanded] = useState(false);
  const grouped = kind === 'registered' || kind === 'discovered';
  return <section className={`resource resource-${kind}`} aria-labelledby={`${id}-name`}>
    <div className="resource-row">
      {grouped ? <Button className="resource-chevron" aria-label={`${expanded ? 'Collapse' : 'Expand'} ${name}`} aria-expanded={expanded} aria-controls={`${id}-children`} onClick={() => setExpanded(value => !value)}><span aria-hidden="true">{expanded ? '⌄' : '›'}</span></Button> : <span className="resource-indent" />}
      {status}
      <h3 id={`${id}-name`}>{name}</h3>
      <span className="resource-kind">{kind}</span>
      {notice}
      <div className="resource-actions">{actions}</div>
      {primaryAction}
    </div>
    {grouped && expanded ? <div id={`${id}-children`} className="resource-children">{children}</div> : null}
    {footer}
  </section>;
}

export function ResourceStatus({ label, tone = 'unknown' }: { label: string; tone?: 'running' | 'stopped' | 'warning' | 'unknown' }) {
  return <span className={`resource-status resource-status-${tone}`} title={label}><span className="sr-only">{label}</span></span>;
}
