import { BarChart, LineChart } from "echarts/charts";
import {
  GridComponent,
  LegendComponent,
  ToolboxComponent,
  TooltipComponent,
} from "echarts/components";
import {
  init,
  use as registerEChartsModules,
  type EChartsCoreOption,
} from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";

registerEChartsModules([
  BarChart,
  LineChart,
  GridComponent,
  LegendComponent,
  ToolboxComponent,
  TooltipComponent,
  CanvasRenderer,
]);

export function mountDashboardChart(
  element: HTMLDivElement,
  option: EChartsCoreOption,
) {
  const chart = init(element, undefined, { renderer: "canvas" });
  chart.setOption(option);
  return chart;
}

export type DashboardChartOption = EChartsCoreOption;
