import { createEncryptedEnvelope, MAX_IMAGE_BYTES } from './phone-protocol.js';
import { MAX_SOURCE_IMAGE_BYTES, sanitizeImageFile } from './phone-image.js';

const SESSION_ID_BYTES = 16;
const TOKEN_BYTES = 32;
const AES_KEY_BYTES = 32;

const statusElement = document.querySelector('#status');
const drawPanel = document.querySelector('#draw-panel');
const imagePanel = document.querySelector('#image-panel');
const canvas = document.querySelector('#signature-canvas');
const expandDrawingButton = document.querySelector('#expand-drawing-button');
const clearButton = document.querySelector('#clear-button');
const sendDrawingButton = document.querySelector('#send-drawing-button');
const imageInput = document.querySelector('#image-input');
const imagePreview = document.querySelector('#image-preview');
const sendImageButton = document.querySelector('#send-image-button');

let pairing = null;
let selectedImage = null;
let selectedImageUrl = null;
let drawing = false;
let hasDrawing = false;
let resizeFrame = null;
let pendingUpload = null;
let imageSelectionRequest = 0;
let expandedDrawing = false;

function setStatus(message, kind = 'neutral') {
  statusElement.textContent = message;
  statusElement.dataset.kind = kind;
}

function base64UrlToBytes(value, expectedBytes) {
  if (!/^[A-Za-z0-9_-]+$/.test(value)) return null;
  try {
    const padded = value.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(value.length / 4) * 4, '=');
    const binary = atob(padded);
    const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
    if (bytes.byteLength !== expectedBytes) return null;
    return bytesToBase64Url(bytes) === value ? bytes : null;
  } catch {
    return null;
  }
}

function bytesToBase64Url(bytes) {
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/u, '');
}

function parsePairing() {
  const sessionMatch = /^\/s\/([A-Za-z0-9_-]+)$/u.exec(window.location.pathname);
  const fragment = window.location.hash.startsWith('#') ? window.location.hash.slice(1) : '';
  const parameters = new URLSearchParams(fragment);
  const expectedNames = ['e', 'k', 'm', 't', 'v'];
  const actualNames = [...parameters.keys()].sort();
  const hasExactNames =
    actualNames.length === expectedNames.length &&
    actualNames.every((name, index) => name === expectedNames[index]) &&
    expectedNames.every((name) => parameters.getAll(name).length === 1);

  if (!sessionMatch || !hasExactNames || window.location.search) return null;

  const sessionId = sessionMatch[1];
  const version = parameters.get('v');
  const mode = parameters.get('m');
  const expiresText = parameters.get('e');
  const token = parameters.get('t');
  const keyText = parameters.get('k');
  const sessionBytes = base64UrlToBytes(sessionId, SESSION_ID_BYTES);
  const tokenBytes = token ? base64UrlToBytes(token, TOKEN_BYTES) : null;
  const keyBytes = keyText ? base64UrlToBytes(keyText, AES_KEY_BYTES) : null;
  const expiresAt = expiresText && /^\d+$/.test(expiresText) ? Number(expiresText) : Number.NaN;

  if (
    !sessionBytes ||
    version !== '1' ||
    (mode !== 'draw' && mode !== 'image') ||
    !Number.isSafeInteger(expiresAt) ||
    expiresAt <= 0 ||
    !token ||
    !tokenBytes ||
    !keyBytes
  ) {
    return null;
  }

  return { sessionId, mode, expiresAt, token, keyBytes };
}

function removePairingFragment() {
  window.history.replaceState(null, '', window.location.pathname);
}

