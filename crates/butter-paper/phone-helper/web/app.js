const canvas = document.querySelector('canvas');
const status = document.querySelector('#status');
const send = document.querySelector('#send');
const clear = document.querySelector('#clear');
const pad = new SignaturePad(canvas, { penColor: '#171717', minWidth: 0.8, maxWidth: 2.5 });
const imageMode = document.body.dataset.mode === 'image';
const file = document.querySelector('#file');
let selectedImage = null;
let finished = false;
let busy = false;
let previous = null;
function resize() {
  const points = pad.toData();
  const bounds = canvas.getBoundingClientRect();
  // Keep capture quality independent of the display pixel ratio.
  const ratio = 2048 / Math.max(bounds.width, bounds.height);
  if (previous) for (const group of points) for (const point of group.points) { point.x *= bounds.width / previous.width; point.y *= bounds.height / previous.height; }
  canvas.width = Math.round(bounds.width * ratio);
  canvas.height = Math.round(bounds.height * ratio);
  canvas.getContext('2d').setTransform(ratio, 0, 0, ratio, 0, 0);
  pad.clear(); pad.fromData(points); previous = bounds;
  if (selectedImage) {
    const scale = Math.min(bounds.width / selectedImage.width, bounds.height / selectedImage.height);
    canvas.getContext('2d').drawImage(selectedImage, (bounds.width - selectedImage.width * scale) / 2, (bounds.height - selectedImage.height * scale) / 2, selectedImage.width * scale, selectedImage.height * scale);
  }
}
pad.addEventListener('beginStroke', () => { status.textContent = ''; });
resize(); window.addEventListener('resize', resize);
if (imageMode) { pad.off(); file.hidden = false; document.querySelector('h1').textContent = 'Choose image'; canvas.setAttribute('aria-label', 'Signature preview'); }
file.addEventListener('change', async () => {
  if (!file.files[0] || busy || finished) return;
  busy = true; send.disabled = clear.disabled = file.disabled = true;
  const url = URL.createObjectURL(file.files[0]);
  try {
    const img = new Image(); img.src = url; await img.decode();
    if (img.width * img.height > 32 * 1024 * 1024) throw new Error('Choose a smaller image.');
    selectedImage = img; resize(); status.textContent = '';
  } catch (_) { status.textContent = 'Cannot open this image.'; }
  finally { URL.revokeObjectURL(url); busy = false; send.disabled = clear.disabled = file.disabled = false; }
});
clear.addEventListener('click', () => { selectedImage = null; file.value = ''; pad.clear(); status.textContent = ''; });
send.addEventListener('click', async () => {
  if (busy || finished) return;
  if (imageMode ? !selectedImage : pad.isEmpty()) { status.textContent = imageMode ? 'Choose an image.' : 'Draw a signature.'; return; }
  busy = true; pad.off(); send.disabled = clear.disabled = file.disabled = true; status.textContent = 'Sending…';
  try {
    const png = await new Promise(resolve => canvas.toBlob(resolve, 'image/png'));
    if (!png || png.size > 1024 * 1024) throw new Error('Image too large. Try again.');
    const response = await fetch(location.pathname, { method: 'POST', headers: { 'Content-Type': 'image/png' }, body: png, cache: 'no-store' });
    if (!response.ok) throw new Error('Session ended. Try again on your computer.');
    finished = true; selectedImage = null; file.value = ''; pad.clear(); status.textContent = 'Sent';
  } catch (error) { status.textContent = error.message || 'Cannot send. Try again.'; }
  finally { busy = false; if (!finished) { if (!imageMode) pad.on(); send.disabled = clear.disabled = file.disabled = false; } }
});
window.addEventListener('pagehide', () => { pad.off(); pad.clear(); });
