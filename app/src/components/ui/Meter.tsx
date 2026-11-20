// One meter for every usage figure (a plan's session or week, a team's
// budget): a thin bar, grey until 70 %, amber from 70, red from 90.

export function meterLevel(percent: number): "ok" | "warn" | "danger" {
  return percent >= 90 ? "danger" : percent >= 70 ? "warn" : "ok";
}

export function Meter({ percent, label }: { percent: number; label: string }) {
  const value = Math.max(0, Math.min(100, Math.round(percent)));
  return (
    <span
      className={`meter meter--${meterLevel(value)}`}
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={value}
    >
      <span className="meter__fill" style={{ width: `${value}%` }} />
    </span>
  );
}