function configureCanvas({ preserve = false } = {}) {
  let snapshot = null;
  if (preserve && hasDrawing && canvas.width > 0 && canvas.height > 0) {
    snapshot = document.createElement('canvas');
    snapshot.width = canvas.width;
    snapshot.height = canvas.height;
    snapshot.getContext('2d').drawImage(canvas, 0, 0);
  }

  const scale = Math.max(1, window.devicePixelRatio || 1);
  const bounds = canvas.getBoundingClientRect();
  canvas.width = Math.round(bounds.width * scale);
  canvas.height = Math.round(bounds.height * scale);
  const context = canvas.getContext('2d');
  context.setTransform(scale, 0, 0, scale, 0, 0);
  context.lineCap = 'round';
  context.lineJoin = 'round';
  context.lineWidth = 2.5;
  context.strokeStyle = '#171717';
  if (snapshot) {
    context.drawImage(snapshot, 0, 0, snapshot.width, snapshot.height, 0, 0, bounds.width, bounds.height);
  }
}

function preserveDrawingOnResize() {
  if (!pairing || pairing.mode !== 'draw') return;
  if (resizeFrame !== null) cancelAnimationFrame(resizeFrame);
  resizeFrame = requestAnimationFrame(() => {
    resizeFrame = null;
    configureCanvas({ preserve: true });
  });
}

function toggleExpandedDrawing() {
  expandedDrawing = !expandedDrawing;
  document.body.classList.toggle('drawing-expanded', expandedDrawing);
  expandDrawingButton.setAttribute('aria-pressed', String(expandedDrawing));
  expandDrawingButton.textContent = expandedDrawing ? 'Exit full screen' : 'Full screen';
  preserveDrawingOnResize();
}

function pointForEvent(event) {
  const bounds = canvas.getBoundingClientRect();
  return { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
}

function startDrawing(event) {
  if (!pairing || event.button > 0) return;
  pendingUpload = null;
  drawing = true;
  hasDrawing = true;
  canvas.setPointerCapture(event.pointerId);
  const point = pointForEvent(event);
  const context = canvas.getContext('2d');
  context.beginPath();
  context.moveTo(point.x, point.y);
  event.preventDefault();
}

function continueDrawing(event) {
  if (!drawing) return;
  const point = pointForEvent(event);
  const context = canvas.getContext('2d');
  context.lineTo(point.x, point.y);
  context.stroke();
  event.preventDefault();
}

function stopDrawing(event) {
  if (!drawing) return;
  drawing = false;
  if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
}

function clearDrawing() {
  const context = canvas.getContext('2d');
  context.clearRect(0, 0, canvas.width, canvas.height);
  hasDrawing = false;
  pendingUpload = null;
  setStatus('Draw your signature, then select Send signature.');
}

function canvasToBlob() {
  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (blob) resolve(blob);
      else reject(new Error('The drawing could not be encoded.'));
    }, 'image/png');
  });
}

async function encryptPayload(blob, mode) {
  if (!pairing) throw new Error('The pairing link is invalid.');
  if (blob.size > MAX_IMAGE_BYTES) throw new Error('The image must be 1 MiB or smaller.');
  if (blob.type !== 'image/png' && blob.type !== 'image/jpeg') throw new Error('Use a PNG or JPEG image.');
  return createEncryptedEnvelope({
    rawBytes: new Uint8Array(await blob.arrayBuffer()),
    mediaType: blob.type,
    sessionId: pairing.sessionId,
    expiresAt: pairing.expiresAt,
    mode,
    keyBytes: pairing.keyBytes,
  });
}

function disableControls() {
  clearButton.disabled = true;
  sendDrawingButton.disabled = true;
  imageInput.disabled = true;
  sendImageButton.disabled = true;
}

async function sendBlob(blob, mode) {
  if (!pendingUpload) {
    setStatus('Encrypting signature…');
    pendingUpload = await encryptPayload(blob, mode);
  }
  clearButton.disabled = true;
  imageInput.disabled = true;
  setStatus('Sending encrypted signature…');
  const response = await fetch(`/api/sessions/${pairing.sessionId}/payload`, {
    method: 'PUT',
    headers: {
      Authorization: `Bearer ${pairing.token}`,
      'Content-Type': 'application/octet-stream',
    },
    body: pendingUpload.envelope,
    cache: 'no-store',
    credentials: 'same-origin',
  });

  if (response.status === 200 || response.status === 201) {
    disableControls();
    pendingUpload = null;
    pairing = null;
    setStatus('Signature sent. You can close this page.', 'success');
    return;
  }
  if (response.status === 409) throw new Error('A signature was already sent for this session.');
  if (response.status === 410) throw new Error('This pairing session expired. Start a new phone transfer on the desktop.');
  if (response.status === 401) throw new Error('This pairing link is not authorized.');
  if (response.status === 413) throw new Error('The encrypted image is too large to send.');
  throw new Error('The signature could not be sent. Try again.');
}

