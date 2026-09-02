import { Alert } from "./alert";
import { Button } from "./button";

interface RetryAlertProps {
  message: string;
  actionLabel: string;
  onRetry(): void;
}

export function RetryAlert({ message, actionLabel, onRetry }: RetryAlertProps) {
  return (
    <Alert size="lg">
      <p>{message}</p>
      <Button type="button" onClick={onRetry} className="mt-3">{actionLabel}</Button>
    </Alert>
  );
}
