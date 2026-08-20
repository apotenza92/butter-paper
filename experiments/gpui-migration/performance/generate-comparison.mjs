#!/usr/bin/env node

import { readFile, writeFile } from "node:fs/promises";
import { basename, dirname, relative, resolve } from "node:path";

function usage() {
  return `Usage:
  node generate-comparison.mjs --electron <report.json> --gpui <report.json> [more pairs] --output <comparison.html>
`;
}

function parseArguments(argv) {
  const options = { electron: [], gpui: [] };
  for (let index = 0; index < argv.length; index += 2) {
    const option = argv[index];
    const value = argv[index + 1];
    if (!value || !["--electron", "--gpui", "--output"].includes(option)) {
      throw new Error(`invalid arguments\n${usage()}`);
    }
    if (option === "--output") options.output = resolve(value);
    else options[option.slice(2)].push(resolve(value));
  }
  if (!options.output || options.electron.length === 0 || options.gpui.length === 0) {
    throw new Error(`Electron, GPUI, and output paths are required\n${usage()}`);
  }
  return options;
}

async function loadReport(path) {
  const report = JSON.parse(await readFile(path, "utf8"));
  return { path, report };
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function number(value, digits = 1) {
  return Number.isFinite(value) ? value.toFixed(digits) : "Not measured";
}

function measurement(value, suffix, digits = 1) {
  return Number.isFinite(value) ? `${number(value, digits)} ${suffix}` : "Not measured";
}

function durationMedian(report, event) {
  return report.summary?.duration_events_ms?.[event]?.median ?? null;
}

function implementationMetrics(report) {
  const isElectron = report.implementation === "electron";
  const measured = (report.summary?.successful_iterations ?? 0) > 0;
  const operationEvent =
    report.scenario === "page-navigation" ? "page-navigation-completed" : "zoom-completed";
  return {
    success: report.summary?.successful_iterations ?? 0,
    failed: report.summary?.failed_iterations ?? 0,
    firstVisible: measured
      ? durationMedian(report, isElectron ? "first-page-visible" : "viewport-visible")
      : null,
    operation: isElectron
      ? (measured ? durationMedian(report, operationEvent) : null)
      : (measured ? durationMedian(report, "operation-visible") : null),
    frameP95: measured
      ? (isElectron
          ? report.summary?.frames?.interval_ms?.p95
          : report.summary?.frame_intervals_ms?.p95)
      : null,
    peakRssMb: measured ? (report.summary?.process_tree?.peak_rss_kb ?? NaN) / 1024 : null,
    peakCpu: measured ? report.summary?.process_tree?.peak_cpu_percent : null,
  };
}

function metricRows(electron, gpui) {
  const left = implementationMetrics(electron);
  const right = implementationMetrics(gpui);
  return [
    ["Successful / failed runs", `${left.success} / ${left.failed}`, `${right.success} / ${right.failed}`],
    ["Open request to first visible frame", measurement(left.firstVisible, "ms"), measurement(right.firstVisible, "ms")],
    ["Scripted operation median", measurement(left.operation, "ms"), measurement(right.operation, "ms")],
    ["Frame interval p95", measurement(left.frameP95, "ms", 2), measurement(right.frameP95, "ms", 2)],
    ["Peak process-tree RSS", measurement(left.peakRssMb, "MiB"), measurement(right.peakRssMb, "MiB")],
    ["Peak process-tree CPU", measurement(left.peakCpu, "%"), measurement(right.peakCpu, "%")],
  ];
}

function status(report) {
  if ((report.summary?.failed_iterations ?? 0) === 0) return ["Measured", "good"];
  const nativeLaunchFailure = report.implementation === "gpui" && report.iterations?.some(
    (iteration) => /hiservices|kLSNoExecutableErr|window-created/.test(iteration.stderr ?? ""),
  );
  return [nativeLaunchFailure ? "Blocked: native launch" : "Blocked or failed", "bad"];
}

function reportLink(output, item) {
  return relative(dirname(output), item.path).split("\\").join("/");
}

function scenarioSection(output, scenario, electronItem, gpuiItem) {
  const electron = electronItem.report;
  const gpui = gpuiItem.report;
  const [electronStatus, electronClass] = status(electron);
  const [gpuiStatus, gpuiClass] = status(gpui);
  const rows = metricRows(electron, gpui)
    .map(
      ([label, left, right]) =>
        `<tr><th>${escapeHtml(label)}</th><td>${escapeHtml(left)}</td><td>${escapeHtml(right)}</td></tr>`,
    )
    .join("\n");
  return `<section>
  <div class="section-head"><div><span class="eyebrow">Matched Hibbeler scenario</span><h2>${escapeHtml(scenario)}</h2></div><div class="status-pair"><span class="status ${electronClass}">Electron: ${electronStatus}</span><span class="status ${gpuiClass}">GPUI: ${gpuiStatus}</span></div></div>
  <table><thead><tr><th>Metric</th><th>Electron + PDF.js</th><th>GPUI + Poppler</th></tr></thead><tbody>${rows}</tbody></table>
  <p class="sources">Raw evidence: <a href="${escapeHtml(reportLink(output, electronItem))}">${escapeHtml(basename(electronItem.path))}</a> · <a href="${escapeHtml(reportLink(output, gpuiItem))}">${escapeHtml(basename(gpuiItem.path))}</a></p>
</section>`;
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const [electronItems, gpuiItems] = await Promise.all([
    Promise.all(options.electron.map(loadReport)),
    Promise.all(options.gpui.map(loadReport)),
  ]);
  const electronByScenario = new Map(electronItems.map((item) => [item.report.scenario, item]));
  const gpuiByScenario = new Map(gpuiItems.map((item) => [item.report.scenario, item]));
  const scenarios = [...electronByScenario.keys()].filter((scenario) => gpuiByScenario.has(scenario));
  if (scenarios.length === 0) throw new Error("no matching scenarios in the supplied reports");
  const sections = scenarios
    .map((scenario) => scenarioSection(options.output, scenario, electronByScenario.get(scenario), gpuiByScenario.get(scenario)))
    .join("\n");
  const html = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Butter Paper performance comparison</title><style>
  :root{font-family:Inter,system-ui,sans-serif;color:#18181b;background:#f4f4f5}body{margin:0}.page{max-width:1120px;margin:auto;padding:32px}.hero,section{background:white;border:1px solid #d4d4d8;border-radius:14px;padding:24px;margin-bottom:20px}.eyebrow{font-size:12px;text-transform:uppercase;letter-spacing:.08em;color:#71717a}h1,h2{margin:.35rem 0}.lede,.sources{color:#52525b;line-height:1.5}.section-head{display:flex;justify-content:space-between;gap:16px;align-items:start}.status-pair{display:flex;gap:8px;flex-wrap:wrap}.status{font-size:12px;font-weight:650;border-radius:999px;padding:5px 9px}.good{background:#dcfce7;color:#166534}.bad{background:#fee2e2;color:#991b1b}table{border-collapse:collapse;width:100%;margin-top:18px}th,td{text-align:left;border-top:1px solid #e4e4e7;padding:10px 12px}thead th{border-top:0;background:#fafafa}tbody th{width:40%;font-weight:550}a{color:#0f766e}@media(max-width:700px){.page{padding:14px}.section-head{display:block}.status-pair{margin-top:12px}th,td{font-size:13px;padding:8px}}
  </style></head><body><main class="page"><header class="hero"><span class="eyebrow">Generated local development evidence</span><h1>Electron and GPUI performance comparison</h1><p class="lede">The same Hibbeler PDF and scripted interactions are compared. End-to-end product latency is comparable. PDF.js and Poppler raster costs are renderer-specific and must not be attributed only to Electron, React, or GPUI.</p><p><a href="protocol.md">Read the fixed protocol</a> · <a href="../index.html">Return to the UI migration review</a></p></header>${sections}</main></body></html>\n`;
  await writeFile(options.output, html, "utf8");
  process.stdout.write(`Wrote ${options.output}\n`);
}

await main();
