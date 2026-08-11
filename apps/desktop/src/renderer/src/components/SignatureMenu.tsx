import { useEffect, useRef, useState, type ChangeEvent } from 'react';
import SignaturePad from 'signature_pad';
import '@fontsource/allura/400.css';
import {
  createSignatureAppearanceAsset,
  processSignaturePixels,
  type SignatureAppearanceAsset,
} from '@butter-paper/core';
import {
  CameraIcon,
  EraserIcon,
  ImageIcon,
  PlusIcon,
  SignatureIcon,
  SmartphoneIcon,
  Trash2Icon,
  XIcon,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import {
  Item,
  ItemActions,
  ItemGroup,
} from '@/components/ui/item';
import {
  Popover,
  PopoverContent,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from '@/components/ui/popover';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import { Toggle } from '@/components/ui/toggle';
import { Spinner } from '@/components/ui/spinner';
import { Separator } from '@/components/ui/separator';
import { ScrollArea } from '@/components/ui/scroll-area';
import { cn } from '@/lib/utils';
import { ConfirmationPopover } from './ConfirmationPopover';
import { RAIL_BUTTON_SIZE } from './shellSpacing';
import type { PhoneSignatureSession, RecentSignature } from '../../../shared/protocol';

const DRAWING_WIDTH = 640;
const DRAWING_HEIGHT = 240;
const MAX_IMAGE_BYTES = 10 * 1024 * 1024;
const MAX_IMAGE_DIMENSION = 4096;
const MAX_IMAGE_PIXELS = 16 * 1024 * 1024;
const MAX_IMAGE_ASPECT_RATIO = 25;
const MAX_PROCESSED_IMAGE_DIMENSION = 2048;
const SIGNATURE_INK = '#111827';

type SignatureMode = 'draw' | 'type' | 'image';
type DrawingDevice = 'computer' | 'phone';
type ImageSource = 'file' | 'webcam' | 'phone';
type RecentStorageIssue = 'unavailable' | 'load' | 'save' | 'delete';

interface SignatureMenuProps {
  disabled?: boolean;
  contextId?: string | null;
  onUseSignature: (asset: SignatureAppearanceAsset) => void;
}

export function SignatureMenu({ disabled = false, contextId = null, onUseSignature }: SignatureMenuProps) {
  const [open, setOpen] = useState(false);
  const [mode, setMode] = useState<SignatureMode>('draw');
  const [drawingDevice, setDrawingDevice] = useState<DrawingDevice>('computer');
  const [imageSource, setImageSource] = useState<ImageSource | null>(null);
  const [typedName, setTypedName] = useState('');
  const [typedFontReady, setTypedFontReady] = useState(() => !document.fonts);
  const [typedFontFallback, setTypedFontFallback] = useState(false);
  const [drawn, setDrawn] = useState(false);
  const [drawingCanvas, setDrawingCanvas] = useState<HTMLCanvasElement | null>(null);
  const [imageAsset, setImageAsset] = useState<SignatureAppearanceAsset | null>(null);
  const [imageError, setImageError] = useState<string | null>(null);
  const [cameraStarting, setCameraStarting] = useState(false);
  const [cameraReady, setCameraReady] = useState(false);
  const [cameraProcessedReady, setCameraProcessedReady] = useState(false);
  const [cameraStream, setCameraStream] = useState<MediaStream | null>(null);
  const [cameraVideo, setCameraVideo] = useState<HTMLVideoElement | null>(null);
  const [phoneStarting, setPhoneStarting] = useState(false);
  const [phoneSession, setPhoneSession] = useState<PhoneSignatureSession | null>(null);
  const [recentSignatures, setRecentSignatures] = useState<RecentSignature[]>([]);
  const [recentStorageIssue, setRecentStorageIssue] = useState<RecentStorageIssue | null>(null);
  const [addingNew, setAddingNew] = useState(false);
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);
  const padRef = useRef<SignaturePad | null>(null);
  const imageInputRef = useRef<HTMLInputElement | null>(null);
  const imageRequestRef = useRef(0);
  const cameraRequestRef = useRef(0);
  const cameraStreamRef = useRef<MediaStream | null>(null);
  const cameraProcessedCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const phoneRequestRef = useRef(0);
  const phoneSessionRef = useRef<string | null>(null);
  const recentRequestRef = useRef(0);
  const recentOperationRef = useRef<Promise<void>>(Promise.resolve());
  const recentUseButtonRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const drawModeButtonRef = useRef<HTMLButtonElement | null>(null);
  const pendingRecentFocusRef = useRef<number | 'mode' | null>(null);

  useEffect(() => {
    if (disabled) {
      resetSensitiveState();
      setOpen(false);
    }
  }, [disabled]);

  useEffect(() => {
    resetSensitiveState();
    setOpen(false);
  }, [contextId]);

  useEffect(() => {
    const requestId = ++recentRequestRef.current;
    if (!open) return;
    void window.butterPaper.signatureRecent.list().then((snapshot) => {
      if (requestId !== recentRequestRef.current) return;
      setRecentSignatures(snapshot.signatures);
      setRecentStorageIssue((current) => (
        snapshot.available ? (current === 'save' ? current : null) : 'unavailable'
      ));
    }).catch(() => {
      if (requestId === recentRequestRef.current) {
        setRecentSignatures([]);
        setRecentStorageIssue('load');
      }
    });
  }, [open]);

  useEffect(() => {
    const pendingFocus = pendingRecentFocusRef.current;
    if (pendingFocus == null) return;
    pendingRecentFocusRef.current = null;
    if (pendingFocus === 'mode') drawModeButtonRef.current?.focus();
    else recentUseButtonRefs.current[pendingFocus]?.focus();
  }, [recentSignatures]);

  useEffect(() => {
    if (!open || !document.fonts) return;
    let cancelled = false;
    setTypedFontReady(false);
    setTypedFontFallback(false);
    void document.fonts.load('96px Allura', typedName.trim() || 'Signature').then(() => {
      if (!cancelled) {
        setTypedFontReady(true);
        setTypedFontFallback(false);
      }
    }).catch(() => {
      if (!cancelled) {
        setTypedFontReady(true);
        setTypedFontFallback(true);
      }
    });
    return () => { cancelled = true; };
  }, [open, typedName]);

  useEffect(() => {
    if (!open || mode !== 'draw' || !drawingCanvas) return;

    const canvas = drawingCanvas;
    let canvasSize = prepareDrawingCanvas(canvas);
    const pad = new SignaturePad(canvas, {
      minWidth: 1.5,
      maxWidth: 4,
      penColor: SIGNATURE_INK,
      throttle: 8,
    });
    const handleEndStroke = () => {
      setImageAsset(null);
      setDrawn(!pad.isEmpty());
    };
    pad.addEventListener('endStroke', handleEndStroke);
    padRef.current = pad;
    setDrawn(false);
    const resizeObserver = new ResizeObserver(() => {
      const nextSize = readDrawingCanvasSize(canvas);
      if (nextSize.backingWidth === canvas.width && nextSize.backingHeight === canvas.height) return;
      const pointGroups = pad.toData();
      const widthScale = nextSize.logicalWidth / canvasSize.logicalWidth;
      const heightScale = nextSize.logicalHeight / canvasSize.logicalHeight;
      canvasSize = prepareDrawingCanvas(canvas, nextSize);
      if (pointGroups.length === 0) {
        pad.clear();
      } else {
        pad.fromData(pointGroups.map((group) => ({
          ...group,
          points: group.points.map((point) => ({
            ...point,
            x: point.x * widthScale,
            y: point.y * heightScale,
          })),
        })));
      }
      setDrawn(!pad.isEmpty());
    });
    resizeObserver.observe(canvas);

    return () => {
      resizeObserver.disconnect();
      pad.removeEventListener('endStroke', handleEndStroke);
      pad.off();
      if (padRef.current === pad) padRef.current = null;
    };
  }, [drawingCanvas, mode, open]);

  useEffect(() => {
    if (!cameraVideo) return;
    cameraVideo.srcObject = cameraStream;
    return () => {
      if (cameraVideo.srcObject === cameraStream) cameraVideo.srcObject = null;
    };
  }, [cameraStream, cameraVideo]);

  useEffect(() => () => {
    cameraRequestRef.current += 1;
    stopMediaStream(cameraStreamRef.current);
    cameraStreamRef.current = null;
    phoneRequestRef.current += 1;
    const sessionId = phoneSessionRef.current;
    phoneSessionRef.current = null;
    if (sessionId) void window.butterPaper.signaturePhone.stop(sessionId).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!phoneSession) return;
    let cancelled = false;
    let pollTimer: ReturnType<typeof setTimeout> | null = null;
    const requestId = phoneRequestRef.current;

    const poll = async () => {
      try {
        const result = await window.butterPaper.signaturePhone.poll(phoneSession.id);
        if (cancelled || requestId !== phoneRequestRef.current) return;
        if (result.status === 'waiting') {
          pollTimer = setTimeout(() => { void poll(); }, 2_000);
          return;
        }
        phoneSessionRef.current = null;
        setPhoneSession(null);
        if (result.status === 'expired') {
          setImageError('The phone link expired. Start a new transfer to try again.');
          return;
        }
        const imageRequestId = ++imageRequestRef.current;
        await importImageDataUrl(
          result.image.dataUrl,
          result.image.mimeType,
          imageRequestId,
          result.image.mode === 'draw' ? 'drawn' : 'image',
        );
      } catch (error) {
        if (!cancelled && requestId === phoneRequestRef.current) {
          void window.butterPaper.signaturePhone.stop(phoneSession.id).catch(() => undefined);
          phoneSessionRef.current = null;
          setPhoneSession(null);
          setImageError(error instanceof Error ? error.message : 'Unable to receive the phone signature.');
        }
      }
    };

    pollTimer = setTimeout(() => { void poll(); }, 250);
    return () => {
      cancelled = true;
      if (pollTimer) clearTimeout(pollTimer);
    };
  }, [phoneSession]);

  function useAsset(asset: SignatureAppearanceAsset): void {
    onUseSignature(asset);
    void queueRecentOperation(() => window.butterPaper.signatureRecent.remember(asset)).then((snapshot) => {
      setRecentSignatures(snapshot.signatures);
      setRecentStorageIssue(snapshot.available ? null : 'unavailable');
    }).catch(() => {
      setRecentStorageIssue('save');
    });
    resetSensitiveState();
    setOpen(false);
  }

  async function removeRecentSignature(id: string, index: number): Promise<void> {
    try {
      const snapshot = await queueRecentOperation(() => window.butterPaper.signatureRecent.remove(id));
      pendingRecentFocusRef.current = snapshot.signatures.length > 0
        ? Math.min(index, snapshot.signatures.length - 1)
        : 'mode';
      setRecentSignatures(snapshot.signatures);
      setRecentStorageIssue(snapshot.available ? null : 'unavailable');
    } catch {
      const snapshot = await queueRecentOperation(() => window.butterPaper.signatureRecent.list()).catch(() => null);
      if (snapshot) setRecentSignatures(snapshot.signatures);
      setRecentStorageIssue('delete');
    }
  }

  function queueRecentOperation<T>(operation: () => Promise<T>): Promise<T> {
    const result = recentOperationRef.current.then(operation);
    recentOperationRef.current = result.then(() => undefined, () => undefined);
    return result;
  }

  function resetSensitiveState(): void {
    imageRequestRef.current += 1;
    stopCamera();
    stopPhoneTransfer();
    padRef.current?.clear();
    setDrawn(false);
    setDrawingDevice('computer');
    setImageSource(null);
    setAddingNew(false);
    setPendingDeleteId(null);
    setTypedName('');
    setImageAsset(null);
    setImageError(null);
  }

  function handleOpenChange(nextOpen: boolean): void {
    if (!nextOpen) resetSensitiveState();
    setOpen(nextOpen);
  }

  function useDrawing(): void {
    const pad = padRef.current;
    const canvas = drawingCanvas;
    if (!pad || !canvas || pad.isEmpty()) return;
    useAsset(createSignatureAppearanceAsset({
      dataUrl: pad.toDataURL('image/png'),
      mimeType: 'image/png',
      width: canvas.width,
      height: canvas.height,
      source: 'drawn',
    }));
  }

  function useTypedName(): void {
    const value = typedName.trim();
    if (!value || !typedFontReady) return;
    useAsset(renderTypedSignature(value));
  }

  async function handleImageChange(event: ChangeEvent<HTMLInputElement>): Promise<void> {
    stopCamera();
    stopPhoneTransfer();
    const file = event.currentTarget.files?.[0] ?? null;
    const requestId = ++imageRequestRef.current;
    event.currentTarget.value = '';
    setImageAsset(null);
    setImageError(null);
    if (!file) return;
    setImageSource('file');
    if (file.type !== 'image/png' && file.type !== 'image/jpeg') {
      setImageError('Choose a PNG or JPEG image.');
      return;
    }
    if (file.size > MAX_IMAGE_BYTES) {
      setImageError('Choose an image smaller than 10 MB.');
      return;
    }

    const dataUrl = await readFileAsDataUrl(file).catch((error) => {
      if (requestId === imageRequestRef.current) {
        setImageError(error instanceof Error ? error.message : 'Unable to read this image.');
      }
      return null;
    });
    if (dataUrl) await importImageDataUrl(dataUrl, file.type, requestId);
  }

  async function importImageDataUrl(
    dataUrl: string,
    mimeType: 'image/png' | 'image/jpeg',
    requestId: number,
    source: 'drawn' | 'image' = 'image',
  ): Promise<void> {
    try {
      validateImageDimensions(readImageHeaderDimensions(dataUrl, mimeType));
      const image = await decodeImage(dataUrl);
      if (requestId !== imageRequestRef.current) return;
      const dimensions = validateImageDimensions(image);
      const processed = renderImportedSignature(image, dimensions);
      setImageAsset(createSignatureAppearanceAsset({
        dataUrl: processed.dataUrl,
        mimeType: 'image/png',
        width: processed.width,
        height: processed.height,
        source,
      }));
      setImageError(null);
    } catch (error) {
      if (requestId === imageRequestRef.current) {
        setImageError(error instanceof Error ? error.message : 'Unable to read this image.');
      }
    }
  }

  function stopCamera(): void {
    cameraRequestRef.current += 1;
    stopMediaStream(cameraStreamRef.current);
    cameraStreamRef.current = null;
    setCameraStream(null);
    setCameraStarting(false);
    setCameraReady(false);
    setCameraProcessedReady(false);
  }

  async function startCamera(): Promise<void> {
    setImageSource('webcam');
    if (!navigator.mediaDevices?.getUserMedia) {
      setImageError('A camera is not available in this app or on this device.');
      return;
    }
    imageRequestRef.current += 1;
    stopPhoneTransfer();
    const requestId = ++cameraRequestRef.current;
    setImageAsset(null);
    setImageError(null);
    setCameraStarting(true);
    setCameraReady(false);
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
      if (requestId !== cameraRequestRef.current || !open || mode !== 'image') {
        stopMediaStream(stream);
        return;
      }
      stopMediaStream(cameraStreamRef.current);
      cameraStreamRef.current = stream;
      setCameraStream(stream);
    } catch {
      if (requestId === cameraRequestRef.current) {
        setImageError('Camera access was denied or the camera is unavailable.');
      }
    } finally {
      if (requestId === cameraRequestRef.current) setCameraStarting(false);
    }
  }

  function stopPhoneTransfer(): void {
    phoneRequestRef.current += 1;
    const sessionId = phoneSessionRef.current;
    phoneSessionRef.current = null;
    setPhoneSession(null);
    setPhoneStarting(false);
    if (sessionId) void window.butterPaper.signaturePhone.stop(sessionId).catch(() => undefined);
  }

  async function startPhoneTransfer(phoneMode: 'draw' | 'image'): Promise<void> {
    if (phoneMode === 'image') setImageSource('phone');
    stopCamera();
    stopPhoneTransfer();
    const requestId = ++phoneRequestRef.current;
    imageRequestRef.current += 1;
    setImageAsset(null);
    setImageError(null);
    setPhoneStarting(true);
    try {
      const session = await window.butterPaper.signaturePhone.start(phoneMode);
      if (requestId !== phoneRequestRef.current || !open || mode !== phoneMode) {
        await window.butterPaper.signaturePhone.stop(session.id).catch(() => undefined);
        return;
      }
      phoneSessionRef.current = session.id;
      setPhoneSession(session);
    } catch (error) {
      if (requestId === phoneRequestRef.current) {
        setImageError(error instanceof Error ? error.message : 'Unable to start the phone signature transfer.');
      }
    } finally {
      if (requestId === phoneRequestRef.current) setPhoneStarting(false);
    }
  }

  function captureCameraImage(): void {
    if (!cameraVideo || !cameraReady) return;
    try {
      const dimensions = validateImageDimensions({
        width: cameraVideo.videoWidth,
        height: cameraVideo.videoHeight,
      });
      const processed = renderImportedSignature(cameraVideo, dimensions, { focusBand: true });
      setImageAsset(createSignatureAppearanceAsset({
        dataUrl: processed.dataUrl,
        mimeType: 'image/png',
        width: processed.width,
        height: processed.height,
        source: 'image',
      }));
      setImageError(null);
      stopCamera();
    } catch (error) {
      setImageError(error instanceof Error ? error.message : 'Unable to capture this camera image.');
    }
  }

  function updateCameraProcessedPreview(video: HTMLVideoElement): void {
    const preview = cameraProcessedCanvasRef.current;
    if (!preview || video.videoWidth <= 0 || video.videoHeight <= 0) return;
    try {
      const processed = renderImportedSignature(video, {
        width: video.videoWidth,
        height: video.videoHeight,
      }, { focusBand: true });
      const context = preview.getContext('2d');
      if (!context) return;
      context.clearRect(0, 0, preview.width, preview.height);
      const scale = Math.min(preview.width / processed.width, preview.height / processed.height);
      const width = processed.width * scale;
      const height = processed.height * scale;
      context.drawImage(
        processed.canvas,
        (preview.width - width) / 2,
        (preview.height - height) / 2,
        width,
        height,
      );
      setCameraProcessedReady(true);
    } catch {
      preview.getContext('2d')?.clearRect(0, 0, preview.width, preview.height);
      setCameraProcessedReady(false);
    }
  }

  const sortedRecentSignatures = [...recentSignatures].sort((first, second) => (
    second.lastUsedAt - first.lastUsedAt || first.id.localeCompare(second.id)
  ));
  const hasRecentSignatures = sortedRecentSignatures.length > 0;
  const showNewSignatureForm = !hasRecentSignatures || addingNew;
  const recentSignatureList = (
    <ItemGroup
      className={cn('grid grid-cols-2 gap-2', sortedRecentSignatures.length > 4 && 'pr-3')}
      data-testid="recent-signatures"
    >
      {sortedRecentSignatures.map((recent, index) => (
        <div
          key={recent.id}
          role="listitem"
          className="group/signature relative min-w-0"
          data-recent-signature-id={recent.id}
        >
          <Item
            render={(
              <Button
                type="button"
                variant="outline"
                ref={(element) => { recentUseButtonRefs.current[index] = element; }}
              />
            )}
            variant="outline"
            size="sm"
            className="h-24 min-w-0 cursor-pointer justify-center p-2 hover:bg-muted"
            aria-label={recentSignatureActionLabel('Use', recent, index)}
            onClick={() => useAsset(recent.asset)}
          >
            <span className="flex size-full items-center justify-center rounded-md bg-white p-2">
              <img src={recent.asset.dataUrl} alt="" className="max-h-full max-w-full object-contain" />
            </span>
          </Item>
          <ItemActions className="absolute right-1.5 top-1.5 opacity-0 transition-opacity group-hover/signature:opacity-100 group-focus-within/signature:opacity-100">
            <ConfirmationPopover
              open={pendingDeleteId === recent.id}
              onOpenChange={(nextOpen) => setPendingDeleteId(nextOpen ? recent.id : null)}
              trigger={(
                <Button
                  type="button"
                  variant="outline"
                  size="icon-xs"
                  aria-label={recentSignatureActionLabel('Remove', recent, index)}
                >
                  <Trash2Icon aria-hidden="true" />
                </Button>
              )}
              side="left"
              align="start"
              title="Delete this signature?"
              description="This removes it from Recent. This action cannot be undone."
              actionLabel="Delete"
              actionVariant="destructive"
              onAction={() => {
                setPendingDeleteId(null);
                void removeRecentSignature(recent.id, index);
              }}
            />
          </ItemActions>
        </div>
      ))}
    </ItemGroup>
  );

  return (
    <div className="flex" data-testid="signature-controls">
      <Popover open={open} onOpenChange={handleOpenChange}>
        <PopoverTrigger render={(
          <Toggle
            type="button"
            pressed={open}
            className={cn('relative shrink-0 p-0', RAIL_BUTTON_SIZE)}
            disabled={disabled}
            data-testid="tool-signature"
            data-rail-tooltip="Signature"
            aria-label="Signature"
          >
            <SignatureIcon aria-hidden="true" />
          </Toggle>
        )} />
        <PopoverContent
          side="left"
          align="start"
          sideOffset={8}
          className="max-h-[calc(100vh-1rem)] w-96 max-w-[calc(100vw-1rem)] overflow-y-auto"
          data-testid="signature-popover"
        >
          <PopoverHeader>
            <PopoverTitle>Signature</PopoverTitle>
          </PopoverHeader>
          <div className="flex flex-col gap-3">
            {hasRecentSignatures ? (
              <>
                {sortedRecentSignatures.length > 4 ? (
                  <ScrollArea className="h-52 w-full" data-testid="recent-signatures-scroll-area">
                    {recentSignatureList}
                  </ScrollArea>
                ) : recentSignatureList}
                {!addingNew ? (
                  <Button type="button" variant="outline" className="w-full" onClick={() => setAddingNew(true)}>
                    <PlusIcon data-icon="inline-start" />
                    Add new signature
                  </Button>
                ) : null}
              </>
            ) : null}
            {recentStorageIssue ? (
              <FieldError role="status">
                {recentStorageIssue === 'unavailable'
                  ? 'Recent signatures need secure system storage.'
                  : recentStorageIssue === 'load'
                    ? 'Recent signatures could not be loaded.'
                    : recentStorageIssue === 'save'
                      ? 'This signature could not be saved to Recent.'
                      : 'Recent signatures could not be changed.'}
              </FieldError>
            ) : null}
            {showNewSignatureForm ? (
            <>
            {hasRecentSignatures ? <Separator /> : null}
            <div className="flex items-center justify-between gap-2">
              <span className="text-sm font-medium">Add new signature</span>
              {hasRecentSignatures ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  aria-label="Close new signature editor"
                  onClick={resetSensitiveState}
                >
                  <XIcon aria-hidden="true" />
                </Button>
              ) : null}
            </div>
            <ToggleGroup
              aria-label="Signature input method"
              className="grid w-full grid-cols-3"
              spacing={0}
              variant="outline"
              value={[mode]}
              onValueChange={(values) => {
                const nextMode = values.at(-1) as SignatureMode | undefined;
                if (!nextMode) return;
                imageRequestRef.current += 1;
                stopPhoneTransfer();
                if (nextMode !== 'image') stopCamera();
                setImageSource(null);
                setDrawingDevice('computer');
                setMode(nextMode);
              }}
            >
              <ToggleGroupItem ref={drawModeButtonRef} className="w-full" value="draw" aria-controls="signature-draw-panel">
                Draw
              </ToggleGroupItem>
              <ToggleGroupItem className="w-full" value="type" aria-controls="signature-type-panel">
                Type
              </ToggleGroupItem>
              <ToggleGroupItem className="w-full" value="image" aria-controls="signature-image-panel">
                Image
              </ToggleGroupItem>
            </ToggleGroup>
            {mode === 'draw' ? (
            <div id="signature-draw-panel" role="region" aria-label="Draw signature" className="flex flex-col gap-2" data-testid="signature-mode-panel">
              <ToggleGroup
                aria-label="Drawing device"
                className="grid w-full grid-cols-2"
                spacing={0}
                variant="outline"
                value={[drawingDevice]}
                onValueChange={(values) => {
                  const nextDevice = values.at(-1) as DrawingDevice | undefined;
                  if (!nextDevice || nextDevice === drawingDevice) return;
                  setImageError(null);
                  setDrawingDevice(nextDevice);
                  if (nextDevice === 'phone') {
                    padRef.current?.clear();
                    setDrawn(false);
                    void startPhoneTransfer('draw');
                  } else {
                    stopPhoneTransfer();
                    setImageAsset(null);
                  }
                }}
              >
                <ToggleGroupItem className="w-full" value="computer">Computer</ToggleGroupItem>
                <ToggleGroupItem className="w-full" value="phone">Phone</ToggleGroupItem>
              </ToggleGroup>
              {drawingDevice === 'computer' ? (
                <>
                  <div className="overflow-hidden rounded-md border border-input bg-white">
                    <canvas
                      ref={setDrawingCanvas}
                      className="block aspect-[8/3] h-auto w-full touch-none"
                      aria-label="Draw your signature"
                      data-testid="signature-draw-canvas"
                    />
                  </div>
                  <div className="grid grid-cols-2 gap-2">
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => {
                        padRef.current?.clear();
                        setDrawn(false);
                      }}
                    >
                      <EraserIcon data-icon="inline-start" />
                      Clear
                    </Button>
                    <Button type="button" size="sm" disabled={!drawn} onClick={useDrawing}>
                      Add signature
                    </Button>
                  </div>
                </>
              ) : phoneSession ? (
                <PhoneTransferPanel session={phoneSession} onClose={stopPhoneTransfer} />
              ) : imageAsset?.source === 'drawn' ? (
                <>
                  <div className="flex h-28 items-center justify-center overflow-hidden rounded-md border border-input bg-white p-2">
                    <img
                      src={imageAsset.dataUrl}
                      alt="Phone drawing preview"
                      className="max-h-full max-w-full object-contain"
                      data-testid="signature-draw-phone-preview"
                    />
                  </div>
                  <Button type="button" size="sm" onClick={() => useAsset(imageAsset)}>
                    Add signature
                  </Button>
                </>
              ) : (
                <Button type="button" variant="outline" size="sm" disabled={phoneStarting} onClick={() => { void startPhoneTransfer('draw'); }}>
                  {phoneStarting ? <Spinner data-icon="inline-start" /> : <SmartphoneIcon data-icon="inline-start" />}
                  {phoneStarting ? 'Starting…' : 'Connect phone'}
                </Button>
              )}
              {imageError ? <FieldError role="alert">{imageError}</FieldError> : null}
            </div>
            ) : null}
            {mode === 'type' ? (
            <div id="signature-type-panel" role="region" aria-label="Type signature" data-testid="signature-mode-panel">
              <FieldGroup>
                <Field>
                  <FieldLabel htmlFor="signature-name">Name</FieldLabel>
                  <Input
                    id="signature-name"
                    value={typedName}
                    maxLength={80}
                    placeholder="Type your name"
                    onChange={(event) => setTypedName(event.currentTarget.value)}
                  />
                </Field>
                <div
                  className="flex h-28 items-center justify-center overflow-hidden rounded-md border border-input bg-white px-4 text-center text-5xl text-slate-900"
                  style={{ fontFamily: 'Allura, cursive' }}
                  data-testid="signature-type-preview"
                >
                  {typedName.trim() || 'Your signature'}
                </div>
                <Button type="button" size="sm" disabled={!typedName.trim() || !typedFontReady} onClick={useTypedName}>
                  Add signature
                </Button>
                {!typedFontReady ? <FieldDescription>Loading the signature font…</FieldDescription> : null}
                {typedFontFallback ? <FieldDescription>The signature font could not load. Using a system cursive font.</FieldDescription> : null}
              </FieldGroup>
            </div>
            ) : null}
            {mode === 'image' ? (
            <div id="signature-image-panel" role="region" aria-label="Image signature" data-testid="signature-mode-panel">
              <FieldGroup>
                <Field data-invalid={Boolean(imageError)}>
                  <Input
                    ref={imageInputRef}
                    id="signature-image"
                    type="file"
                    accept="image/png,image/jpeg"
                    className="sr-only"
                    aria-hidden="true"
                    tabIndex={-1}
                    onChange={(event) => { void handleImageChange(event); }}
                  />
                  <ToggleGroup
                    aria-label="Image source"
                    className="grid w-full grid-cols-3"
                    spacing={0}
                    variant="outline"
                    value={imageSource ? [imageSource] : []}
                    data-testid="signature-image-source-actions"
                  >
                    <ToggleGroupItem
                      className="w-full"
                      value="file"
                      disabled={cameraStarting || phoneStarting}
                      aria-invalid={Boolean(imageError)}
                      onClick={() => {
                        stopCamera();
                        stopPhoneTransfer();
                        imageInputRef.current?.click();
                      }}
                    >
                      <ImageIcon data-icon="inline-start" />
                      Choose file
                    </ToggleGroupItem>
                    <ToggleGroupItem className="w-full" value="webcam" disabled={cameraStarting || phoneStarting} onClick={() => { void startCamera(); }}>
                      {cameraStarting ? <Spinner data-icon="inline-start" /> : <CameraIcon data-icon="inline-start" />}
                      {cameraStarting ? 'Starting…' : 'Webcam'}
                    </ToggleGroupItem>
                    <ToggleGroupItem className="w-full" value="phone" disabled={cameraStarting || phoneStarting} onClick={() => { void startPhoneTransfer('image'); }}>
                      {phoneStarting ? <Spinner data-icon="inline-start" /> : <SmartphoneIcon data-icon="inline-start" />}
                      {phoneStarting ? 'Starting…' : 'Phone'}
                    </ToggleGroupItem>
                  </ToggleGroup>
                  {imageError ? <FieldError role="alert">{imageError}</FieldError> : null}
                </Field>
                {cameraStream ? (
                  <Field>
                    <div className="flex items-start justify-between gap-2">
                      <FieldDescription>
                        Use dark ink on white paper. Hold the signature level with the guide.
                      </FieldDescription>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon-xs"
                        aria-label="Close webcam"
                        onClick={stopCamera}
                      >
                        <XIcon aria-hidden="true" />
                      </Button>
                    </div>
                    <div className="flex justify-end">
                      <Button type="button" size="sm" disabled={!cameraReady || !cameraProcessedReady} onClick={captureCameraImage}>
                        <CameraIcon data-icon="inline-start" />
                        Take photo
                      </Button>
                    </div>
                    <div className="relative overflow-hidden rounded-md border border-input bg-black">
                      <video
                        ref={setCameraVideo}
                        autoPlay
                        muted
                        playsInline
                        aria-label="Camera preview"
                        className="aspect-video w-full object-contain"
                        data-testid="signature-camera-preview"
                        onCanPlay={(event) => {
                          setCameraReady(true);
                          updateCameraProcessedPreview(event.currentTarget);
                        }}
                        onTimeUpdate={(event) => updateCameraProcessedPreview(event.currentTarget)}
                      />
                      <span className="pointer-events-none absolute inset-x-4 top-1/2 h-px bg-primary" aria-hidden="true" />
                    </div>
                    <div className="flex h-28 items-center justify-center overflow-hidden rounded-md border border-input bg-white p-2">
                      <canvas
                        ref={cameraProcessedCanvasRef}
                        width={640}
                        height={240}
                        className="max-h-full max-w-full object-contain"
                        aria-label="Processed signature preview"
                        data-testid="signature-camera-processed-preview"
                      />
                    </div>
                    <FieldDescription aria-live="polite">
                      {cameraProcessedReady ? 'Processed preview' : 'Waiting for a signature on white paper…'}
                    </FieldDescription>
                  </Field>
                ) : phoneSession ? <PhoneTransferPanel session={phoneSession} onClose={stopPhoneTransfer} /> : null}
                {imageAsset?.source === 'image' ? (
                  <div className="flex h-28 items-center justify-center overflow-hidden rounded-md border border-input bg-white p-2">
                    <img
                      src={imageAsset.dataUrl}
                      alt="Signature preview"
                      className="max-h-full max-w-full object-contain"
                      data-testid="signature-image-preview"
                    />
                  </div>
                ) : null}
                {imageAsset?.source === 'image' ? (
                  <Button type="button" size="sm" onClick={() => useAsset(imageAsset)}>
                    Add signature
                  </Button>
                ) : null}
              </FieldGroup>
            </div>
            ) : null}
            </>
            ) : null}
          </div>
        </PopoverContent>
      </Popover>
    </div>
  );
}

