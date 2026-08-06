const FOUR_NEIGHBOURS = [[1, 0], [-1, 0], [0, 1], [0, -1]];
const EIGHT_NEIGHBOURS = [...FOUR_NEIGHBOURS, [1, 1], [1, -1], [-1, 1], [-1, -1]];

export function createInkMask(image, { background = [255, 255, 255], threshold = 24, alphaThreshold = 16 } = {}) {
  assertImage(image);
  if (!Array.isArray(background) || background.length < 3) throw new Error('background must contain RGB values');
  const mask = new Uint8Array(image.width * image.height);
  for (let i = 0; i < mask.length; i += 1) {
    const offset = i * 4;
    const alpha = image.data[offset + 3];
    const distance = Math.max(
      Math.abs(image.data[offset] - background[0]),
      Math.abs(image.data[offset + 1] - background[1]),
      Math.abs(image.data[offset + 2] - background[2]),
    );
    mask[i] = alpha >= alphaThreshold && distance >= threshold ? 1 : 0;
  }
  return { width: image.width, height: image.height, data: mask };
}

export function createExclusionMask(image, { threshold = 128, invert = false } = {}) {
  assertImage(image);
  const data = new Uint8Array(image.width * image.height);
  for (let i = 0; i < data.length; i += 1) {
    const offset = i * 4;
    const luminance = rgbaLuminance(image.data, offset);
    const selected = image.data[offset + 3] > 0 && luminance < threshold;
    data[i] = (invert ? !selected : selected) ? 1 : 0;
  }
  return { width: image.width, height: image.height, data };
}

export function excludeMaskPixels(mask, exclusionMask) {
  assertMask(mask);
  if (!exclusionMask) return cloneMask(mask);
  assertSameMaskSize(mask, exclusionMask);
  const data = Uint8Array.from(mask.data, (value, index) => exclusionMask.data[index] ? 0 : value);
  return { width: mask.width, height: mask.height, data };
}

export function connectedComponents(mask, { connectivity = 8, minimumArea = 1 } = {}) {
  assertMask(mask);
  if (connectivity !== 4 && connectivity !== 8) throw new Error('connectivity must be 4 or 8');
  const visited = new Uint8Array(mask.data.length);
  const components = [];
  const neighbours = connectivity === 4 ? FOUR_NEIGHBOURS : EIGHT_NEIGHBOURS;
  for (let start = 0; start < mask.data.length; start += 1) {
    if (!mask.data[start] || visited[start]) continue;
    const queue = [start];
    const pixels = [];
    visited[start] = 1;
    let cursor = 0;
    let minX = mask.width;
    let minY = mask.height;
    let maxX = -1;
    let maxY = -1;
    while (cursor < queue.length) {
      const index = queue[cursor++];
      const x = index % mask.width;
      const y = Math.floor(index / mask.width);
      pixels.push(index);
      minX = Math.min(minX, x); minY = Math.min(minY, y);
      maxX = Math.max(maxX, x); maxY = Math.max(maxY, y);
      for (const [dx, dy] of neighbours) {
        const nx = x + dx; const ny = y + dy;
        if (nx < 0 || ny < 0 || nx >= mask.width || ny >= mask.height) continue;
        const next = ny * mask.width + nx;
        if (mask.data[next] && !visited[next]) { visited[next] = 1; queue.push(next); }
      }
    }
    if (pixels.length >= minimumArea) components.push({
      area: pixels.length,
      bounds: { x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1 },
      pixels,
    });
  }
  return components
    .sort((a, b) => b.area - a.area || a.bounds.y - b.bounds.y || a.bounds.x - b.bounds.x)
    .map(({ pixels: _pixels, ...component }) => component);
}

