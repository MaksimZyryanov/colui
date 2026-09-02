import type { ReactNode } from 'react';

export function ProjectsRoute(): ReactNode {
  return <main><h1>Projects</h1></main>;
}

export function AppRoutes(): ReactNode {
  return <ProjectsRoute />;
}
