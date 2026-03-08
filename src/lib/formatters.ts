export function formatElapsed(
  startedAt: string,
  finishedAt: string | null,
  totalSecs?: number,
): string {
  const secs = totalSecs !== undefined
    ? Math.max(0, Math.floor(totalSecs))
    : startedAt
      ? Math.max(
          0,
          Math.floor(
            ((finishedAt ? new Date(finishedAt).getTime() : Date.now()) -
              new Date(startedAt).getTime()) / 1000,
          ),
        )
      : 0;

  if (secs < 60) return `${secs}s`;

  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ${secs % 60}s`;

  const hours = Math.floor(mins / 60);
  return `${hours}h ${mins % 60}m`;
}

export function formatRelativeDate(dateStr: string): string {
  if (!dateStr) return "";

  const date = new Date(dateStr);
  if (Number.isNaN(date.getTime())) return "";

  const diffMs = Date.now() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);

  if (diffMins < 1) return "Just now";
  if (diffMins < 60) return `${diffMins}m ago`;

  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours}h ago`;

  return `${Math.floor(diffHours / 24)}d ago`;
}

export function formatCompactNumber(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}k`;
  return value.toString();
}

export function formatTimestamp(timestamp: string): string {
  try {
    const date = new Date(timestamp);
    return date.toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return "";
  }
}

export function formatWorkspaceAge(days: number): string {
  if (days < 1) return "< 1 day";
  if (days < 2) return "1 day";
  return `${Math.floor(days)} days`;
}

export function formatBytes(sizeBytes: number): string {
  if (sizeBytes < 1024 * 1024) {
    return `${(sizeBytes / 1024).toFixed(1)} KB`;
  }

  if (sizeBytes < 1024 * 1024 * 1024) {
    return `${(sizeBytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  return `${(sizeBytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}