function PhoneTransferPanel({
  session,
  onClose,
}: {
  session: PhoneSignatureSession;
  onClose: () => void;
}) {
  return (
    <div className="flex flex-col items-center gap-2 py-1">
      <div className="flex w-full justify-end">
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          aria-label="Close phone transfer"
          onClick={onClose}
        >
          <XIcon aria-hidden="true" />
        </Button>
      </div>
      <img
        src={session.qrDataUrl}
        alt={`QR code to ${session.mode === 'draw' ? 'draw' : 'capture'} a signature on a phone`}
        className="size-48 rounded-sm bg-white"
        data-testid="signature-phone-qr"
      />
      <FieldDescription className="text-center">
        Scan with your phone. Expires in 5 minutes.
      </FieldDescription>
    </div>
  );
}

function renderTypedSignature(value: string): SignatureAppearanceAsset {
  const canvas = document.createElement('canvas');
  const measurementContext = canvas.getContext('2d');
  if (!measurementContext) throw new Error('Unable to render the typed signature.');
  measurementContext.font = '96px Allura, cursive';
  const measuredWidth = measurementContext.measureText(value).width;
  canvas.width = Math.max(240, Math.min(1200, Math.ceil(measuredWidth + 64)));
  canvas.height = 200;
  const context = canvas.getContext('2d');
  if (!context) throw new Error('Unable to render the typed signature.');
  context.clearRect(0, 0, canvas.width, canvas.height);
  context.fillStyle = SIGNATURE_INK;
  context.font = '96px Allura, cursive';
  context.textAlign = 'center';
  context.textBaseline = 'middle';
  context.fillText(value, canvas.width / 2, canvas.height / 2, canvas.width - 32);
  return createSignatureAppearanceAsset({
    dataUrl: canvas.toDataURL('image/png'),
    mimeType: 'image/png',
    width: canvas.width,
    height: canvas.height,
    source: 'typed',
  });
}

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener('load', () => {
      if (typeof reader.result === 'string') resolve(reader.result);
      else reject(new Error('Unable to read this image.'));
    });
    reader.addEventListener('error', () => reject(new Error('Unable to read this image.')));
    reader.readAsDataURL(file);
  });
}

