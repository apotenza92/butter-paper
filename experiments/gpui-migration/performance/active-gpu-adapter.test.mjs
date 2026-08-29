import assert from "node:assert/strict";
import test from "node:test";

import {
  activeGpuAdapterRequired,
  annotateElectronActiveGpuDevice,
  buildElectronActiveGpuAdapterReceipt,
  buildGpuiActiveGpuAdapterReceipt,
  parseSingleNvidiaAdapterIdentity,
} from "./active-gpu-adapter.mjs";

const nvidiaIdentity =
  "NVIDIA RTX 4000 Ada Generation, GPU-87203edb-af3c-bf52-7395-ee3ad8f7389f, 580.173.02, 20475";

function chromiumNvidiaInfo() {
  return {
    gpu: {
      devices: [
        {
          vendorId: 0x10de,
          deviceId: 0x27b2,
          vendorString: "NVIDIA Corporation",
          deviceString:
            "ANGLE (NVIDIA Corporation, NVIDIA RTX 4000 Ada Generation/PCIe/SSE2, OpenGL 4.6)",
          driverVendor: "NVIDIA",
          driverVersion: "580.173.2",
        },
      ],
      auxAttributes: {
        displayType: "ANGLE_OPENGL",
        glRenderer:
          "ANGLE (NVIDIA Corporation, NVIDIA RTX 4000 Ada Generation/PCIe/SSE2, OpenGL 4.6)",
      },
      featureStatus: { gpu_compositing: "enabled" },
    },
  };
}

test("binds Chromium's active hardware renderer to the single NVIDIA UUID", () => {
  const info = chromiumNvidiaInfo();
  const receipt = buildElectronActiveGpuAdapterReceipt(info, nvidiaIdentity);
  assert.equal(receipt.passed, true);
  assert.equal(receipt.active, true);
  assert.equal(receipt.vendor_id, 0x10de);
  assert.equal(receipt.device_uuid, "GPU-87203edb-af3c-bf52-7395-ee3ad8f7389f");
  assert.equal(
    receipt.selection_source,
    "chromium-system-info-active-gl-renderer",
  );
  const annotated = annotateElectronActiveGpuDevice(info, receipt);
  assert.equal(annotated.gpu.devices[0].active, true);
  assert.deepEqual(annotated.gpu.active_adapter_receipt, receipt);
});

test("selects the active NVIDIA renderer when Chromium also enumerates a virtual display adapter", () => {
  const info = chromiumNvidiaInfo();
  info.gpu.devices[0].deviceString = "";
  info.gpu.devices.push({
    vendorId: 0x1af4,
    deviceId: 0x1050,
    vendorString: "",
    deviceString: "",
    driverVendor: "",
    driverVersion: "",
  });
  const receipt = buildElectronActiveGpuAdapterReceipt(info, nvidiaIdentity);
  assert.equal(receipt.passed, true);
  assert.equal(receipt.active_device_index, 0);
  assert.match(receipt.device_name, /NVIDIA RTX 4000 Ada Generation/);
  const annotated = annotateElectronActiveGpuDevice(info, receipt);
  assert.equal(annotated.gpu.devices[0].active, true);
  assert.equal(annotated.gpu.devices[1].active, false);
});

test("does not issue an NVIDIA UUID for Chromium software rendering", () => {
  const info = chromiumNvidiaInfo();
  info.gpu.devices[0] = {
    vendorId: 0x1af4,
    deviceId: 0x1050,
    vendorString: "Google Inc. (Mesa)",
    deviceString: "ANGLE (Mesa, llvmpipe (LLVM 21.1.8, 256 bits))",
    driverVendor: "Mesa",
    driverVersion: "26.0.8",
  };
  info.gpu.auxAttributes.glRenderer = info.gpu.devices[0].deviceString;
  info.gpu.featureStatus.gpu_compositing = "disabled_software";
  const receipt = buildElectronActiveGpuAdapterReceipt(info, nvidiaIdentity);
  assert.equal(receipt.passed, false);
  assert.equal(receipt.is_software_emulated, true);
  assert.equal(receipt.device_uuid, null);
  assert.match(receipt.blocker, /software-emulated/);
});

test("binds the GPUI window's selected wgpu adapter to the NVIDIA UUID", () => {
  const receipt = buildGpuiActiveGpuAdapterReceipt(
    [
      {
        schema_version: 1,
        event: "gpu-adapter-selected",
        t_ms: 42,
        available: true,
        is_software_emulated: false,
        device_name: "NVIDIA RTX 4000 Ada Generation",
        driver_name: "NVIDIA",
        driver_info: "580.173.02",
      },
    ],
    nvidiaIdentity,
  );
  assert.equal(receipt.passed, true);
  assert.equal(receipt.active, true);
  assert.equal(receipt.selection_source, "gpui-window-gpu-specs");
  assert.equal(receipt.device_uuid, "GPU-87203edb-af3c-bf52-7395-ee3ad8f7389f");
});

test("rejects duplicate or software GPUI adapter selection receipts", () => {
  const selected = {
    event: "gpu-adapter-selected",
    available: true,
    is_software_emulated: true,
    device_name: "llvmpipe (LLVM 21.1.8, 256 bits)",
    driver_name: "llvmpipe",
    driver_info: "Mesa 26.0.8",
  };
  for (const events of [[selected], [selected, selected]]) {
    const receipt = buildGpuiActiveGpuAdapterReceipt(events, nvidiaIdentity);
    assert.equal(receipt.passed, false);
    assert.equal(receipt.device_uuid, null);
  }
});

test("parses one exact NVIDIA identity and activates only on the paid lane", () => {
  assert.deepEqual(parseSingleNvidiaAdapterIdentity(nvidiaIdentity), {
    name: "NVIDIA RTX 4000 Ada Generation",
    uuid: "GPU-87203edb-af3c-bf52-7395-ee3ad8f7389f",
    driver_version: "580.173.02",
    memory_total_mib: 20475,
  });
  assert.equal(
    parseSingleNvidiaAdapterIdentity(`${nvidiaIdentity}\n${nvidiaIdentity}`),
    null,
  );
  assert.equal(activeGpuAdapterRequired({ BP_PERF_REQUIRE_NVIDIA: "1" }), true);
  assert.equal(activeGpuAdapterRequired({}), false);
});
