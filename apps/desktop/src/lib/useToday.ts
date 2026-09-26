import { useEffect, useState } from "react";
import { todayIso } from "./format";

/**
 * Today's local date ("YYYY-MM-DD"), updated at midnight — so "Today"/"Tomorrow" labels and
 * day windows stay right when the app is left open overnight.
 */
export function useToday(): string {
  const [today, setToday] = useState(todayIso);
  useEffect(() => {
    // The start of the day after `today`; +1 s so the new day has certainly begun.
    const [y, m, d] = today.split("-").map(Number) as [number, number, number];
    const nextMidnight = new Date(y, m - 1, d + 1).getTime();
    const timer = setTimeout(() => setToday(todayIso()), nextMidnight - Date.now() + 1000);
    return () => clearTimeout(timer);
  }, [today]);
  return today;
}
