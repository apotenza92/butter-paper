import assert from "node:assert/strict";
import test from "node:test";

import {
  assessElectronImageUploadTrace,
  summarizeElectronImageTrace,
} from "./electron-image-upload-trace.mjs";

const checkerDecode = {
  cat: "cc,benchmark",
  name: "SoftwareImageDecodeCache::DecodeImageInTask",
  ph: "X",
  pid: 10,
  tid: 20,
  args: {
    key: "frame_key[content_id: 39,frame_index: 0]\ntype[SubrectAndScale]\nsrc_rect[0,0 512x384]\ntarget_size[256x192]\nhash[4068201495679635583]",
  },
};

test("records checker decode and compositor submission without inventing a GPU upload", () => {
  const assessment = assessElectronImageUploadTrace([
    checkerDecode,
    {
      cat: "cc",
      name: "CanvasResource::PrepareTransferableResource",
      ph: "X",
      pid: 10,
      tid: 20,
      args: {},
    },
    {
      cat: "gpu",
      name: "SharedImageStub::CreateSharedImage",
      ph: "X",
      pid: 11,
      tid: 21,
      args: { width: 128, height: 64 },
    },
  ], { source_width: 512, source_height: 384 });

  assert.deepEqual(assessment.decoded_source_size, { width: 512, height: 384 });
  assert.deepEqual(assessment.decoded_target_size, { width: 256, height: 192 });
  assert.equal(assessment.decode_recorded, true);
  assert.equal(assessment.compositor_resource_submission_recorded, true);
  assert.equal(assessment.gpu_upload_event_count, 0);
  assert.equal(assessment.attributable_gpu_upload_event_count, 0);
  assert.equal(assessment.upload_bytes, null);
  assert.equal(assessment.physical_bus_bytes_observed, false);
  assert.equal(assessment.status, "blocked");
  assert.equal(assessment.blocker, "software-raster-trace-has-no-gpu-image-upload-event");
});

test("rejects Chromium GPU upload events that omit image identity and byte count", () => {
  const assessment = assessElectronImageUploadTrace([
    checkerDecode,
    {
      cat: "cc",
      name: "GpuImageDecodeCache::UploadImageInTask",
      ph: "X",
      pid: 10,
      tid: 20,
      args: {},
    },
    {
      cat: "cc",
      name: "ImageUploadTaskImpl::RunOnWorkerThread",
      ph: "X",
      pid: 10,
      tid: 20,
      args: { mode: "gpu", source_prepare_tiles_id: 132 },
    },
  ], { source_width: 512, source_height: 384 });

  assert.equal(assessment.gpu_upload_event_count, 2);
  assert.equal(assessment.attributable_gpu_upload_event_count, 0);
  assert.equal(assessment.upload_bytes, null);
  assert.equal(assessment.status, "blocked");
  assert.equal(
    assessment.blocker,
    "chromium-gpu-image-upload-trace-has-no-per-image-identity-or-byte-count",
  );
});

test("embeds the fail-closed upload assessment in the compact trace summary", () => {
  const summary = summarizeElectronImageTrace([
    { cat: "metadata", name: "thread_name", args: { name: "CrRendererMain" } },
    checkerDecode,
  ], { source_width: 512, source_height: 384 });

  assert.equal(summary.total_event_count, 2);
  assert.equal(summary.candidate_event_count, 1);
  assert.equal(summary.upload_assessment.decode_recorded, true);
  assert.equal(summary.upload_assessment.upload_bytes, null);
  assert.equal(summary.candidate_events.length, 1);
});
