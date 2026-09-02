import { Component, type ErrorInfo, type ReactNode } from 'react';
import { Button } from './Button';
import { Alert } from './Alert';
import { AppErrorException } from '../../ipc/errors';

type Props = { children: ReactNode };
type State = { error: Error | null };
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };
  static getDerivedStateFromError(error: Error): State { return { error }; }
  componentDidCatch(error: Error, info: ErrorInfo) { console.error('Feature boundary caught error', error, info.componentStack); }
  render() {
    if (!this.state.error) return this.props.children;
    const error = this.state.error;
    const mismatch = error instanceof AppErrorException && error.code === 'protocol_mismatch';
    const details = error instanceof AppErrorException ? error.details : null;
    return <Alert variant="destructive"><strong>{mismatch ? 'Response format mismatch' : 'Feature failed to load'}</strong><p>{mismatch ? 'Retry connection to refresh response format.' : 'Something went wrong. Retry to continue.'}</p>{details ? <p>{details.slice(0, 500)}</p> : null}<Button onClick={() => this.setState({ error: null })}>Retry</Button></Alert>;
  }
}
