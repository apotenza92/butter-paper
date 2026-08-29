const receiptType = "bp-active-renderer-adapter-v1";
const softwareRendererPattern =
  /swiftshader|llvmpipe|software rasterizer|microsoft basic render|lavapipe/i;

function normalizeName(value) {
  return String(value ?? "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

function normalizeDriverVersion(value) {
  const parts = String(value ?? "").match(/\d+/g);
  return parts?.map((part) => String(Number(part))).join(".") ?? "";
}

function nameMatches(candidate, expected) {
  const left = normalizeName(candidate);
  const right = normalizeName(expected);
  return (
    left.length > 0 &&
    right.length > 0 &&
    (left.includes(right) || right.includes(left))
  );
}

function numericId(value) {
  if (typeof value === "number" && Number.isInteger(value)) return value;
  if (typeof value !== "string" || value.trim().length === 0) return null;
  const parsed = Number.parseInt(value, 0);
  return Number.isInteger(parsed) ? parsed : null;
}

export function parseSingleNvidiaAdapterIdentity(value) {
  const rows = String(value ?? "")
    .trim()
    .split("\n")
    .map((row) => row.trim())
    .filter(Boolean);
  if (rows.length !== 1) return null;
  const fields = rows[0].split(",").map((field) => field.trim());
  if (fields.length !== 4) return null;
  const [name, uuid, driverVersion, memoryTotalMib] = fields;
  if (
    name.length === 0 ||
    !/^GPU-[A-Za-z0-9-]+$/.test(uuid) ||
    normalizeDriverVersion(driverVersion).length === 0 ||
    !/^\d+(?:\.\d+)?$/.test(memoryTotalMib)
  ) {
    return null;
  }
  return {
    name,
    uuid,
    driver_version: driverVersion,
    memory_total_mib: Number(memoryTotalMib),
  };
}

function receipt({ implementation, selectionSource, fields, failures }) {
  const passed = failures.length === 0;
  return {
    schema_version: 1,
    receipt_type: receiptType,
    implementation,
    selection_source: selectionSource,
    active: fields.active === true,
    active_device_index: fields.active_device_index ?? null,
    available: fields.available === true,
    is_software_emulated: fields.is_software_emulated,
    vendor_id: fields.vendor_id,
    device_id: fields.device_id,
    device_name: fields.device_name,
    driver_name: fields.driver_name,
    driver_info: fields.driver_info,
    backend: fields.backend,
    device_uuid: passed ? fields.device_uuid : null,
    uuid_source: passed
      ? "single-nvidia-smi-adapter-matched-to-active-renderer"
      : null,
    passed,
    blocker: passed ? null : failures.join("; "),
  };
}

export function buildElectronActiveGpuAdapterReceipt(
  browserGpuInfo,
  nvidiaGpuIdentity,
) {
  const gpu = browserGpuInfo?.gpu;
  const devices = Array.isArray(gpu?.devices) ? gpu.devices : [];
  const nvidiaDevices = devices
    .map((device, index) => ({ device, index }))
    .filter(({ device }) => numericId(device?.vendorId) === 0x10de);
  const selected = nvidiaDevices.length === 1 ? nvidiaDevices[0] : null;
  const device = selected?.device ?? null;
  const renderer = gpu?.auxAttributes?.glRenderer ?? "";
  const compositor = gpu?.featureStatus?.gpu_compositing ?? "";
  const nvidia = parseSingleNvidiaAdapterIdentity(nvidiaGpuIdentity);
  const vendorId = numericId(device?.vendorId);
  // Chromium can enumerate a second virtual display adapter and can leave the
  // NVIDIA deviceString empty even while ANGLE's active GL renderer identifies
  // the NVIDIA device exactly. Select the sole NVIDIA device and use the active
  // renderer as its name only when deviceString is absent.
  const deviceName = device?.deviceString || renderer;
  const driverInfo = device?.driverVersion ?? "";
  const active = device !== null && renderer.length > 0;
  const failures = [];
  if (!nvidia) failures.push("exactly one NVIDIA adapter identity is required");
  if (nvidiaDevices.length !== 1) {
    failures.push("Chromium did not report exactly one NVIDIA GPU device");
  }
  if (!active) failures.push("Chromium active GL renderer is unavailable");
  if (vendorId !== 0x10de)
    failures.push("Chromium active device is not NVIDIA");
  if (
    softwareRendererPattern.test(
      `${deviceName} ${renderer} ${device?.vendorString ?? ""}`,
    )
  ) {
    failures.push("Chromium active renderer is software-emulated");
  }
  if (nvidia && !nameMatches(deviceName, nvidia.name)) {
    failures.push("Chromium active device name differs from nvidia-smi");
  }
  if (nvidia && !nameMatches(renderer, nvidia.name)) {
    failures.push("Chromium active GL renderer differs from nvidia-smi");
  }
  if (
    nvidia &&
    normalizeDriverVersion(driverInfo) !==
      normalizeDriverVersion(nvidia.driver_version)
  ) {
    failures.push("Chromium active driver differs from nvidia-smi");
  }
  if (typeof compositor !== "string" || !compositor.startsWith("enabled")) {
    failures.push("Chromium GPU compositing is not enabled");
  }
  return receipt({
    implementation: "electron",
    selectionSource: "chromium-system-info-active-gl-renderer",
    fields: {
      active,
      active_device_index: selected?.index ?? null,
      available: device !== null,
      is_software_emulated: softwareRendererPattern.test(
        `${deviceName} ${renderer}`,
      ),
      vendor_id: vendorId,
      device_id: numericId(device?.deviceId),
      device_name: deviceName || null,
      driver_name: device?.driverVendor ?? null,
      driver_info: driverInfo || null,
      backend: gpu?.auxAttributes?.displayType ?? null,
      device_uuid: nvidia?.uuid ?? null,
    },
    failures,
  });
}

export function annotateElectronActiveGpuDevice(
  browserGpuInfo,
  adapterReceipt,
) {
  if (!browserGpuInfo?.gpu || !Array.isArray(browserGpuInfo.gpu.devices)) {
    return browserGpuInfo;
  }
  return {
    ...browserGpuInfo,
    gpu: {
      ...browserGpuInfo.gpu,
      devices: browserGpuInfo.gpu.devices.map((device, index) => ({
        ...device,
        active:
          adapterReceipt?.active === true &&
          index === adapterReceipt.active_device_index,
      })),
      active_adapter_receipt: adapterReceipt,
    },
  };
}

export function buildGpuiActiveGpuAdapterReceipt(events, nvidiaGpuIdentity) {
  const selected = Array.isArray(events)
    ? events.filter((event) => event?.event === "gpu-adapter-selected")
    : [];
  const event = selected.length === 1 ? selected[0] : null;
  const nvidia = parseSingleNvidiaAdapterIdentity(nvidiaGpuIdentity);
  const deviceName = event?.device_name ?? "";
  const driverName = event?.driver_name ?? "";
  const driverInfo = event?.driver_info ?? "";
  const software =
    event?.is_software_emulated === true ||
    softwareRendererPattern.test(`${deviceName} ${driverName} ${driverInfo}`);
  const failures = [];
  if (!nvidia) failures.push("exactly one NVIDIA adapter identity is required");
  if (selected.length !== 1) {
    failures.push("GPUI did not report exactly one selected wgpu adapter");
  }
  if (event?.available !== true) {
    failures.push("GPUI selected adapter is unavailable");
  }
  if (software) failures.push("GPUI selected adapter is software-emulated");
  if (nvidia && !nameMatches(deviceName, nvidia.name)) {
    failures.push("GPUI selected adapter name differs from nvidia-smi");
  }
  if (!/nvidia/i.test(driverName)) {
    failures.push("GPUI selected adapter driver is not NVIDIA");
  }
  if (
    nvidia &&
    normalizeDriverVersion(driverInfo) !==
      normalizeDriverVersion(nvidia.driver_version)
  ) {
    failures.push("GPUI selected adapter driver differs from nvidia-smi");
  }
  return receipt({
    implementation: "gpui",
    selectionSource: "gpui-window-gpu-specs",
    fields: {
      active: event !== null,
      active_device_index: null,
      available: event?.available === true,
      is_software_emulated:
        typeof event?.is_software_emulated === "boolean"
          ? event.is_software_emulated
          : null,
      vendor_id: failures.length === 0 ? 0x10de : null,
      device_id: null,
      device_name: deviceName || null,
      driver_name: driverName || null,
      driver_info: driverInfo || null,
      backend: "wgpu",
      device_uuid: nvidia?.uuid ?? null,
    },
    failures,
  });
}

export function activeGpuAdapterRequired(environment = process.env) {
  return environment.BP_PERF_REQUIRE_NVIDIA === "1";
}