export function analyzeOutlineContinuity(mask, { connectivity = 8, minimumArea = 1 } = {}) {
  assertMask(mask);
  const detailed = detailedComponents(mask, { connectivity, minimumArea });
  const foregroundPixels = detailed.reduce((sum, component) => sum + component.area, 0);
  const nearestGaps = detailed.map((component, index) => {
    let nearest = Number.POSITIVE_INFINITY;
    for (let otherIndex = 0; otherIndex < detailed.length; otherIndex += 1) {
      if (otherIndex === index) continue;
      nearest = Math.min(nearest, componentDistance(component, detailed[otherIndex]));
    }
    return Number.isFinite(nearest) ? Math.max(0, nearest - 1) : 0;
  });
  return {
    foregroundPixels,
    componentCount: detailed.length,
    components: detailed.map(({ pixels: _pixels, boundary: _boundary, ...component }) => component),
    largestComponentRatio: foregroundPixels ? detailed[0].area / foregroundPixels : 1,
    disconnectedPixels: foregroundPixels - (detailed[0]?.area ?? 0),
    maximumNearestComponentGap: nearestGaps.length ? Math.max(...nearestGaps) : 0,
    minimumIntercomponentGap: nearestGaps.length > 1 ? Math.min(...nearestGaps) : 0,
    enclosedBackgroundRegions: enclosedBackgroundRegions(mask),
  };
}

export function maskIoU(left, right, offset = { x: 0, y: 0 }, exclusionMask) {
  assertMask(left); assertMask(right);
  if (exclusionMask) assertSameMaskSize(left, exclusionMask);
  let intersection = 0; let union = 0;
  const minX = Math.min(0, offset.x); const minY = Math.min(0, offset.y);
  const maxX = Math.max(left.width, offset.x + right.width);
  const maxY = Math.max(left.height, offset.y + right.height);
  for (let y = minY; y < maxY; y += 1) {
    for (let x = minX; x < maxX; x += 1) {
      if (x >= 0 && y >= 0 && x < left.width && y < left.height && exclusionMask?.data[y * left.width + x]) continue;
      const a = x >= 0 && y >= 0 && x < left.width && y < left.height ? left.data[y * left.width + x] : 0;
      const bx = x - offset.x; const by = y - offset.y;
      const b = bx >= 0 && by >= 0 && bx < right.width && by < right.height ? right.data[by * right.width + bx] : 0;
      if (a && b) intersection += 1;
      if (a || b) union += 1;
    }
  }
  return union ? intersection / union : 1;
}

export function registerMasks(reference, candidate, { maximumOffset = 8, exclusionMask } = {}) {
  assertMask(reference); assertMask(candidate);
  if (!Number.isInteger(maximumOffset) || maximumOffset < 0) throw new Error('maximumOffset must be a non-negative integer');
  let best = { x: 0, y: 0, iou: -1 };
  for (let y = -maximumOffset; y <= maximumOffset; y += 1) {
    for (let x = -maximumOffset; x <= maximumOffset; x += 1) {
      const iou = maskIoU(reference, candidate, { x, y }, exclusionMask);
      if (iou > best.iou || (iou === best.iou && tieBreak(x, y, best.x, best.y))) best = { x, y, iou };
    }
  }
  return best;
}

export function boundaryDistance(left, right, offset = { x: 0, y: 0 }) {
  const a = boundaryPoints(left);
  const b = boundaryPoints(right).map(({ x, y }) => ({ x: x + offset.x, y: y + offset.y }));
  if (!a.length && !b.length) return { mean: 0, p95: 0, hausdorff: 0 };
  if (!a.length || !b.length) return { mean: Number.POSITIVE_INFINITY, p95: Number.POSITIVE_INFINITY, hausdorff: Number.POSITIVE_INFINITY };
  const distances = [...nearestDistances(a, b), ...nearestDistances(b, a)].sort((x, y) => x - y);
  return {
    mean: distances.reduce((sum, value) => sum + value, 0) / distances.length,
    p95: percentile(distances, 0.95),
    hausdorff: distances[distances.length - 1],
  };
}

export function translateImage(image, offset = { x: 0, y: 0 }, { background = [255, 255, 255, 255] } = {}) {
  assertImage(image);
  const data = new Uint8Array(image.data.length);
  for (let index = 0; index < image.width * image.height; index += 1) data.set(background, index * 4);
  for (let y = 0; y < image.height; y += 1) {
    for (let x = 0; x < image.width; x += 1) {
      const destinationX = x + offset.x;
      const destinationY = y + offset.y;
      if (destinationX < 0 || destinationY < 0 || destinationX >= image.width || destinationY >= image.height) continue;
      const sourceOffset = (y * image.width + x) * 4;
      const destinationOffset = (destinationY * image.width + destinationX) * 4;
      data.set(image.data.subarray(sourceOffset, sourceOffset + 4), destinationOffset);
    }
  }
  return { width: image.width, height: image.height, data };
}

