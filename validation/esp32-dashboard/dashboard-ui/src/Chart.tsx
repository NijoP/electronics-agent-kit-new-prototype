// Line chart of one history column. A gap (null = failed sensor read) breaks the line;
// it is never interpolated.

const W = 600;
const H = 180;
const PAD = 8;

interface Props {
  title: string;
  unit: string;
  digits: number;
  values: (number | null)[];
  className: string;
  limit?: { value: number; label: string };
}

export function Chart({ title, unit, digits, values, className, limit }: Props) {
  const present = values.filter((v): v is number => v !== null);
  if (present.length < 2) {
    return (
      <figure className="chart">
        <figcaption>{title}</figcaption>
        <p className="muted">not enough samples yet</p>
      </figure>
    );
  }
  const rawLo = Math.min(...present);
  const rawHi = Math.max(...present);
  const margin = Math.max((rawHi - rawLo) * 0.1, 0.5);
  const lo = rawLo - margin;
  const hi = rawHi + margin;
  const n = values.length;
  const x = (i: number) => (i * W) / (n - 1);
  const y = (v: number) => PAD + ((hi - v) / (hi - lo)) * (H - 2 * PAD);

  const segments: string[] = [];
  let current = "";
  values.forEach((v, i) => {
    if (v === null) {
      if (current) segments.push(current);
      current = "";
      return;
    }
    current += `${current ? "L" : "M"}${x(i).toFixed(1)} ${y(v).toFixed(1)}`;
  });
  if (current) segments.push(current);
  const missing = n - present.length;

  return (
    <figure className="chart">
      <figcaption>
        {title}
        <span className="muted">
          {` ${rawLo.toFixed(digits)}–${rawHi.toFixed(digits)} ${unit} · ${n} samples`}
          {missing > 0 ? ` · ${missing} missing` : ""}
        </span>
      </figcaption>
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" role="img" aria-label={title}>
        {limit && limit.value > lo && limit.value < hi && (
          <line className="limit" x1={0} x2={W} y1={y(limit.value)} y2={y(limit.value)}>
            <title>{limit.label}</title>
          </line>
        )}
        {segments.map((d, i) => (
          <path key={i} d={d} className={className} />
        ))}
      </svg>
    </figure>
  );
}
