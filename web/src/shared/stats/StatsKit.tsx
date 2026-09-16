// Pure-CSS chart primitives for stats pages (ported from blog-test's BarRow pattern).
// No JS animation — just semantic tokens, so they adapt to the active color theme.

export interface CountRow {
  name: string;
  count: number;
}

export function BarRow({ name, count, max }: { name: string; count: number; max: number }) {
  const pct = max > 0 ? (count / max) * 100 : 0;
  return (
    <div className="flex items-center gap-3 py-1">
      <span className="w-40 shrink-0 truncate text-sm text-foreground">{name}</span>
      <div className="relative h-2 flex-1 overflow-hidden rounded-full bg-surface">
        <div
          className="h-full rounded-full bg-primary/70"
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="w-8 shrink-0 text-right text-sm text-subtle tabular-nums">{count}</span>
    </div>
  );
}

export function SummaryBand({
  items,
}: {
  items: { label: string; value: string | number | null }[];
}) {
  // 데이터가 없는 차원(0 또는 null)은 카드째로 숨긴다 — "0 국가" 같은
  // 빈 요약이 페이지를 채우지 않도록 한다.
  const shown = items.filter(
    (it): it is { label: string; value: string | number } =>
      it.value !== null && it.value !== 0,
  );
  return (
    <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-6">
      {shown.map((it) => (
        <div key={it.label} className="rounded-lg border border-line bg-canvas p-3">
          <div className="text-2xl font-semibold tabular-nums text-foreground">{it.value}</div>
          <div className="text-xs text-subtle">{it.label}</div>
        </div>
      ))}
    </div>
  );
}

export function ColumnChart({
  data,
  max,
}: {
  data: { year: number; count: number }[];
  max: number;
}) {
  const top = Math.max(max, 1);
  return (
    <div className="flex h-40 items-end gap-1">
      {data.map((d) => (
        <div
          key={d.year}
          className="flex flex-1 flex-col items-end justify-end gap-1"
          title={`${d.year}: ${d.count}`}
        >
          <div
            className="w-full rounded-t bg-primary/70"
            style={{
              height: `${(d.count / top) * 100}%`,
              minHeight: d.count > 0 ? "2px" : "0",
            }}
          />
          <span className="text-[10px] text-subtle tabular-nums">{d.year}</span>
        </div>
      ))}
    </div>
  );
}
