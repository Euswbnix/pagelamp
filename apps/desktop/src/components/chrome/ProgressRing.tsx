/** A small determinate ring (decorative: the capsule's text says the same). */
export function ProgressRing({ value }: { value: number }) {
  const r = 6;
  const c = 2 * Math.PI * r;
  const done = Math.min(1, Math.max(0, value));
  return (
    <svg viewBox="0 0 16 16" className="size-4 -rotate-90" aria-hidden>
      <circle
        cx="8"
        cy="8"
        r={r}
        fill="none"
        stroke="currentColor"
        strokeOpacity={0.2}
        strokeWidth="2"
      />
      <circle
        cx="8"
        cy="8"
        r={r}
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeDasharray={c}
        strokeDashoffset={c * (1 - done)}
        className="pl-progress-ring"
      />
    </svg>
  );
}
