import { useEffect, useRef } from "react";
import type { DashboardChartOption } from "./dashboardCharts";

interface DashboardChartProps {
  description: string;
  empty: boolean;
  option: DashboardChartOption;
  title: string;
}

export function DashboardChart({
  description,
  empty,
  option,
  title,
}: DashboardChartProps) {
  const chartElement = useRef<HTMLDivElement>(null);
  const headingId = `${title.toLowerCase().replaceAll(/[^a-z0-9]+/g, "-")}-heading`;

  useEffect(() => {
    const element = chartElement.current;
    if (!element || empty) return;
    let active = true;
    let dispose = () => undefined;
    void import("./dashboardCharts").then(({ mountDashboardChart }) => {
      if (!active) return;
      const chart = mountDashboardChart(element, option);
      const resize = new ResizeObserver(() => chart.resize());
      resize.observe(element);
      dispose = () => {
        resize.disconnect();
        chart.dispose();
      };
    });
    return () => {
      active = false;
      dispose();
    };
  }, [empty, option]);

  return (
    <section className="dashboard-chart-panel" aria-labelledby={headingId}>
      <div className="dashboard-panel-heading">
        <h3 id={headingId}>{title}</h3>
        <small>Offline · chart menu exports PNG</small>
      </div>
      <p className="sr-only">{description}</p>
      {empty ? (
        <div className="chart-empty">
          <strong>No supported records</strong>
          <span>{description}</span>
        </div>
      ) : (
        <div
          className="dashboard-chart"
          ref={chartElement}
          role="img"
          aria-label={`${title}. ${description}`}
        />
      )}
    </section>
  );
}
