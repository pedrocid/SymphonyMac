import { useEffect, useEffectEvent } from "react";
import {
  subscribeToAgentStatusChanged,
  subscribeToOrchestratorStatus,
} from "../lib/api";

interface UseStatusPollingOptions {
  enabled?: boolean;
  pollMs?: number;
}

export function useStatusPolling(
  onRefresh: () => Promise<void> | void,
  options: UseStatusPollingOptions = {},
) {
  const { enabled = true, pollMs = 3000 } = options;
  const refresh = useEffectEvent(() => {
    void onRefresh();
  });

  useEffect(() => {
    if (!enabled) {
      return undefined;
    }

    refresh();

    const intervalId = pollMs > 0 ? window.setInterval(() => refresh(), pollMs) : null;
    const subscriptions = [
      subscribeToAgentStatusChanged(() => refresh()),
      subscribeToOrchestratorStatus(() => refresh()),
    ];

    return () => {
      if (intervalId !== null) {
        window.clearInterval(intervalId);
      }

      for (const unlistenPromise of subscriptions) {
        void unlistenPromise.then((unlisten) => unlisten());
      }
    };
  }, [enabled, pollMs]);
}