export function basicSsim(left, right, { exclusionMask } = {}) {
  assertSameSize(left, right);
  if (exclusionMask) assertSameImageAndMask(left, exclusionMask);
  const indices = includedPixelIndices(left.width, left.height, exclusionMask);
  if (!indices.length) return 1;
  let meanA = 0; let meanB = 0;
  for (const i of indices) { meanA += rgbaLuminance(left.data, i * 4); meanB += rgbaLuminance(right.data, i * 4); }
  meanA /= indices.length; meanB /= indices.length;
  let varianceA = 0; let varianceB = 0; let covariance = 0;
  for (const i of indices) {
    const da = rgbaLuminance(left.data, i * 4) - meanA;
    const db = rgbaLuminance(right.data, i * 4) - meanB;
    varianceA += da * da; varianceB += db * db; covariance += da * db;
  }
  const denominator = Math.max(1, indices.length - 1);
  varianceA /= denominator; varianceB /= denominator; covariance /= denominator;
  const c1 = (0.01 * 255) ** 2; const c2 = (0.03 * 255) ** 2;
  return ((2 * meanA * meanB + c1) * (2 * covariance + c2))
    / ((meanA ** 2 + meanB ** 2 + c1) * (varianceA + varianceB + c2));
}

export function luminanceDifference(left, right, { exclusionMask } = {}) {
  assertSameSize(left, right);
  if (exclusionMask) assertSameImageAndMask(left, exclusionMask);
  const indices = includedPixelIndices(left.width, left.height, exclusionMask);
  if (!indices.length) return { meanAbsoluteError: 0, rootMeanSquareError: 0, maximumAbsoluteError: 0 };
  let absolute = 0; let squared = 0; let maximum = 0;
  for (const i of indices) {
    const delta = Math.abs(rgbaLuminance(left.data, i * 4) - rgbaLuminance(right.data, i * 4));
    absolute += delta; squared += delta ** 2; maximum = Math.max(maximum, delta);
  }
  return {
    meanAbsoluteError: absolute / indices.length,
    rootMeanSquareError: Math.sqrt(squared / indices.length),
    maximumAbsoluteError: maximum,
  };
}

export function comparisonHeatmap(left, right, { exclusionMask } = {}) {
  assertSameSize(left, right);
  if (exclusionMask) assertSameImageAndMask(left, exclusionMask);
  const data = new Uint8Array(left.data.length);
  for (let i = 0; i < left.width * left.height; i += 1) {
    const offset = i * 4;
    if (exclusionMask?.data[i]) {
      data.set([128, 128, 128, 80], offset);
      continue;
    }
    const delta = Math.max(
      Math.abs(left.data[offset] - right.data[offset]),
      Math.abs(left.data[offset + 1] - right.data[offset + 1]),
      Math.abs(left.data[offset + 2] - right.data[offset + 2]),
    );
    data[offset] = delta;
    data[offset + 1] = Math.max(0, 255 - delta * 2);
    data[offset + 2] = 0;
    data[offset + 3] = 255;
  }
  return { width: left.width, height: left.height, data };
}

export function comparisonOverlay(leftMask, rightMask, offset = { x: 0, y: 0 }, exclusionMask) {
  assertMask(leftMask); assertMask(rightMask);
  if (leftMask.width !== rightMask.width || leftMask.height !== rightMask.height) throw new Error('Masks must have identical dimensions');
  if (exclusionMask) assertSameMaskSize(leftMask, exclusionMask);
  const data = new Uint8Array(leftMask.width * leftMask.height * 4);
  for (let y = 0; y < leftMask.height; y += 1) {
    for (let x = 0; x < leftMask.width; x += 1) {
      const index = y * leftMask.width + x;
      const bx = x - offset.x; const by = y - offset.y;
      const left = leftMask.data[index];
      const right = bx >= 0 && by >= 0 && bx < rightMask.width && by < rightMask.height ? rightMask.data[by * rightMask.width + bx] : 0;
      const rgba = exclusionMask?.data[index]
        ? [180, 180, 180, 96]
        : left && right ? [0, 0, 0, 255]
          : left ? [0, 180, 255, 255]
            : right ? [255, 0, 180, 255]
              : [255, 255, 255, 255];
      data.set(rgba, index * 4);
    }
  }
  return { width: leftMask.width, height: leftMask.height, data };
}

