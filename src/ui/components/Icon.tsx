import type { SVGProps } from 'react';

type IconName = 'activity' | 'chevron-down' | 'chevron-right' | 'logs' | 'play' | 'rotate-cw' | 'settings-2' | 'square';

const paths: Record<IconName, readonly string[]> = {
  activity: ['M3 12h4l3 8l4 -16l3 8h4'],
  'chevron-down': ['M6 9l6 6l6 -6'],
  'chevron-right': ['M9 6l6 6l-6 6'],
  logs: ['M4 12h.01', 'M4 6h.01', 'M4 18h.01', 'M8 18h2', 'M8 12h2', 'M8 6h2', 'M14 6h6', 'M14 12h6', 'M14 18h6'],
  play: ['M7 4v16l13 -8l-13 -8'],
  'rotate-cw': ['M4.05 11a8 8 0 1 1 .5 4m-.5 5v-5h5'],
  'settings-2': ['M19.875 6.27a2.225 2.225 0 0 1 1.125 1.948v7.284c0 .809 -.443 1.555 -1.158 1.948l-6.75 4.27a2.269 2.269 0 0 1 -2.184 0l-6.75 -4.27a2.225 2.225 0 0 1 -1.158 -1.948v-7.285c0 -.809 .443 -1.554 1.158 -1.947l6.75 -3.98a2.33 2.33 0 0 1 2.25 0l6.75 3.98h-.033', 'M9 12a3 3 0 1 0 6 0a3 3 0 1 0 -6 0'],
  square: ['M3 5a2 2 0 0 1 2 -2h14a2 2 0 0 1 2 2v14a2 2 0 0 1 -2 2h-14a2 2 0 0 1 -2 -2v-14'],
};

export function Icon({ name, ...props }: { name: IconName } & SVGProps<SVGSVGElement>) {
  return <svg {...props} aria-hidden="true" className={`ui-icon ${props.className ?? ''}`} focusable="false" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    {paths[name].map(path => <path key={path} d={path} />)}
  </svg>;
}