function decodeImage(dataUrl: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.addEventListener('load', () => resolve(image));
    image.addEventListener('error', () => reject(new Error('Unable to decode this image.')));
    image.src = dataUrl;
  });
}

function readImageHeaderDimensions(
  dataUrl: string,
  mimeType: 'image/png' | 'image/jpeg',
): { width: number; height: number } {
  const payload = dataUrl.slice(dataUrl.indexOf(',') + 1);
  const binary = atob(payload);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  if (mimeType === 'image/png') {
    if (bytes.length < 24
      || bytes[0] !== 0x89 || bytes[1] !== 0x50 || bytes[2] !== 0x4e || bytes[3] !== 0x47) {
      throw new Error('Unable to read this PNG image.');
    }
    return {
      width: readUint32(bytes, 16),
      height: readUint32(bytes, 20),
    };
  }

  if (bytes.length < 4 || bytes[0] !== 0xff || bytes[1] !== 0xd8) {
    throw new Error('Unable to read this JPEG image.');
  }
  let offset = 2;
  while (offset + 8 < bytes.length) {
    while (bytes[offset] === 0xff) offset += 1;
    const marker = bytes[offset++];
    if (marker === undefined || marker === 0xd9 || marker === 0xda) break;
    if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd8)) continue;
    if (offset + 1 >= bytes.length) break;
    const segmentLength = (bytes[offset] << 8) | bytes[offset + 1];
    if (segmentLength < 2 || offset + segmentLength > bytes.length) break;
    if ([0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf].includes(marker)) {
      if (segmentLength < 7) break;
      return {
        height: (bytes[offset + 3] << 8) | bytes[offset + 4],
        width: (bytes[offset + 5] << 8) | bytes[offset + 6],
      };
    }
    offset += segmentLength;
  }
  throw new Error('Unable to read this JPEG image\'s dimensions.');
}

