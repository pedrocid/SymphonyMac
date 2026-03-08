import { useEffect, useEffectEvent } from "react";

type SubscribeFn<TPayload> = (handler: (payload: TPayload) => void) => Promise<() => void>;

interface UseTauriSubscriptionOptions {
  enabled?: boolean;
}

export function useTauriSubscription<TPayload>(
  subscribe: SubscribeFn<TPayload>,
  handler: (payload: TPayload) => void,
  options: UseTauriSubscriptionOptions = {},
) {
  const { enabled = true } = options;
  const onEvent = useEffectEvent(handler);

  useEffect(() => {
    if (!enabled) {
      return undefined;
    }

    let active = true;
    const unlistenPromise = subscribe((payload) => {
      if (active) {
        onEvent(payload);
      }
    });

    return () => {
      active = false;
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [enabled, subscribe]);
}
