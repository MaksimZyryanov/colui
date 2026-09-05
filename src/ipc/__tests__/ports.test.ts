import { expect, it } from 'vitest';
import { containerInstanceSchema } from '../schemas';

it('preserves all partial published host bindings', () => {
  for (const hostIp of [null, '::']) {
    for (const hostPort of [null, 8080]) {
      const port = { hostIp, hostPort, containerPort: 80, protocol: 'tcp', action: { copy: '80/tcp', url: null } };
      const parsed = containerInstanceSchema.parse({ id: 'abc', name: 'test', image: 'alpine', state: 'running', statusText: '', publishedPorts: [port] });
      expect(parsed.publishedPorts).toEqual([port]);
    }
  }
});
