import { Component, type ErrorInfo, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@pv/ui';

function Fallback({ onRetry }: { onRetry: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="error-boundary" role="alert">
      <p>{t('errors.ui.render')}</p>
      <Button variant="secondary" onClick={onRetry}>
        {t('errors.ui.retry')}
      </Button>
    </div>
  );
}

/**
 * A rendering error in one area (a panel, the views) shows a message there instead of
 * blanking the whole window; the model, in Rust, is untouched. « Retry » renders again.
 */
export class ErrorBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  override state = { failed: false };

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('render error', error, info.componentStack);
  }

  override render(): ReactNode {
    if (this.state.failed) {
      return (
        <Fallback
          onRetry={() => {
            this.setState({ failed: false });
          }}
        />
      );
    }
    return this.props.children;
  }
}
