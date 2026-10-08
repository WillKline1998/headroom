import type { HistoryPoint, Limit } from "../api";
import { axisLabels, chartPaths, hasEnoughData, windowChart } from "../details/history";

const W = 700;
const H = 110;
const LEFT = 34; // room for the y-axis labels
const PLOT_W = W - LEFT;

/** How one limit's window filled so far, against an even-pace diagonal. Hand-drawn SVG, themed by CSS. */
export function WindowChart({ limit, history, now }: { limit: Limit; history: HistoryPoint[]; now: Date }) {
  const chart = windowChart(history, limit, now);
  const labels = axisLabels(new Date(limit.resetsAt!), limit.windowSecs!);

  return (
    <figure className="window-chart">
      <figcaption>{limit.label}</figcaption>
      {hasEnoughData(chart) ? (
        <>
          <ChartSvg chart={chart} />
          <div className="chart-axis" style={{ marginLeft: `${(LEFT / W) * 100}%` }}>
            <span>{labels.start}</span>
            <span>{labels.end}</span>
          </div>
        </>
      ) : (
        <p className="hint">Headroom records each check; this chart fills in as you use Claude.</p>
      )}
    </figure>
  );
}

function ChartSvg({ chart }: { chart: ReturnType<typeof windowChart> }) {
  const { line, area } = chartPaths(chart.points, PLOT_W, H);
  return (
    <svg viewBox={`0 0 ${W} ${H + 8}`} role="img" aria-label="Percent used over the window" className="window-svg">
      {[0, 50, 100].map((v) => {
        const y = H - (v / 100) * H;
        return (
          <g key={v}>
            <line x1={LEFT} x2={W} y1={y} y2={y} className="wc-grid" />
            <text x={LEFT - 6} y={y + 3.5} textAnchor="end" className="wc-label">{v}%</text>
          </g>
        );
      })}
      <g transform={`translate(${LEFT} 0)`}>
        <line x1={0} y1={H} x2={PLOT_W} y2={0} className="wc-pace" />
        <path d={area} className="wc-area" />
        <path d={line} className="wc-line" />
      </g>
    </svg>
  );
}