function readUint32(bytes: Uint8Array, offset: number): number {
  return ((bytes[offset] * 0x1000000)
    + (bytes[offset + 1] << 16)
    + (bytes[offset + 2] << 8)
    + bytes[offset + 3]) >>> 0;
}

function validateImageDimensions(
  image: { width: number; height: number; naturalWidth?: number; naturalHeight?: number },
): { width: number; height: number } {
  const width = image.naturalWidth || image.width;
  const height = image.naturalHeight || image.height;
  const aspectRatio = Math.max(width / height, height / width);
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    throw new Error('Unable to read this image\'s dimensions.');
  }
  if (width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION || width * height > MAX_IMAGE_PIXELS) {
    throw new Error('Choose an image no larger than 4096 × 4096 pixels.');
  }
  if (aspectRatio > MAX_IMAGE_ASPECT_RATIO) {
    throw new Error('Choose an image with a less extreme aspect ratio.');
  }
  return { width, height };
}

function renderImportedSignature(
  image: CanvasImageSource,
  dimensions: { width: number; height: number },
  options: { focusBand?: boolean } = {},
): { canvas: HTMLCanvasElement; dataUrl: string; width: number; height: number } {
  const sourceX = options.focusBand ? dimensions.width * 0.05 : 0;
  const sourceY = options.focusBand ? dimensions.height * 0.2 : 0;
  const sourceWidth = options.focusBand ? dimensions.width * 0.9 : dimensions.width;
  const sourceHeight = options.focusBand ? dimensions.height * 0.6 : dimensions.height;
  const scale = Math.min(1, MAX_PROCESSED_IMAGE_DIMENSION / Math.max(sourceWidth, sourceHeight));
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, Math.round(sourceWidth * scale));
  canvas.height = Math.max(1, Math.round(sourceHeight * scale));
  const context = canvas.getContext('2d');
  if (!context) throw new Error('Unable to process this image.');
  context.clearRect(0, 0, canvas.width, canvas.height);
  context.imageSmoothingEnabled = true;
  context.imageSmoothingQuality = 'high';
  context.drawImage(
    image,
    sourceX,
    sourceY,
    sourceWidth,
    sourceHeight,
    0,
    0,
    canvas.width,
    canvas.height,
  );
  const pixels = context.getImageData(0, 0, canvas.width, canvas.height);
  const processed = processSignaturePixels({ data: pixels.data, width: canvas.width, height: canvas.height });
  const output = document.createElement('canvas');
  output.width = processed.width;
  output.height = processed.height;
  const outputContext = output.getContext('2d');
  if (!outputContext) throw new Error('Unable to process this image.');
  const outputPixels = outputContext.createImageData(processed.width, processed.height);
  outputPixels.data.set(processed.data);
  outputContext.putImageData(outputPixels, 0, 0);
  return {
    canvas: output,
    dataUrl: output.toDataURL('image/png'),
    width: output.width,
    height: output.height,
  };
}

