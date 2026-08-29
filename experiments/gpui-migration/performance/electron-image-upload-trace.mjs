export const electronImageTraceCategories = [
  "blink",
  "cc",
  "gpu",
  "skia",
  "viz",
  "disabled-by-default-blink.image_decoding",
  "disabled-by-default-cc",
  "disabled-by-default-gpu.debug",
  "disabled-by-default-gpu.service",
  "disabled-by-default-skia",
  "disabled-by-default-skia.gpu",
  "disabled-by-default-skia.gpu.cache",
  "disabled-by-default-devtools.timeline",
];

export async function startElectronImageTrace(cdp) {
  await cdp.send("Tracing.start", {
    categories: electronImageTraceCategories.join(","),
    options: "record-as-much-as-possible",
    transferMode: "ReturnAsStream",
  });
}

export async function stopElectronImageTrace(cdp) {
  const completion = cdp.waitForEvent("Tracing.tracingComplete", 20_000);
  await cdp.send("Tracing.end");
  const { stream } = await completion;
  if (!stream) throw new Error("Chromium tracing completed without a result stream");
  let payload = "";
  for (;;) {
    const chunk = await cdp.send("IO.read", { handle: stream });
    payload += chunk.base64Encoded
      ? Buffer.from(chunk.data, "base64").toString("utf8")
      : chunk.data;
    if (chunk.eof) break;
  }
  await cdp.send("IO.close", { handle: stream });
  const parsed = JSON.parse(payload);
  return parsed.traceEvents ?? [];
}

function traceEventSearchable(traceEvent) {
  return `${traceEvent?.cat ?? ""} ${traceEvent?.name ?? ""} ${JSON.stringify(traceEvent?.args ?? {})}`;
}

function parseDecodeGeometry(traceEvent) {
  const key = traceEvent?.args?.key;
  if (typeof key !== "string") return null;
  const source = key.match(/src_rect\[-?\d+,-?\d+ (\d+)x(\d+)\]/);
  const target = key.match(/target_size\[(\d+)x(\d+)\]/);
  if (!source) return null;
  return {
    source: { width: Number(source[1]), height: Number(source[2]) },
    target: target ? { width: Number(target[1]), height: Number(target[2]) } : null,
  };
}

/**
 * Chromium's image-decode cache trace names can prove that the locked checker
 * reached decode and raster preparation. The corresponding GPU upload trace
 * points expose neither the decoded image key nor an uploaded byte count. Keep
 * this assessment fail-closed: decoded RGBA size, texture allocation size, and
 * unrelated SharedImage creation are not receipts for this image's GPU upload.
 */
export function assessElectronImageUploadTrace(traceEvents, expectedSource = {}) {
  const events = traceEvents ?? [];
  const decoded = events
    .map((traceEvent) => ({ traceEvent, geometry: parseDecodeGeometry(traceEvent) }))
    .filter(({ traceEvent, geometry }) => geometry
      && /ImageDecodeCache::DecodeImageInTask/.test(traceEvent?.name ?? ""));
  const matchedDecode = decoded.find(({ geometry }) => (
    (!Number.isFinite(expectedSource.source_width)
      || geometry.source.width === expectedSource.source_width)
    && (!Number.isFinite(expectedSource.source_height)
      || geometry.source.height === expectedSource.source_height)
  )) ?? null;
  const gpuUploads = events.filter(({ name = "" }) => (
    /GpuImageDecodeCache::UploadImage(?:InTask)?/.test(name)
    || /ImageUploadTaskImpl::RunOnWorkerThread/.test(name)
  ));
  const compositorSubmissions = events.filter(({ name = "" }) => (
    /PrepareTransferableResource|ProduceCanvasResource/.test(name)
  ));

  // Current Chromium upload events contain no image key, content id, source
  // geometry, or byte count. Thread/timestamp proximity is not image identity.
  const attributableGpuUploads = [];
  return {
    status: "blocked",
    decode_recorded: matchedDecode !== null,
    decoded_source_size: matchedDecode?.geometry.source ?? null,
    decoded_target_size: matchedDecode?.geometry.target ?? null,
    compositor_resource_submission_recorded: compositorSubmissions.length > 0,
    compositor_resource_submission_event_count: compositorSubmissions.length,
    gpu_upload_event_count: gpuUploads.length,
    attributable_gpu_upload_event_count: attributableGpuUploads.length,
    upload_bytes: null,
    physical_bus_bytes_observed: false,
    blocker: gpuUploads.length > 0
      ? "chromium-gpu-image-upload-trace-has-no-per-image-identity-or-byte-count"
      : "software-raster-trace-has-no-gpu-image-upload-event",
  };
}

export function summarizeElectronImageTrace(traceEvents, expectedSource = {}) {
  const candidates = (traceEvents ?? []).filter((traceEvent) => {
    return /image|decode|upload|texture|transfer|raster/i.test(traceEventSearchable(traceEvent));
  });
  return {
    total_event_count: traceEvents?.length ?? 0,
    candidate_event_count: candidates.length,
    upload_assessment: assessElectronImageUploadTrace(traceEvents, expectedSource),
    candidate_events: candidates.slice(0, 2_000).map((traceEvent) => ({
      cat: traceEvent.cat ?? null,
      name: traceEvent.name ?? null,
      ph: traceEvent.ph ?? null,
      pid: traceEvent.pid ?? null,
      tid: traceEvent.tid ?? null,
      id: traceEvent.id ?? traceEvent.id2 ?? null,
      args: traceEvent.args ?? {},
    })),
    truncated: candidates.length > 2_000,
  };
}
