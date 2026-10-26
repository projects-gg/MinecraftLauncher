// A friends setting's switch that turns at the click (SOC-LAG). The tester's "arkadaşlık ayarları
// çok geç işliyor": the privacy switches waited for the service's answer (about half a second from
// Turkey, more on a cold start) before they moved. Now the page shows the new value at once, `save`
// sends it, and the switch follows the launcher's view again once that view shows the value (the
// launcher shows a change itself until the service answers), so the answer never makes it flicker.
// A refused change turns the switch back; the caller's `attempt` says why in one short toast.
import { useCallback, useEffect, useRef, useState } from "react";

/**
 * `kept`: the value the launcher's view shows. `save`: sends a new value and settles once it went
 * through or was refused. Returns the value to show and the click handler.
 */
export function useSwitchNow(
  kept: boolean,
  save: (next: boolean) => Promise<unknown>,
): [boolean, (next: boolean) => void] {
  const [pending, setPending] = useState<boolean | null>(null);
  // The newest click: an older click's answer never undoes a newer one.
  const latest = useRef(0);
  useEffect(() => {
    if (pending !== null && kept === pending) setPending(null);
  }, [pending, kept]);
  const change = useCallback(
    (next: boolean) => {
      const click = ++latest.current;
      setPending(next);
      void save(next).then(
        () => {
          // Went through or refused: the view says how it stands now.
          if (latest.current === click) setPending(null);
        },
        () => {
          if (latest.current === click) setPending(null);
        },
      );
    },
    [save],
  );
  return [pending ?? kept, change];
}