interface DrawingCanvasSize {
  readonly logicalWidth: number;
  readonly logicalHeight: number;
  readonly backingWidth: number;
  readonly backingHeight: number;
  readonly pixelRatio: number;
}

function readDrawingCanvasSize(canvas: HTMLCanvasElement): DrawingCanvasSize {
  const width = canvas.offsetWidth > 0 ? canvas.offsetWidth : DRAWING_WIDTH;
  const height = canvas.offsetHeight > 0 ? canvas.offsetHeight : DRAWING_HEIGHT;
  const pixelRatio = Math.max(window.devicePixelRatio || 1, 1);
  return {
    logicalWidth: width,
    logicalHeight: height,
    backingWidth: Math.max(1, Math.round(width * pixelRatio)),
    backingHeight: Math.max(1, Math.round(height * pixelRatio)),
    pixelRatio,
  };
}

function prepareDrawingCanvas(
  canvas: HTMLCanvasElement,
  size = readDrawingCanvasSize(canvas),
): DrawingCanvasSize {
  canvas.width = size.backingWidth;
  canvas.height = size.backingHeight;
  canvas.getContext('2d')?.scale(size.pixelRatio, size.pixelRatio);
  return size;
}

function stopMediaStream(stream: MediaStream | null): void {
  for (const track of stream?.getTracks() ?? []) track.stop();
}

function recentSignatureSource(recent: RecentSignature): 'drawn' | 'typed' | 'image' {
  return recent.asset.source === 'drawn'
    ? 'drawn'
    : recent.asset.source === 'typed'
      ? 'typed'
      : 'image';
}

function recentSignatureActionLabel(
  action: 'Use' | 'Remove',
  recent: RecentSignature,
  index: number,
): string {
  return `${action} recent ${recentSignatureSource(recent)} signature ${index + 1}`;
}
