// @vitest-environment jsdom

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { dismissToolShortcutPopup } from '../utils/toolShortcuts';
import { SignatureMenu } from './SignatureMenu';

const signaturePadState = vi.hoisted(() => ({
  empty: true,
  endStroke: null as null | (() => void),
  pointGroups: [] as Array<{ points: Array<{ x: number; y: number; pressure: number; time: number }> }>,
  restoredPointGroups: [] as Array<{ points: Array<{ x: number; y: number; pressure: number; time: number }> }>,
}));

vi.mock('signature_pad', () => ({
  default: class MockSignaturePad {
    constructor() {
      signaturePadState.empty = true;
    }

    addEventListener(_name: string, listener: () => void) { signaturePadState.endStroke = listener; }
    removeEventListener() { signaturePadState.endStroke = null; }
    isEmpty() { return signaturePadState.empty; }
    toDataURL() { return 'data:image/png;base64,iVBORw0KGgo='; }
    toData() { return signaturePadState.pointGroups; }
    fromData(pointGroups: typeof signaturePadState.pointGroups) {
      signaturePadState.pointGroups = pointGroups;
      signaturePadState.restoredPointGroups = pointGroups;
      signaturePadState.empty = pointGroups.length === 0;
    }
    clear() {
      signaturePadState.empty = true;
      signaturePadState.pointGroups = [];
    }
    off() {}
  },
}));

class TestResizeObserver {
  static instances: TestResizeObserver[] = [];
  readonly targets: Element[] = [];

  constructor(private readonly callback: ResizeObserverCallback) {
    TestResizeObserver.instances.push(this);
  }

  observe(target: Element) { this.targets.push(target); }
  unobserve() {}
  disconnect() {}
  trigger() { this.callback([], this); }
}

class TestImage {
  naturalWidth = 480;
  naturalHeight = 160;
  width = 480;
  height = 160;
  private listeners = new Map<string, () => void>();

  addEventListener(name: string, listener: () => void) { this.listeners.set(name, listener); }
  set src(_value: string) { queueMicrotask(() => this.listeners.get('load')?.()); }
}

class OversizedTestImage extends TestImage {
  override naturalWidth = 8192;
  override naturalHeight = 8192;
  override width = 8192;
  override height = 8192;
}

class TestFileReader {
  result: string | ArrayBuffer | null = null;
  private listeners = new Map<string, () => void>();

  addEventListener(name: string, listener: () => void) { this.listeners.set(name, listener); }
  readAsDataURL(file: File) {
    const payload = file.type === 'image/jpeg'
      ? '/9j/wAARCACgAeADAREAAhEAAxEA/9k='
      : file.name.includes('unsafe-dimensions')
        ? 'iVBORw0KGgoAAAANSUhEUgAAIAAAACAA'
        : 'iVBORw0KGgoAAAANSUhEUgAAAeAAAACg';
    this.result = `data:${file.type};base64,${payload}`;
    queueMicrotask(() => this.listeners.get('load')?.());
  }
}

class ControlledFileReader extends TestFileReader {
  static pending: ControlledFileReader[] = [];

  override readAsDataURL(file: File) {
    this.result = `data:${file.type};base64,iVBORw0KGgoAAAANSUhEUgAAAeAAAACg`;
    ControlledFileReader.pending.push(this);
  }

  emitLoad() {
    const listeners = (this as unknown as { listeners: Map<string, () => void> }).listeners;
    listeners.get('load')?.();
  }
}