async function sendDrawing() {
  if (!hasDrawing) {
    setStatus('Draw a signature before sending.', 'error');
    return;
  }
  try {
    sendDrawingButton.disabled = true;
    await sendBlob(await canvasToBlob(), 'draw');
  } catch (error) {
    sendDrawingButton.disabled = false;
    setStatus(error instanceof Error ? error.message : 'The signature could not be sent.', 'error');
  }
}

async function selectImage() {
  const requestId = ++imageSelectionRequest;
  const file = imageInput.files?.[0];
  selectedImage = null;
  pendingUpload = null;
  sendImageButton.disabled = true;
  imagePreview.hidden = true;
  if (selectedImageUrl) URL.revokeObjectURL(selectedImageUrl);
  selectedImageUrl = null;
  if (!file) return;

  if (file.type !== 'image/png' && file.type !== 'image/jpeg') {
    setStatus('Choose a PNG or JPEG image.', 'error');
    imageInput.value = '';
    return;
  }
  if (file.size > MAX_SOURCE_IMAGE_BYTES) {
    setStatus('The source image must be 10 MiB or smaller.', 'error');
    imageInput.value = '';
    return;
  }

  setStatus('Preparing and sanitizing image…');
  imageInput.disabled = true;
  try {
    const sanitizedImage = await sanitizeImageFile(file);
    if (requestId !== imageSelectionRequest) return;
    selectedImage = sanitizedImage;
    selectedImageUrl = URL.createObjectURL(sanitizedImage);
    imagePreview.src = selectedImageUrl;
    imagePreview.hidden = false;
    sendImageButton.disabled = false;
    setStatus('Preview the processed image, then select Send signature.');
  } catch (error) {
    if (requestId !== imageSelectionRequest) return;
    imageInput.value = '';
    setStatus(error instanceof Error ? error.message : 'The image could not be processed.', 'error');
  } finally {
    if (requestId === imageSelectionRequest && pairing) imageInput.disabled = false;
  }
}

async function sendImage() {
  if (!selectedImage) return;
  try {
    sendImageButton.disabled = true;
    await sendBlob(selectedImage, 'image');
  } catch (error) {
    sendImageButton.disabled = false;
    setStatus(error instanceof Error ? error.message : 'The signature could not be sent.', 'error');
  }
}

function initialize() {
  pairing = parsePairing();
  removePairingFragment();
  if (!pairing) {
    disableControls();
    setStatus('This pairing link is invalid. Start a new phone transfer on the desktop.', 'error');
    return;
  }
  if (pairing.expiresAt <= Date.now()) {
    disableControls();
    pairing = null;
    setStatus('This pairing session expired. Start a new phone transfer on the desktop.', 'error');
    return;
  }

  if (pairing.mode === 'draw') {
    drawPanel.hidden = false;
    configureCanvas();
    setStatus('Draw your signature, then select Send signature.');
  } else {
    imagePanel.hidden = false;
    setStatus('Take a photo or choose a signature image.');
  }
}

canvas.addEventListener('pointerdown', startDrawing);
canvas.addEventListener('pointermove', continueDrawing);
canvas.addEventListener('pointerup', stopDrawing);
canvas.addEventListener('pointercancel', stopDrawing);
clearButton.addEventListener('click', clearDrawing);
expandDrawingButton.addEventListener('click', toggleExpandedDrawing);
sendDrawingButton.addEventListener('click', sendDrawing);
imageInput.addEventListener('change', selectImage);
sendImageButton.addEventListener('click', sendImage);
window.addEventListener('resize', preserveDrawingOnResize);

initialize();