export function encodePgm(mask) {
  assertMask(mask);
  const header = Buffer.from(`P5\n${mask.width} ${mask.height}\n255\n`);
  return Buffer.concat([header, Buffer.from(Uint8Array.from(mask.data, (value) => value ? 255 : 0))]);
}

export function encodePpm(image) {
  assertImage(image);
  const rgb = Buffer.alloc(image.width * image.height * 3);
  for (let i = 0; i < image.width * image.height; i += 1) {
    rgb[i * 3] = image.data[i * 4]; rgb[i * 3 + 1] = image.data[i * 4 + 1]; rgb[i * 3 + 2] = image.data[i * 4 + 2];
  }
  return Buffer.concat([Buffer.from(`P6\n${image.width} ${image.height}\n255\n`), rgb]);
}

export function cropImage(image, roi) {
  assertImage(image);
  for (const key of ['x', 'y', 'width', 'height']) if (!Number.isInteger(roi?.[key])) throw new Error(`ROI ${key} must be an integer`);
  if (roi.x < 0 || roi.y < 0 || roi.width <= 0 || roi.height <= 0 || roi.x + roi.width > image.width || roi.y + roi.height > image.height) {
    throw new Error('ROI lies outside the image');
  }
  const data = new Uint8Array(roi.width * roi.height * 4);
  for (let y = 0; y < roi.height; y += 1) {
    const sourceStart = ((roi.y + y) * image.width + roi.x) * 4;
    data.set(image.data.subarray(sourceStart, sourceStart + roi.width * 4), y * roi.width * 4);
  }
  return { width: roi.width, height: roi.height, data };
}

function detailedComponents(mask, { connectivity, minimumArea }) {
  const visited = new Uint8Array(mask.data.length);
  const neighbours = connectivity === 4 ? FOUR_NEIGHBOURS : EIGHT_NEIGHBOURS;
  const components = [];
  for (let start = 0; start < mask.data.length; start += 1) {
    if (!mask.data[start] || visited[start]) continue;
    const queue = [start]; const pixels = []; visited[start] = 1;
    let cursor = 0; let minX = mask.width; let minY = mask.height; let maxX = -1; let maxY = -1;
    while (cursor < queue.length) {
      const index = queue[cursor++]; const x = index % mask.width; const y = Math.floor(index / mask.width);
      pixels.push(index); minX = Math.min(minX, x); minY = Math.min(minY, y); maxX = Math.max(maxX, x); maxY = Math.max(maxY, y);
      for (const [dx, dy] of neighbours) {
        const nx = x + dx; const ny = y + dy;
        if (nx < 0 || ny < 0 || nx >= mask.width || ny >= mask.height) continue;
        const next = ny * mask.width + nx;
        if (mask.data[next] && !visited[next]) { visited[next] = 1; queue.push(next); }
      }
    }
    if (pixels.length >= minimumArea) {
      const pixelSet = new Set(pixels);
      const boundary = pixels.filter((index) => {
        const x = index % mask.width; const y = Math.floor(index / mask.width);
        return FOUR_NEIGHBOURS.some(([dx, dy]) => {
          const nx = x + dx; const ny = y + dy;
          return nx < 0 || ny < 0 || nx >= mask.width || ny >= mask.height || !pixelSet.has(ny * mask.width + nx);
        });
      }).map((index) => ({ x: index % mask.width, y: Math.floor(index / mask.width) }));
      components.push({ area: pixels.length, bounds: { x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1 }, pixels, boundary });
    }
  }
  return components.sort((a, b) => b.area - a.area || a.bounds.y - b.bounds.y || a.bounds.x - b.bounds.x);
}

function componentDistance(left, right) {
  let minimumSquared = Number.POSITIVE_INFINITY;
  for (const a of left.boundary) for (const b of right.boundary) minimumSquared = Math.min(minimumSquared, (a.x - b.x) ** 2 + (a.y - b.y) ** 2);
  return Math.sqrt(minimumSquared);
}