describe('SignatureMenu', () => {
  let host: HTMLDivElement;
  let root: Root;
  let getUserMedia: ReturnType<typeof vi.fn>;
  let originalMediaDevices: PropertyDescriptor | undefined;
  let canvasContext: CanvasRenderingContext2D;
  let canvasOffsetWidth: number;
  let canvasOffsetHeight: number;
  let originalButterPaper: PropertyDescriptor | undefined;
  let originalGetAnimations: PropertyDescriptor | undefined;
  let phoneStart: ReturnType<typeof vi.fn>;
  let phonePoll: ReturnType<typeof vi.fn>;
  let phoneStop: ReturnType<typeof vi.fn>;
  let recentList: ReturnType<typeof vi.fn>;
  let recentRemember: ReturnType<typeof vi.fn>;
  let recentRemove: ReturnType<typeof vi.fn>;
  let recentClear: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.stubGlobal('ResizeObserver', TestResizeObserver);
    vi.stubGlobal('Image', TestImage);
    vi.stubGlobal('FileReader', TestFileReader);
    vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true);
    vi.stubGlobal('devicePixelRatio', 2);
    originalGetAnimations = Object.getOwnPropertyDescriptor(Element.prototype, 'getAnimations');
    Object.defineProperty(Element.prototype, 'getAnimations', {
      configurable: true,
      value: vi.fn(() => []),
    });
    canvasOffsetWidth = 360;
    canvasOffsetHeight = 135;
    originalMediaDevices = Object.getOwnPropertyDescriptor(navigator, 'mediaDevices');
    getUserMedia = vi.fn();
    phoneStart = vi.fn().mockImplementation(async (mode: 'draw' | 'image') => ({
      id: 'EREREREREREREREREREREQ',
      qrDataUrl: 'data:image/png;base64,qr-code',
      expiresAt: Date.now() + 300_000,
      mode,
    }));
    phonePoll = vi.fn().mockResolvedValue({ status: 'waiting' });
    phoneStop = vi.fn().mockResolvedValue(undefined);
    recentList = vi.fn().mockResolvedValue({ available: true, signatures: [] });
    recentRemember = vi.fn().mockResolvedValue({ available: true, signatures: [] });
    recentRemove = vi.fn().mockResolvedValue({ available: true, signatures: [] });
    recentClear = vi.fn().mockResolvedValue({ available: true, signatures: [] });
    originalButterPaper = Object.getOwnPropertyDescriptor(window, 'butterPaper');
    Object.defineProperty(window, 'butterPaper', {
      configurable: true,
      value: {
        signaturePhone: { start: phoneStart, poll: phonePoll, stop: phoneStop },
        signatureRecent: {
          list: recentList,
          remember: recentRemember,
          remove: recentRemove,
          clear: recentClear,
        },
      },
    });
    Object.defineProperty(navigator, 'mediaDevices', {
      configurable: true,
      value: { getUserMedia },
    });
    canvasContext = {
      clearRect: vi.fn(),
      drawImage: vi.fn(),
      getImageData: vi.fn((_x: number, _y: number, width: number, height: number) => {
        const data = new Uint8ClampedArray(width * height * 4);
        for (let offset = 0; offset < data.length; offset += 4) data.set([255, 255, 255, 255], offset);
        for (let row = Math.floor(height * 0.35); row < Math.ceil(height * 0.65); row += 1) {
          for (let column = Math.floor(width * 0.25); column < Math.ceil(width * 0.75); column += 1) {
            data.set([0, 0, 0, 255], (row * width + column) * 4);
          }
        }
        return { data, width, height } as ImageData;
      }),
      createImageData: vi.fn((width: number, height: number) => ({
        data: new Uint8ClampedArray(width * height * 4),
        width,
        height,
      }) as ImageData),
      putImageData: vi.fn(),
      fillText: vi.fn(),
      measureText: vi.fn(() => ({ width: 300 })),
      scale: vi.fn(),
      fillStyle: '',
      font: '',
      textAlign: 'start',
      textBaseline: 'alphabetic',
    } as unknown as CanvasRenderingContext2D;
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(canvasContext);
    vi.spyOn(HTMLCanvasElement.prototype, 'offsetWidth', 'get').mockImplementation(() => canvasOffsetWidth);
    vi.spyOn(HTMLCanvasElement.prototype, 'offsetHeight', 'get').mockImplementation(() => canvasOffsetHeight);
    vi.spyOn(HTMLCanvasElement.prototype, 'toDataURL').mockReturnValue('data:image/png;base64,iVBORw0KGgo=');
    host = document.createElement('div');
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    document.querySelectorAll('[data-slot="popover-content"]').forEach((element) => element.remove());
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    if (originalMediaDevices) Object.defineProperty(navigator, 'mediaDevices', originalMediaDevices);
    else Reflect.deleteProperty(navigator, 'mediaDevices');
    if (originalButterPaper) Object.defineProperty(window, 'butterPaper', originalButterPaper);
    else Reflect.deleteProperty(window, 'butterPaper');
    if (originalGetAnimations) Object.defineProperty(Element.prototype, 'getAnimations', originalGetAnimations);
    else Reflect.deleteProperty(Element.prototype, 'getAnimations');
    signaturePadState.empty = true;
    signaturePadState.endStroke = null;
    signaturePadState.pointGroups = [];
    signaturePadState.restoredPointGroups = [];
    TestResizeObserver.instances = [];
    ControlledFileReader.pending = [];
  });

  it('keeps drawing coordinates and strokes aligned after a responsive resize', async () => {
    act(() => root.render(createElement(SignatureMenu, { onUseSignature: vi.fn() })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    await act(async () => { await Promise.resolve(); });
    const canvas = document.querySelector<HTMLCanvasElement>('[data-testid="signature-draw-canvas"]');
    signaturePadState.empty = false;
    signaturePadState.pointGroups = [{
      points: [{ x: 180, y: 67.5, pressure: 0.5, time: 1 }],
    }];

    canvasOffsetWidth = 240;
    canvasOffsetHeight = 90;
    act(() => TestResizeObserver.instances.find((observer) => observer.targets.includes(canvas!))?.trigger());

    expect(canvas?.width).toBe(480);
    expect(canvas?.height).toBe(180);
    expect(signaturePadState.restoredPointGroups[0]?.points[0]).toMatchObject({ x: 120, y: 45 });
  });

  it('creates a drawn signature', async () => {
    const onUseSignature = vi.fn();
    act(() => root.render(createElement(SignatureMenu, { onUseSignature })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    await act(async () => { await Promise.resolve(); });

    expect(document.body.textContent).not.toContain('It does not verify identity or detect later changes.');
    expect(document.querySelector('[data-testid="signature-popover"]')?.className).toContain('overflow-y-auto');
    const modeControl = document.querySelector('[data-slot="toggle-group"]');
    expect(modeControl?.getAttribute('data-variant')).toBe('outline');
    expect(modeControl?.getAttribute('data-spacing')).toBe('0');
    const drawingCanvas = document.querySelector<HTMLCanvasElement>('[data-testid="signature-draw-canvas"]');
    expect(drawingCanvas?.width).toBe(720);
    expect(drawingCanvas?.height).toBe(270);
    expect(canvasContext.scale).toHaveBeenCalledWith(2, 2);
    const drawPanel = document.querySelector<HTMLElement>('[data-testid="signature-draw-canvas"]')
      ?.closest<HTMLElement>('[data-testid="signature-mode-panel"]') ?? null;
    expect(findButton(drawPanel, 'Computer')?.getAttribute('aria-pressed')).toBe('true');
    expect(findButton(drawPanel, 'Phone')).toBeTruthy();
    let useButton = findButton(drawPanel, 'Add signature');
    expect(useButton?.disabled).toBe(true);
    expect(signaturePadState.endStroke).toBeTypeOf('function');

    act(() => {
      signaturePadState.empty = false;
      signaturePadState.endStroke?.();
    });
    useButton = findButton(drawPanel, 'Add signature');
    expect(useButton?.disabled).toBe(false);
    const clearButton = findButton(drawPanel, 'Clear');
    act(() => clearButton?.click());
    expect(findButton(drawPanel, 'Add signature')?.disabled).toBe(true);
    act(() => {
      signaturePadState.empty = false;
      signaturePadState.endStroke?.();
    });
    useButton = findButton(drawPanel, 'Add signature');
    act(() => useButton?.click());

    expect(onUseSignature).toHaveBeenCalledWith(expect.objectContaining({
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      mimeType: 'image/png',
      width: 720,
      height: 270,
      source: 'drawn',
    }));
  });

  it('renders a typed name as a transparent PNG signature asset', async () => {
    const onUseSignature = vi.fn();
    act(() => root.render(createElement(SignatureMenu, { onUseSignature })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    const typeTab = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-slot="toggle-group-item"]'))
      .find((button) => button.textContent?.includes('Type'));
    act(() => typeTab?.click());

    const input = document.querySelector<HTMLInputElement>('#signature-name');
    expect(input).toBeTruthy();
    act(() => {
      setNativeInputValue(input!, 'Alex Potenza');
      input?.dispatchEvent(new Event('input', { bubbles: true }));
    });
    expect(document.querySelector('[data-testid="signature-type-preview"]')?.textContent).toContain('Alex Potenza');
    expect((document.querySelector('[data-testid="signature-type-preview"]') as HTMLElement | null)?.style.fontFamily).toContain('Allura');

    const useButton = findButton(activePanel(), 'Add signature');
    await act(async () => { useButton?.click(); });
    expect(onUseSignature).toHaveBeenCalledWith(expect.objectContaining({
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      mimeType: 'image/png',
      source: 'typed',
    }));
  });

  it('imports a PNG as a visual signature asset', async () => {
    const onUseSignature = vi.fn();
    act(() => root.render(createElement(SignatureMenu, { onUseSignature })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    const imageTab = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-slot="toggle-group-item"]'))
      .find((button) => button.textContent?.includes('Image'));
    act(() => imageTab?.click());

    const input = document.querySelector<HTMLInputElement>('#signature-image');
    const sourceActions = document.querySelector<HTMLElement>('[data-testid="signature-image-source-actions"]');
    expect(sourceActions?.className).toContain('grid-cols-3');
    expect(sourceActions?.getAttribute('data-slot')).toBe('toggle-group');
    expect(sourceActions?.getAttribute('data-variant')).toBe('outline');
    expect(sourceActions?.getAttribute('data-spacing')).toBe('0');
    const sourceOptions = sourceActions?.querySelectorAll<HTMLButtonElement>('[data-slot="toggle-group-item"]');
    expect(sourceOptions).toHaveLength(3);
    expect(Array.from(sourceOptions ?? []).every((button) => button.getAttribute('aria-pressed') === 'false')).toBe(true);
    expect(findButton(sourceActions, 'Webcam')).toBeTruthy();
    expect(findButton(sourceActions, 'Phone')).toBeTruthy();
    const file = new File(['signature'], 'signature.png', { type: 'image/png' });
    Object.defineProperty(input, 'files', { configurable: true, value: [file] });
    await act(async () => {
      input?.dispatchEvent(new Event('change', { bubbles: true }));
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(document.querySelector('[data-testid="signature-image-preview"]')).toBeTruthy();
    expect(findButton(sourceActions, 'Choose file')?.getAttribute('aria-pressed')).toBe('true');
    const useButton = findButton(activePanel(), 'Add signature');
    act(() => useButton?.click());
    expect(onUseSignature).toHaveBeenCalledWith(expect.objectContaining({
      mimeType: 'image/png',
      width: expect.any(Number),
      height: expect.any(Number),
      source: 'image',
    }));
  });

  it('sanitizes an imported JPEG into a fresh PNG asset', async () => {
    const onUseSignature = vi.fn();
    act(() => root.render(createElement(SignatureMenu, { onUseSignature })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    const imageTab = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-slot="toggle-group-item"]'))
      .find((button) => button.textContent?.includes('Image'));
    act(() => imageTab?.click());
    const input = document.querySelector<HTMLInputElement>('#signature-image');
    Object.defineProperty(input, 'files', {
      configurable: true,
      value: [new File(['camera metadata'], 'camera.jpg', { type: 'image/jpeg' })],
    });

    await act(async () => {
      input?.dispatchEvent(new Event('change', { bubbles: true }));
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    act(() => findButton(activePanel(), 'Add signature')?.click());
    expect(onUseSignature).toHaveBeenCalledWith(expect.objectContaining({
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      mimeType: 'image/png',
      source: 'image',
    }));
  });

  it('captures a local camera frame as a sanitized PNG and stops the camera', async () => {
    const stopTrack = vi.fn();
    const stream = { getTracks: () => [{ stop: stopTrack }] } as unknown as MediaStream;
    getUserMedia.mockResolvedValue(stream);
    const onUseSignature = vi.fn();
    act(() => root.render(createElement(SignatureMenu, { onUseSignature })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    const imageTab = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-slot="toggle-group-item"]'))
      .find((button) => button.textContent?.includes('Image'));
    act(() => imageTab?.click());

    await act(async () => {
      findButton(activePanel(), 'Webcam')?.click();
      await Promise.resolve();
    });
    expect(getUserMedia).toHaveBeenCalledWith({ video: true, audio: false });
    const video = document.querySelector<HTMLVideoElement>('[data-testid="signature-camera-preview"]');
    expect(video?.srcObject).toBe(stream);
    const sourceActions = document.querySelector('[data-testid="signature-image-source-actions"]');
    expect(sourceActions).toBeTruthy();
    expect(sourceActions?.compareDocumentPosition(findButton(activePanel(), 'Take photo')!))
      .toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(findButton(activePanel(), 'Take photo')?.compareDocumentPosition(video!))
      .toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    Object.defineProperty(video, 'videoWidth', { configurable: true, value: 640 });
    Object.defineProperty(video, 'videoHeight', { configurable: true, value: 480 });
    act(() => video?.dispatchEvent(new Event('canplay')));
    expect(activePanel()?.textContent).toContain('Use dark ink on white paper');
    expect(activePanel()?.textContent).toContain('Processed preview');
    const closeWebcamButton = findButtonByLabel(activePanel(), 'Close webcam');
    expect(closeWebcamButton).toBeTruthy();
    expect(closeWebcamButton?.parentElement?.className).toContain('justify-between');
    expect(findButton(activePanel(), 'Cancel')).toBeUndefined();
    expect(document.querySelector('[data-testid="signature-camera-processed-preview"]')).toBeTruthy();
    act(() => findButton(activePanel(), 'Take photo')?.click());

    expect(stopTrack).toHaveBeenCalledOnce();
    expect(document.querySelector('[data-testid="signature-camera-preview"]')).toBeNull();
    expect(document.querySelector('[data-testid="signature-image-preview"]')).toBeTruthy();
    act(() => findButton(activePanel(), 'Add signature')?.click());
    expect(onUseSignature).toHaveBeenCalledWith(expect.objectContaining({
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      mimeType: 'image/png',
      width: expect.any(Number),
      height: expect.any(Number),
      source: 'image',
    }));
  });

  it('stops an active camera when the user leaves the Image tab', async () => {
    const stopTrack = vi.fn();
    const stream = { getTracks: () => [{ stop: stopTrack }] } as unknown as MediaStream;
    getUserMedia.mockResolvedValue(stream);
    act(() => root.render(createElement(SignatureMenu, { onUseSignature: vi.fn() })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    const tabs = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-slot="toggle-group-item"]'));
    act(() => tabs.find((button) => button.textContent?.includes('Image'))?.click());
    await act(async () => {
      findButton(activePanel(), 'Webcam')?.click();
      await Promise.resolve();
    });

    act(() => tabs.find((button) => button.textContent?.includes('Draw'))?.click());
    expect(stopTrack).toHaveBeenCalledOnce();
  });

  it('stops an active camera from its top-right close control', async () => {
    const stopTrack = vi.fn();
    const stream = { getTracks: () => [{ stop: stopTrack }] } as unknown as MediaStream;
    getUserMedia.mockResolvedValue(stream);
    act(() => root.render(createElement(SignatureMenu, { onUseSignature: vi.fn() })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    const tabs = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-slot="toggle-group-item"]'));
    act(() => tabs.find((button) => button.textContent?.includes('Image'))?.click());
    await act(async () => {
      findButton(activePanel(), 'Webcam')?.click();
      await Promise.resolve();
    });

    act(() => findButtonByLabel(activePanel(), 'Close webcam')?.click());
    expect(stopTrack).toHaveBeenCalledOnce();
    expect(document.querySelector('[data-testid="signature-camera-preview"]')).toBeNull();
  });

  it('starts and closes a one-time phone signature transfer', async () => {
    act(() => root.render(createElement(SignatureMenu, { onUseSignature: vi.fn() })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    const imageTab = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-slot="toggle-group-item"]'))
      .find((button) => button.textContent?.includes('Image'));
    act(() => imageTab?.click());

    await act(async () => {
      findButton(activePanel(), 'Phone')?.click();
      await Promise.resolve();
    });

    expect(phoneStart).toHaveBeenCalledWith('image');
    expect(document.querySelector<HTMLImageElement>('[data-testid="signature-phone-qr"]')?.src)
      .toBe('data:image/png;base64,qr-code');
    expect(document.querySelector('[data-testid="signature-image-source-actions"]')).toBeTruthy();
    expect(activePanel()?.textContent).toContain('Expires in 5 minutes');
    expect(findButton(activePanel(), 'Cancel')).toBeUndefined();
    const closePhoneButton = findButtonByLabel(activePanel(), 'Close phone transfer');
    expect(closePhoneButton?.parentElement?.className).toContain('justify-end');
    act(() => closePhoneButton?.click());
    expect(phoneStop).toHaveBeenCalledWith('EREREREREREREREREREREQ');
    expect(document.querySelector('[data-testid="signature-phone-qr"]')).toBeNull();
  });

  it('imports and previews an image returned from the phone before use', async () => {
    const onUseSignature = vi.fn();
    phonePoll.mockResolvedValueOnce({
      status: 'received',
      image: {
        dataUrl: 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAeAAAACg',
        mimeType: 'image/png',
        mode: 'image',
      },
    });
    act(() => root.render(createElement(SignatureMenu, { onUseSignature })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    const imageTab = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-slot="toggle-group-item"]'))
      .find((button) => button.textContent?.includes('Image'));
    act(() => imageTab?.click());
    await act(async () => {
      findButton(activePanel(), 'Phone')?.click();
      await Promise.resolve();
    });

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 300));
      await Promise.resolve();
    });
    expect(document.querySelector('[data-testid="signature-image-preview"]')).toBeTruthy();
    act(() => findButton(activePanel(), 'Add signature')?.click());
    expect(onUseSignature).toHaveBeenCalledWith(expect.objectContaining({
      mimeType: 'image/png',
      source: 'image',
    }));
  });

  it('starts the phone in drawing mode and imports the returned drawing', async () => {
    const onUseSignature = vi.fn();
    phonePoll.mockResolvedValueOnce({
      status: 'received',
      image: {
        dataUrl: 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAeAAAACg',
        mimeType: 'image/png',
        mode: 'draw',
      },
    });
    act(() => root.render(createElement(SignatureMenu, { onUseSignature })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());

    await act(async () => {
      signaturePadState.empty = false;
      signaturePadState.endStroke?.();
      findButton(activePanel(), 'Phone')?.click();
      await Promise.resolve();
    });
    expect(phoneStart).toHaveBeenCalledWith('draw');
    expect(document.querySelector('[data-testid="signature-phone-qr"]')).toBeTruthy();
    expect(document.querySelector('[data-testid="signature-draw-canvas"]')).toBeNull();
    expect(findButton(activePanel(), 'Clear')).toBeUndefined();
    expect(findButton(activePanel(), 'Computer')).toBeTruthy();
    expect(findButton(activePanel(), 'Phone')?.getAttribute('aria-pressed')).toBe('true');
    expect(signaturePadState.empty).toBe(true);

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 300));
      await Promise.resolve();
    });
    expect(document.querySelector('[data-testid="signature-draw-phone-preview"]')).toBeTruthy();
    expect(document.querySelector('[data-testid="signature-draw-canvas"]')).toBeNull();
    const phoneAddButton = Array.from(activePanel()?.querySelectorAll<HTMLButtonElement>('button') ?? [])
      .find((button) => button.textContent?.includes('Add signature') && !button.disabled);
    act(() => phoneAddButton?.click());
    expect(onUseSignature).toHaveBeenCalledWith(expect.objectContaining({
      mimeType: 'image/png',
      source: 'drawn',
    }));
  });

  it('ignores an older file import after camera capture starts', async () => {
    vi.stubGlobal('FileReader', ControlledFileReader);
    const stopTrack = vi.fn();
    getUserMedia.mockResolvedValue({ getTracks: () => [{ stop: stopTrack }] } as unknown as MediaStream);
    act(() => root.render(createElement(SignatureMenu, { onUseSignature: vi.fn() })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    const imageTab = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-slot="toggle-group-item"]'))
      .find((button) => button.textContent?.includes('Image'));
    act(() => imageTab?.click());
    const input = document.querySelector<HTMLInputElement>('#signature-image');
    Object.defineProperty(input, 'files', {
      configurable: true,
      value: [new File(['older'], 'older.png', { type: 'image/png' })],
    });
    act(() => input?.dispatchEvent(new Event('change', { bubbles: true })));

    await act(async () => {
      findButton(activePanel(), 'Webcam')?.click();
      await Promise.resolve();
    });
    await act(async () => {
      ControlledFileReader.pending[0]?.emitLoad();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(document.querySelector('[data-testid="signature-camera-preview"]')).toBeTruthy();
    expect(document.querySelector('[data-testid="signature-image-preview"]')).toBeNull();
  });

  it('rejects unsupported and oversized signature images', async () => {
    const onUseSignature = vi.fn();
    act(() => root.render(createElement(SignatureMenu, { onUseSignature })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    const imageTab = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-slot="toggle-group-item"]'))
      .find((button) => button.textContent?.includes('Image'));
    act(() => imageTab?.click());

    const input = document.querySelector<HTMLInputElement>('#signature-image');
    const unsupported = new File(['signature'], 'signature.gif', { type: 'image/gif' });
    Object.defineProperty(input, 'files', { configurable: true, value: [unsupported] });
    await act(async () => { input?.dispatchEvent(new Event('change', { bubbles: true })); });
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('PNG or JPEG');

    const oversized = new File([], 'signature.png', { type: 'image/png' });
    Object.defineProperty(oversized, 'size', { configurable: true, value: 10 * 1024 * 1024 + 1 });
    Object.defineProperty(input, 'files', { configurable: true, value: [oversized] });
    await act(async () => { input?.dispatchEvent(new Event('change', { bubbles: true })); });
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('smaller than 10 MB');
    expect(onUseSignature).not.toHaveBeenCalled();
  });

  it('ignores an older image read after a newer selection starts', async () => {
    vi.stubGlobal('FileReader', ControlledFileReader);
    const onUseSignature = vi.fn();
    act(() => root.render(createElement(SignatureMenu, { onUseSignature })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    const imageTab = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-slot="toggle-group-item"]'))
      .find((button) => button.textContent?.includes('Image'));
    act(() => imageTab?.click());
    const input = document.querySelector<HTMLInputElement>('#signature-image');

    for (const name of ['older.png', 'newer.png']) {
      Object.defineProperty(input, 'files', {
        configurable: true,
        value: [new File([name], name, { type: 'image/png' })],
      });
      act(() => input?.dispatchEvent(new Event('change', { bubbles: true })));
    }
    expect(ControlledFileReader.pending).toHaveLength(2);

    await act(async () => {
      ControlledFileReader.pending[1]?.emitLoad();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    await act(async () => {
      ControlledFileReader.pending[0]?.emitLoad();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(document.querySelector('[data-testid="signature-image-preview"]')).toBeTruthy();
    expect(HTMLCanvasElement.prototype.toDataURL).toHaveBeenCalledTimes(1);
  });

  it('rejects an image with unsafe decoded dimensions', async () => {
    vi.stubGlobal('Image', OversizedTestImage);
    act(() => root.render(createElement(SignatureMenu, { onUseSignature: vi.fn() })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    const imageTab = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-slot="toggle-group-item"]'))
      .find((button) => button.textContent?.includes('Image'));
    act(() => imageTab?.click());
    const input = document.querySelector<HTMLInputElement>('#signature-image');
    Object.defineProperty(input, 'files', {
      configurable: true,
      value: [new File(['signature'], 'unsafe-dimensions.png', { type: 'image/png' })],
    });

    await act(async () => {
      input?.dispatchEvent(new Event('change', { bubbles: true }));
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('4096 × 4096');
    expect(document.querySelector('[data-testid="signature-image-preview"]')).toBeNull();
  });

  it('shows and reuses recent signatures without blocking placement when storage fails', async () => {
    const onUseSignature = vi.fn();
    const recent = recentSignature('a');
    recentList.mockResolvedValue({ available: true, signatures: [recent] });
    recentRemember.mockRejectedValue(new Error('secure storage failed'));
    act(() => root.render(createElement(SignatureMenu, { onUseSignature })));

    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    await act(async () => { await Promise.resolve(); });

    expect(document.querySelector('[data-testid="recent-signatures"]')).toBeTruthy();
    expect(document.querySelector('[data-testid="recent-signatures-scroll-area"]')).toBeNull();
    await act(async () => {
      document.querySelector<HTMLButtonElement>('[aria-label="Use recent drawn signature 1"]')?.click();
      await Promise.resolve();
    });
    expect(onUseSignature).toHaveBeenCalledWith(recent.asset);
    expect(recentRemember).toHaveBeenCalledWith(recent.asset);
    expect(document.querySelector('[data-testid="signature-popover"][data-open]')).toBeNull();
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    await act(async () => { await Promise.resolve(); });
    expect(document.body.textContent).toContain('could not be saved to Recent');
  });

  it('sorts recent signature rows newest first and reveals the Add new form', async () => {
    const oldest = recentSignature('a', 100);
    const newest = recentSignature('b', 500);
    const middle = recentSignature('c', 200);
    const newer = recentSignature('d', 300);
    const secondNewest = recentSignature('e', 400);
    recentList.mockResolvedValue({
      available: true,
      signatures: [oldest, newest, middle, newer, secondNewest],
    });
    act(() => root.render(createElement(SignatureMenu, { onUseSignature: vi.fn() })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    await act(async () => { await Promise.resolve(); });

    const list = document.querySelector('[data-testid="recent-signatures"]');
    expect(list?.getAttribute('role')).toBe('list');
    expect(list?.className).toContain('grid-cols-2');
    expect(document.querySelector('[data-testid="recent-signatures-scroll-area"]')?.getAttribute('data-slot')).toBe('scroll-area');
    const rows = Array.from(list?.querySelectorAll('[role="listitem"]') ?? []);
    expect(rows.map((row) => (
      row.getAttribute('data-recent-signature-id')
    ))).toEqual([newest.id, secondNewest.id, newer.id, middle.id, oldest.id]);
    expect(rows[0]?.querySelector('[data-slot="item"]')?.tagName).toBe('BUTTON');
    expect(rows[0]?.querySelector('[data-slot="item"]')?.className).toContain('h-24');
    expect(rows[0]?.querySelector('[data-slot="item"] [data-slot="popover-trigger"]')).toBeNull();
    expect(rows[0]?.querySelector('[data-slot="item-actions"] [data-slot="popover-trigger"]')).toBeTruthy();
    expect(rows[0]?.className).toContain('group/signature');
    expect(rows[0]?.querySelector('[data-slot="item-actions"]')?.className).toContain('opacity-0');
    expect(rows[0]?.querySelector('[data-slot="item-actions"]')?.className).toContain('group-hover/signature:opacity-100');
    expect(rows[0]?.querySelector('[data-slot="item-actions"]')?.className).toContain('group-focus-within/signature:opacity-100');
    expect(list?.textContent).toBe('');
    expect(document.body.textContent).not.toContain('Recent');
    expect(document.querySelector('[data-slot="popover-title"]')?.className).not.toContain('text-center');
    expect(document.querySelector('[aria-label="Signature input method"]')).toBeNull();

    expect(findButton(document.querySelector('[data-testid="signature-popover"]'), 'Add new signature')).toBeTruthy();
    act(() => findButton(document.querySelector('[data-testid="signature-popover"]'), 'Add new signature')?.click());
    expect(document.querySelector('[aria-label="Signature input method"]')).toBeTruthy();
    const closeEditorButton = findButtonByLabel(document.querySelector('[data-testid="signature-popover"]'), 'Close new signature editor');
    expect(closeEditorButton).toBeTruthy();
    expect(closeEditorButton?.parentElement?.className).toContain('justify-between');
    expect(findButton(document.querySelector('[data-testid="signature-popover"]'), 'Cancel')).toBeUndefined();
    expect(findButton(document.querySelector('[data-testid="signature-popover"]'), 'Clear all')).toBeUndefined();
  });

  it('requires confirmation before removing an individual recent signature', async () => {
    const first = recentSignature('a', 100);
    const second = recentSignature('b', 200);
    recentList.mockResolvedValue({ available: true, signatures: [first, second] });
    recentRemove.mockResolvedValue({ available: true, signatures: [first] });
    act(() => root.render(createElement(SignatureMenu, { onUseSignature: vi.fn() })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    await act(async () => { await Promise.resolve(); });

    act(() => {
      document.querySelector<HTMLButtonElement>('[aria-label="Remove recent drawn signature 1"]')?.click();
    });
    expect(recentRemove).not.toHaveBeenCalled();
    const confirmation = document.querySelector('[data-testid="confirmation-popover"]');
    expect(confirmation?.querySelector('[data-slot="popover-title"]')?.textContent).toBe('Delete this signature?');
    expect(document.querySelector('[data-slot="alert-dialog-overlay"]')).toBeNull();

    act(() => findButton(document.body, 'Cancel')?.click());
    expect(recentRemove).not.toHaveBeenCalled();
    act(() => {
      document.querySelector<HTMLButtonElement>('[aria-label="Remove recent drawn signature 1"]')?.click();
    });
    await act(async () => {
      findButton(document.body, 'Delete')?.click();
      await Promise.resolve();
    });
    expect(recentRemove).toHaveBeenCalledWith(second.id);
    expect(document.querySelectorAll('[aria-label^="Use recent drawn signature"]')).toHaveLength(1);
    expect(document.activeElement?.getAttribute('aria-label')).toBe('Use recent drawn signature 1');
  });

  it('offers deletion from the recent signature context menu', async () => {
    const recent = recentSignature('a');
    recentList.mockResolvedValue({ available: true, signatures: [recent] });
    recentRemove.mockResolvedValue({ available: true, signatures: [] });
    act(() => root.render(createElement(SignatureMenu, { onUseSignature: vi.fn() })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    await act(async () => { await Promise.resolve(); });

    const row = document.querySelector<HTMLElement>('[data-recent-signature-id]');
    await act(async () => {
      row?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      await Promise.resolve();
    });
    const contextDelete = Array.from(document.querySelectorAll<HTMLElement>('[data-slot="context-menu-item"]'))
      .find((item) => item.textContent === 'Delete');
    expect(contextDelete?.getAttribute('data-variant')).toBe('destructive');

    act(() => contextDelete?.click());
    expect(recentRemove).not.toHaveBeenCalled();
    expect(document.querySelector('[data-testid="confirmation-popover"]')).toBeTruthy();
    await act(async () => {
      findButton(document.body, 'Delete')?.click();
      await Promise.resolve();
    });
    expect(recentRemove).toHaveBeenCalledWith(recent.id);
  });

  it('reports when secure recent-signature storage is unavailable', async () => {
    recentList.mockResolvedValue({ available: false, signatures: [] });
    act(() => root.render(createElement(SignatureMenu, { onUseSignature: vi.fn() })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    await act(async () => { await Promise.resolve(); });
    expect(document.body.textContent).toContain('Recent signatures need secure system storage.');
  });

  it('reports a recent-signature removal failure', async () => {
    const recent = recentSignature('a');
    recentList.mockResolvedValue({ available: true, signatures: [recent] });
    recentRemove.mockRejectedValue(new Error('delete failed'));
    act(() => root.render(createElement(SignatureMenu, { onUseSignature: vi.fn() })));
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    await act(async () => { await Promise.resolve(); });

    await act(async () => {
      document.querySelector<HTMLButtonElement>('[aria-label="Remove recent drawn signature 1"]')?.click();
      await Promise.resolve();
    });
    await act(async () => {
      findButton(document.body, 'Delete')?.click();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(document.body.textContent).toContain('Recent signatures could not be changed.');
    expect(document.querySelectorAll('[aria-label^="Use recent drawn signature"]')).toHaveLength(1);
  });

  it('clears typed signature data when the popover closes', () => {
    act(() => root.render(createElement(SignatureMenu, { onUseSignature: vi.fn() })));
    const trigger = host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]');
    act(() => trigger?.click());
    const typeTab = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-slot="toggle-group-item"]'))
      .find((button) => button.textContent?.includes('Type'));
    act(() => typeTab?.click());
    const input = document.querySelector<HTMLInputElement>('#signature-name');
    act(() => {
      setNativeInputValue(input!, 'Private Name');
      input?.dispatchEvent(new Event('input', { bubbles: true }));
    });

    act(() => trigger?.click());
    act(() => trigger?.click());
    expect(document.querySelector<HTMLInputElement>('#signature-name')?.value).toBe('');
  });

  it('closes when a tool shortcut is activated', () => {
    act(() => root.render(createElement(SignatureMenu, { onUseSignature: vi.fn() })));
    const trigger = host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]');
    act(() => trigger?.click());
    expect(document.querySelector('[data-testid="signature-popover"][data-open]')).toBeTruthy();

    act(() => dismissToolShortcutPopup(
      document.querySelector<HTMLButtonElement>('[data-testid="signature-popover"] button'),
    ));

    expect(document.querySelector('[data-testid="signature-popover"][data-open]')).toBeNull();
    expect(trigger?.getAttribute('aria-pressed')).toBe('false');
  });

  it('closes and clears signature data when the active document changes', () => {
    const onUseSignature = vi.fn();
    act(() => root.render(createElement(SignatureMenu, { contextId: 'document-a', onUseSignature })));
    const trigger = host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]');
    act(() => trigger?.click());
    const typeTab = Array.from(document.querySelectorAll<HTMLButtonElement>('[data-slot="toggle-group-item"]'))
      .find((button) => button.textContent?.includes('Type'));
    act(() => typeTab?.click());
    const input = document.querySelector<HTMLInputElement>('#signature-name');
    act(() => {
      setNativeInputValue(input!, 'Document A');
      input?.dispatchEvent(new Event('input', { bubbles: true }));
    });

    act(() => root.render(createElement(SignatureMenu, { contextId: 'document-b', onUseSignature })));
    expect(document.querySelector('[data-testid="signature-popover"][data-open]')).toBeNull();
    act(() => host.querySelector<HTMLButtonElement>('[data-testid="tool-signature"]')?.click());
    expect(document.querySelector<HTMLInputElement>('#signature-name')?.value).toBe('');
    expect(onUseSignature).not.toHaveBeenCalled();
  });
});

function activePanel(): HTMLElement | null {
  return document.querySelector<HTMLElement>('[data-testid="signature-mode-panel"]');
}

function findButton(container: HTMLElement | null, label: string): HTMLButtonElement | undefined {
  return Array.from(container?.querySelectorAll<HTMLButtonElement>('button') ?? [])
    .find((button) => button.textContent?.includes(label));
}

function findButtonByLabel(container: HTMLElement | null, label: string): HTMLButtonElement | undefined {
  return Array.from(container?.querySelectorAll<HTMLButtonElement>('button') ?? [])
    .find((button) => button.getAttribute('aria-label') === label);
}

function setNativeInputValue(input: HTMLInputElement, value: string): void {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
  setter?.call(input, value);
}

function recentSignature(idCharacter: string, lastUsedAt = 100) {
  return {
    id: idCharacter.repeat(43),
    lastUsedAt,
    asset: {
      dataUrl: 'data:image/png;base64,iVBORw0KGgo=',
      mimeType: 'image/png' as const,
      width: 640,
      height: 240,
      source: 'drawn' as const,
    },
  };
}
