/** A Unix time in seconds as a local date and time. */
export function formatTime(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleString();
}

export function describeError(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}