function enclosedBackgroundRegions(mask) {
  const visited = new Uint8Array(mask.data.length);
  const queue = [];
  const visit = (x, y) => {
    if (x < 0 || y < 0 || x >= mask.width || y >= mask.height) return;
    const index = y * mask.width + x;
    if (mask.data[index] || visited[index]) return;
    visited[index] = 1; queue.push(index);
  };
  for (let x = 0; x < mask.width; x += 1) { visit(x, 0); visit(x, mask.height - 1); }
  for (let y = 0; y < mask.height; y += 1) { visit(0, y); visit(mask.width - 1, y); }
  for (let cursor = 0; cursor < queue.length; cursor += 1) {
    const index = queue[cursor]; const x = index % mask.width; const y = Math.floor(index / mask.width);
    for (const [dx, dy] of FOUR_NEIGHBOURS) visit(x + dx, y + dy);
  }
  let enclosed = 0;
  for (let start = 0; start < mask.data.length; start += 1) {
    if (mask.data[start] || visited[start]) continue;
    enclosed += 1; visited[start] = 1; const region = [start];
    for (let cursor = 0; cursor < region.length; cursor += 1) {
      const index = region[cursor]; const x = index % mask.width; const y = Math.floor(index / mask.width);
      for (const [dx, dy] of FOUR_NEIGHBOURS) {
        const nx = x + dx; const ny = y + dy;
        if (nx < 0 || ny < 0 || nx >= mask.width || ny >= mask.height) continue;
        const next = ny * mask.width + nx;
        if (!mask.data[next] && !visited[next]) { visited[next] = 1; region.push(next); }
      }
    }
  }
  return enclosed;
}

function boundaryPoints(mask) {
  assertMask(mask);
  const points = [];
  for (let y = 0; y < mask.height; y += 1) for (let x = 0; x < mask.width; x += 1) {
    if (!mask.data[y * mask.width + x]) continue;
    if (x === 0 || y === 0 || x === mask.width - 1 || y === mask.height - 1
      || !mask.data[y * mask.width + x - 1] || !mask.data[y * mask.width + x + 1]
      || !mask.data[(y - 1) * mask.width + x] || !mask.data[(y + 1) * mask.width + x]) points.push({ x, y });
  }
  return points;
}

function nearestDistances(source, target) {
  return source.map((point) => Math.sqrt(target.reduce((best, other) => Math.min(best, (point.x - other.x) ** 2 + (point.y - other.y) ** 2), Number.POSITIVE_INFINITY)));
}

function percentile(sorted, fraction) {
  if (!sorted.length) return Number.NaN;
  const index = Math.min(sorted.length - 1, Math.max(0, Math.ceil(fraction * sorted.length) - 1));
  return sorted[index];
}

function includedPixelIndices(width, height, exclusionMask) {
  const indices = [];
  for (let i = 0; i < width * height; i += 1) if (!exclusionMask?.data[i]) indices.push(i);
  return indices;
}

function rgbaLuminance(data, offset) {
  return 0.2126 * data[offset] + 0.7152 * data[offset + 1] + 0.0722 * data[offset + 2];
}

function tieBreak(x, y, bestX, bestY) {
  const distance = Math.abs(x) + Math.abs(y); const bestDistance = Math.abs(bestX) + Math.abs(bestY);
  return distance < bestDistance || (distance === bestDistance && (y < bestY || (y === bestY && x < bestX)));
}

function cloneMask(mask) {
  return { width: mask.width, height: mask.height, data: new Uint8Array(mask.data) };
}

function assertMask(mask) {
  if (!mask || !Number.isInteger(mask.width) || !Number.isInteger(mask.height) || mask.width < 0 || mask.height < 0 || mask.data?.length !== mask.width * mask.height) throw new Error('Invalid mask');
}

function assertImage(image) {
  if (!image || !Number.isInteger(image.width) || !Number.isInteger(image.height) || image.width < 0 || image.height < 0 || image.data?.length !== image.width * image.height * 4) throw new Error('Invalid RGBA image');
}

function assertSameMaskSize(left, right) {
  assertMask(left); assertMask(right);
  if (left.width !== right.width || left.height !== right.height) throw new Error('Masks must have identical dimensions');
}

function assertSameImageAndMask(image, mask) {
  assertImage(image); assertMask(mask);
  if (image.width !== mask.width || image.height !== mask.height) throw new Error('Image and mask must have identical dimensions');
}

function assertSameSize(left, right) {
  assertImage(left); assertImage(right);
  if (left.width !== right.width || left.height !== right.height) throw new Error('Images must have identical dimensions');
}
