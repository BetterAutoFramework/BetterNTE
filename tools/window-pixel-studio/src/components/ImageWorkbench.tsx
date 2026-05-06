import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Pipette,
  Square,
  Circle,
  Pentagon,
  Move,
  ZoomIn,
  Save,
  Download,
  RefreshCw,
  Settings,
  Camera,
  CircleDot,
  SquareRoundCorner,
  Copy,
  FileText,
  FolderOpen,
  X,
  Trash2,
  CheckSquare,
  SquareCheck,
  Scissors,
  Maximize,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { cn } from "@/lib/utils";
import {
  type KeyAction,
  DEFAULT_KEYBINDINGS,
  buildKeyMap,
  formatKey,
} from "@/lib/keybindings";
import { I18nProvider, useI18n, getShortcutLabelKey, type Lang } from "@/lib/i18n";

// ─── Types ────────────────────────────────────────────────────────────────────

type DrawTool = "pan" | "pick" | "rect" | "ellipse" | "roundrect" | "polygon";

interface Pt {
  x: number;
  y: number;
}

interface PickPoint {
  x: number;
  y: number;
  hex: string;
  rgba: [number, number, number, number];
}

interface WindowDto {
  hwnd: number;
  title: string;
}

interface CaptureDto {
  width: number;
  height: number;
  pngBase64: string;
}

interface SavedFrameDto {
  path: string;
  name: string;
  modifiedMs: number;
  sizeBytes: number;
}

interface SettingsDto {
  saveDir: string | null;
  cropDir: string | null;
  intervalMs: number;
  roundedRx: number;
  lastHwnd: number | null;
  language: string | null;
  keybindings: Record<string, string> | null;
  scrollDirection: string | null;
  scrollAmount: number | null;
  scrollFrames: number | null;
  scrollDelayMs: number | null;
  panoDirection: string | null;
  panoDragDistance: number | null;
  panoFrames: number | null;
  panoDelayMs: number | null;
}

interface CaptureProgress {
  current: number;
  total: number;
  phase: string;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function joinPath(dir: string, name: string) {
  const sep = dir.includes("\\") ? "\\" : "/";
  const d = dir.replace(/[/\\]+$/, "");
  return `${d}${sep}${name}`;
}

function clamp(v: number, a: number, b: number) {
  return Math.min(b, Math.max(a, v));
}

function normalizeDrag(x0: number, y0: number, x1: number, y1: number) {
  return {
    x: Math.min(x0, x1),
    y: Math.min(y0, y1),
    w: Math.abs(x1 - x0),
    h: Math.abs(y1 - y0),
  };
}

function polyBBox(pts: Pt[]) {
  const xs = pts.map((p) => p.x);
  const ys = pts.map((p) => p.y);
  return {
    x: Math.min(...xs),
    y: Math.min(...ys),
    w: Math.max(...xs) - Math.min(...xs),
    h: Math.max(...ys) - Math.min(...ys),
  };
}

function exportClipped(
  img: HTMLImageElement,
  bbox: { x: number; y: number; w: number; h: number },
  drawClip: (ctx: CanvasRenderingContext2D) => void,
): string {
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.ceil(bbox.w));
  canvas.height = Math.max(1, Math.ceil(bbox.h));
  const ctx = canvas.getContext("2d")!;
  ctx.translate(-bbox.x, -bbox.y);
  ctx.beginPath();
  drawClip(ctx);
  ctx.clip();
  ctx.drawImage(img, 0, 0);
  return canvas.toDataURL("image/png");
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatTime(ms: number): string {
  if (!ms) return "--";
  return new Date(ms).toLocaleString("zh-CN", { hour12: false });
}

/** Generate a JS code snippet from a WPS JSON sidecar file. */
function generateJsSnippet(json: Record<string, unknown>, baseName?: string): string {
  const tplName = baseName ?? "template_name";
  if (json.type === "crop" && typeof json.x === "number") {
    const { x, y, w, h } = json as { x: number; y: number; w: number; h: number };
    return [
      `// OCR region (${x}, ${y}, ${w}x${h})`,
      `const text = await ctx.ocr(${x}, ${y}, ${w+100}, ${h+100});`,
      ``,
      `// Find template in region`,
      `const match = await ctx.findTemplate("${tplName}", { roi: { x: ${x}, y: ${y}, width: ${w+100}, height: ${h+100} }, threshold: 0.95 });`,
      ``,
      `// Find template in region`,
      `const match = await ctx.waitForTemplate("${tplName}", 1000, { roi: { x: ${x}, y: ${y}, width: ${w+100}, height: ${h+100} }, threshold: 0.95 });`,
      ``,
    ].join("\n");
  }
  if (json.type === "pick" && Array.isArray(json.points)) {
    const points = json.points as { x: number; y: number; color: string }[];
    const lines: string[] = [];
    points.forEach((p, i) => {
      lines.push(`// Point #${i + 1}: ${p.color} at (${p.x}, ${p.y})`);
      lines.push(`const color${i + 1} = await ctx.getColor(${p.x}, ${p.y});`);
      lines.push(`const match${i + 1} = await ctx.colorMatch(${p.x}, ${p.y}, "${p.color}", 30);`);
      lines.push(`await ctx.click(${p.x}, ${p.y});`);
      lines.push(`// await ctx.waitForColor(${p.x}, ${p.y}, "${p.color}", 5000);`);
      if (i < points.length - 1) lines.push("");
    });
    lines.push("");
    lines.push(`// Match all points at once (options: defaultTolerance, debug, shiftMax)`);
    lines.push(`const allMatch = await ctx.colorMatchAll([`);
    points.forEach((p, i) => {
      const comma = i < points.length - 1 ? "," : "";
      lines.push(`  { x: ${p.x}, y: ${p.y}, color: "${p.color}" }${comma}`);
    });
    lines.push(`], { defaultTolerance: 32, debug: false, shiftMax: { maxDx: 50, maxDy: 50 } });`);
    return lines.join("\n");
  }
  return `// Unknown JSON type: ${json.type ?? "none"}`;
}

/** Calculate scale and pan to center an image in the viewport. */
function fitToView(
  imgW: number,
  imgH: number,
  vpW: number,
  vpH: number,
): { scale: number; pan: Pt } {
  if (vpW <= 0 || vpH <= 0 || imgW <= 0 || imgH <= 0) {
    return { scale: 1, pan: { x: 0, y: 0 } };
  }
  const padding = 40;
  const scale = Math.min((vpW - padding) / imgW, (vpH - padding) / imgH, 1);
  const panX = (vpW - imgW * scale) / 2;
  const panY = (vpH - imgH * scale) / 2;
  return { scale, pan: { x: panX, y: panY } };
}

// ─── Tool → action mapping ────────────────────────────────────────────────────

const TOOL_ACTION_MAP: Record<DrawTool, KeyAction> = {
  pan: "tool.pan",
  pick: "tool.pick",
  rect: "tool.rect",
  ellipse: "tool.ellipse",
  roundrect: "tool.roundrect",
  polygon: "tool.polygon",
};

const ACTION_TOOL_MAP: Record<string, DrawTool> = Object.fromEntries(
  Object.entries(TOOL_ACTION_MAP).map(([tool, action]) => [action, tool]),
) as Record<string, DrawTool>;

// ─── Component ────────────────────────────────────────────────────────────────

export function ImageWorkbench() {
  const [initialLang, setInitialLang] = useState<Lang>("zh");
  const [ready, setReady] = useState(false);

  useEffect(() => {
    invoke<{ language: string | null }>("wps_load_settings")
      .then((cfg) => {
        setInitialLang((cfg.language as Lang) ?? "zh");
        setReady(true);
      })
      .catch(() => setReady(true));
  }, []);

  if (!ready) return null;

  return (
    <I18nProvider initialLang={initialLang}>
      <ImageWorkbenchInner onLangPersist={(lang) => {
        invoke<SettingsDto>("wps_save_settings", {
          settings: { language: lang },
        }).catch(() => {});
      }} />
    </I18nProvider>
  );
}

function ImageWorkbenchInner({ onLangPersist }: { onLangPersist: (lang: Lang) => void }) {
  const { t, lang, setLang } = useI18n();
  // ─── State ──────────────────────────────────────────────────────────────────

  // Window & capture
  const [windows, setWindows] = useState<WindowDto[]>([]);
  const [selectedHwnd, setSelectedHwnd] = useState<number | null>(null);
  const [imageSrc, setImageSrc] = useState<string | null>(null);
  const [imgSize, setImgSize] = useState({ w: 0, h: 0 });
  const [frames, setFrames] = useState<SavedFrameDto[]>([]);
  const [cropFrames, setCropFrames] = useState<SavedFrameDto[]>([]);
  const [selectedFramePath, setSelectedFramePath] = useState<string | null>(null);

  // Viewport
  const [scale, setScale] = useState(1);
  const [pan, setPan] = useState<Pt>({ x: 0, y: 0 });

  // Tools
  const [tool, setTool] = useState<DrawTool>("pan");
  const [roundedRx, setRoundedRx] = useState(16);
  const [dragRect, setDragRect] = useState<{
    x0: number; y0: number; x1: number; y1: number;
  } | null>(null);
  const [polyPts, setPolyPts] = useState<Pt[]>([]);

  // Multi-pick
  const [pickPoints, setPickPoints] = useState<PickPoint[]>([]);
  const [pickSaveOpen, setPickSaveOpen] = useState(false);
  const [pickSaveFilename, setPickSaveFilename] = useState("");
  const pickSeqRef = useRef(0);

  // Settings
  const [saveDir, setSaveDir] = useState<string | null>(null);
  const [cropDir, setCropDir] = useState<string | null>(null);
  const [intervalMs, setIntervalMs] = useState(500);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsDraft, setSettingsDraft] = useState<SettingsDto>({
    saveDir: null, cropDir: null, intervalMs: 500, roundedRx: 16, lastHwnd: null,
    language: null, keybindings: null,
    scrollDirection: "down", scrollAmount: 120, scrollFrames: 5, scrollDelayMs: 500,
    panoDirection: "right", panoDragDistance: 500, panoFrames: 5, panoDelayMs: 300,
  });
  const [windowFilter, setWindowFilter] = useState("");

  // Recording
  const [recording, setRecording] = useState(false);
  const seqRef = useRef(0);

  // Temp space-pan
  const [spaceHeld, setSpaceHeld] = useState(false);

  // Status
  const [error, setError] = useState<string | null>(null);
  const [hoverPx, setHoverPx] = useState<{ x: number; y: number } | null>(null);
  const [pickedLabel, setPickedLabel] = useState<string | null>(null);

  // Crop save dialog
  const [cropDialogOpen, setCropDialogOpen] = useState(false);
  const [cropFilename, setCropFilename] = useState("");
  const cropDataRef = useRef<string | null>(null);
  const cropSeqRef = useRef(0);

  // Frame selection (for multi-delete)
  const [selectedFrames, setSelectedFrames] = useState<Set<string>>(new Set());
  const [selectMode, setSelectMode] = useState(false);

  // Capture progress
  const [captureProgress, setCaptureProgress] = useState<CaptureProgress | null>(null);

  // Keyboard shortcuts
  const [customKeybindings, setCustomKeybindings] = useState<Record<string, string> | null>(null);
  const [listeningAction, setListeningAction] = useState<KeyAction | null>(null);

  // Refs
  const viewportRef = useRef<HTMLDivElement>(null);
  const imgRef = useRef<HTMLImageElement>(null);
  const cropBboxRef = useRef<{ x: number; y: number; w: number; h: number } | null>(null);
  const panDragRef = useRef<{
    active: boolean; sx: number; sy: number; px: number; py: number;
  } | null>(null);

  const selectedHwndRef = useRef(selectedHwnd);
  const saveDirRef = useRef(saveDir);
  const cropDirRef = useRef(cropDir);
  const toolRef = useRef(tool);
  const spaceHeldRef = useRef(spaceHeld);
  const pickPointsRef = useRef(pickPoints);
  const settingsOpenRef = useRef(settingsOpen);
  const cropDialogOpenRef = useRef(cropDialogOpen);
  const pickSaveOpenRef = useRef(pickSaveOpen);
  const customKeybindingsRef = useRef(customKeybindings);
  const listeningActionRef = useRef(listeningAction);
  const dragRectRef = useRef(dragRect);
  const polyPtsRef = useRef(polyPts);
  const imgSizeRef = useRef(imgSize);
  const recordingRef = useRef(recording);
  const panRef = useRef(pan);
  const scaleRef = useRef(scale);

  useEffect(() => { selectedHwndRef.current = selectedHwnd; }, [selectedHwnd]);
  useEffect(() => { saveDirRef.current = saveDir; }, [saveDir]);
  useEffect(() => { cropDirRef.current = cropDir; }, [cropDir]);
  useEffect(() => { toolRef.current = tool; }, [tool]);
  useEffect(() => { spaceHeldRef.current = spaceHeld; }, [spaceHeld]);
  useEffect(() => { pickPointsRef.current = pickPoints; }, [pickPoints]);
  useEffect(() => { settingsOpenRef.current = settingsOpen; }, [settingsOpen]);
  useEffect(() => { cropDialogOpenRef.current = cropDialogOpen; }, [cropDialogOpen]);
  useEffect(() => { pickSaveOpenRef.current = pickSaveOpen; }, [pickSaveOpen]);
  useEffect(() => { customKeybindingsRef.current = customKeybindings; }, [customKeybindings]);
  useEffect(() => { listeningActionRef.current = listeningAction; }, [listeningAction]);
  useEffect(() => { dragRectRef.current = dragRect; }, [dragRect]);
  useEffect(() => { polyPtsRef.current = polyPts; }, [polyPts]);
  useEffect(() => { imgSizeRef.current = imgSize; }, [imgSize]);
  useEffect(() => { recordingRef.current = recording; }, [recording]);
  useEffect(() => { panRef.current = pan; }, [pan]);
  useEffect(() => { scaleRef.current = scale; }, [scale]);

  // Build keymap from custom bindings
  const keyMap = useMemo(
    () => buildKeyMap(customKeybindings),
    [customKeybindings],
  );

  // ─── Data loading ───────────────────────────────────────────────────────────

  const applyDataUrl = useCallback(async (url: string, center = true) => {
    await new Promise<void>((resolve, reject) => {
      const probe = new Image();
      probe.onload = () => {
        const w = probe.naturalWidth;
        const h = probe.naturalHeight;
        setImgSize({ w, h });
        setImageSrc(url);
        if (center) {
          const vp = viewportRef.current?.getBoundingClientRect();
          if (vp) {
            const { scale: s, pan: p } = fitToView(w, h, vp.width, vp.height);
            setScale(s);
            setPan(p);
          }
        }
        resolve();
      };
      probe.onerror = () => reject(new Error("Image load failed"));
      probe.src = url;
    });
  }, []);

  const refreshWindows = useCallback(async () => {
    setError(null);
    try {
      const list = await invoke<WindowDto[]>("wps_list_windows");
      setWindows(list);
      if (selectedHwndRef.current !== null && !list.some((w) => w.hwnd === selectedHwndRef.current)) {
        setSelectedHwnd(null);
      }
    } catch (e) { setError(String(e)); }
  }, []);

  const refreshFrames = useCallback(async (dirOverride?: string | null) => {
    const dir = dirOverride ?? saveDirRef.current;
    if (!dir) { setFrames([]); setSelectedFramePath(null); return; }
    try {
      const list = await invoke<SavedFrameDto[]>("wps_list_saved_frames", { dir });
      setFrames(list);
      if (selectedFramePath && !list.some((x) => x.path === selectedFramePath)) {
        setSelectedFramePath(null);
      }
    } catch (e) { setError(String(e)); }
  }, [selectedFramePath]);

  const refreshCropFrames = useCallback(async (dirOverride?: string | null) => {
    const dir = dirOverride ?? cropDirRef.current;
    if (!dir) { setCropFrames([]); return; }
    try {
      const list = await invoke<SavedFrameDto[]>("wps_list_saved_frames", { dir });
      setCropFrames(list);
    } catch (e) { setError(String(e)); }
  }, []);

  const loadFrameFromPath = useCallback(async (path: string) => {
    try {
      setError(null);
      const b64 = await invoke<string>("wps_read_frame_base64", { path });
      const url = `data:image/png;base64,${b64}`;
      await applyDataUrl(url);
      setSelectedFramePath(path);
      setDragRect(null);
      setPolyPts([]);
      setPickPoints([]);
    } catch (e) { setError(String(e)); }
  }, [applyDataUrl]);

  const loadSettings = useCallback(async () => {
    try {
      const cfg = await invoke<SettingsDto>("wps_load_settings");
      setSaveDir(cfg.saveDir);
      setCropDir(cfg.cropDir ?? null);
      setIntervalMs(cfg.intervalMs);
      setRoundedRx(cfg.roundedRx);
      setSettingsDraft(cfg);
      setCustomKeybindings(cfg.keybindings ?? null);

      if (cfg.lastHwnd !== null) {
        const list = await invoke<WindowDto[]>("wps_list_windows");
        if (list.some((w) => w.hwnd === cfg.lastHwnd)) setSelectedHwnd(cfg.lastHwnd);
      }

      await refreshFrames(cfg.saveDir);
      await refreshCropFrames(cfg.cropDir);
    } catch (e) { setError(String(e)); }
  }, [refreshFrames, refreshCropFrames]);

  const persistSettings = useCallback(async (draft: SettingsDto) => {
    try {
      const cfg = await invoke<SettingsDto>("wps_save_settings", { settings: draft });
      setSaveDir(cfg.saveDir);
      setCropDir(cfg.cropDir ?? null);
      setIntervalMs(cfg.intervalMs);
      setRoundedRx(cfg.roundedRx);
      setSettingsDraft(cfg);
      setCustomKeybindings(cfg.keybindings ?? null);
      await refreshFrames(cfg.saveDir);
      await refreshCropFrames(cfg.cropDir);
      return cfg;
    } catch (e) { setError(String(e)); return null; }
  }, [refreshFrames, refreshCropFrames]);

  const persistHwnd = useCallback(async (hwnd: number | null) => {
    const updated = { ...settingsDraft, lastHwnd: hwnd };
    setSettingsDraft(updated);
    await persistSettings(updated);
  }, [settingsDraft, persistSettings]);

  useEffect(() => {
    void refreshWindows();
    void loadSettings();
    // Listen for capture progress events
    let unlisten: (() => void) | null = null;
    listen<CaptureProgress>("wps-capture-progress", (event) => {
      setCaptureProgress(event.payload);
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, [refreshWindows, loadSettings]);

  // ─── Capture ────────────────────────────────────────────────────────────────

  const captureOnce = useCallback(async (opts?: { silent?: boolean; persistFrame?: boolean }) => {
    const hwnd = selectedHwndRef.current;
    if (hwnd === null) {
      if (!opts?.silent) setError(t("error.selectWindow"));
      return null;
    }
    setError(null);
    try {
      // Use client-area capture (no title bar)
      const cap = await invoke<CaptureDto>("wps_capture_client", { hwnd });
      const url = `data:image/png;base64,${cap.pngBase64}`;
      await applyDataUrl(url);
      setImgSize({ w: cap.width, h: cap.height });
      setSelectedFramePath(null);

      if (opts?.persistFrame !== false && saveDirRef.current) {
        seqRef.current += 1;
        const name = `wps_${Date.now()}_${seqRef.current}.png`;
        const savedPath = await invoke<string>("wps_save_png", {
          path: joinPath(saveDirRef.current, name), data: url,
        });
        setSelectedFramePath(savedPath);
        await refreshFrames(saveDirRef.current);
      }
      return url;
    } catch (e) { setError(String(e)); return null; }
  }, [applyDataUrl, refreshFrames]);

  useEffect(() => {
    if (!recording) return;
    const tick = () => { void captureOnce({ silent: true, persistFrame: true }); };
    tick();
    const id = window.setInterval(tick, intervalMs);
    return () => clearInterval(id);
  }, [recording, intervalMs, captureOnce]);

  useEffect(() => {
    let alive = true;
    const pending: Array<() => void> = [];
    void listen("wps-hotkey-capture", () => { void captureOnce(); }).then((u) => {
      if (alive) pending.push(u); else u();
    });
    void listen("wps-hotkey-toggle-record", () => { setRecording((r) => !r); }).then((u) => {
      if (alive) pending.push(u); else u();
    });
    return () => { alive = false; pending.forEach((u) => u()); };
  }, [captureOnce]);

  // ─── Viewport math ──────────────────────────────────────────────────────────

  const viewportToImage = useCallback(
    (clientX: number, clientY: number): Pt | null => {
      const vp = viewportRef.current?.getBoundingClientRect();
      if (!vp) return null;
      return {
        x: (clientX - vp.left - pan.x) / scale,
        y: (clientY - vp.top - pan.y) / scale,
      };
    },
    [pan, scale],
  );

  /** Ref-based version: always reads latest pan/scale from refs. */
  const viewportToImageLive = useCallback(
    (clientX: number, clientY: number): Pt | null => {
      const vp = viewportRef.current?.getBoundingClientRect();
      if (!vp) return null;
      const p = panRef.current;
      const s = scaleRef.current;
      return {
        x: (clientX - vp.left - p.x) / s,
        y: (clientY - vp.top - p.y) / s,
      };
    },
    [],
  );

  const onWheel = useCallback((e: React.WheelEvent) => {
    // Handled by native listener below — this is a no-op fallback
  }, []);

  // ─── Pixel pick ─────────────────────────────────────────────────────────────

  const getPixelAt = useCallback((x: number, y: number): PickPoint | null => {
    const img = imgRef.current;
    if (!img || !img.complete || img.naturalWidth === 0) return null;
    const canvas = document.createElement("canvas");
    canvas.width = img.naturalWidth;
    canvas.height = img.naturalHeight;
    const ctx = canvas.getContext("2d")!;
    ctx.drawImage(img, 0, 0);
    const ix = clamp(Math.floor(x), 0, img.naturalWidth - 1);
    const iy = clamp(Math.floor(y), 0, img.naturalHeight - 1);
    const d = ctx.getImageData(ix, iy, 1, 1).data;
    const [r, g, b, a] = d;
    const hex = `#${[r, g, b].map((c) => c.toString(16).padStart(2, "0")).join("")}`.toUpperCase();
    return { x: ix, y: iy, hex, rgba: [r, g, b, a] };
  }, []);

  const pickPixelLabel = useCallback((x: number, y: number): string | null => {
    const p = getPixelAt(x, y);
    if (!p) return null;
    return `${p.hex} · RGBA(${p.rgba[0]}, ${p.rgba[1]}, ${p.rgba[2]}, ${p.rgba[3]}) · (${p.x}, ${p.y})`;
  }, [getPixelAt]);

  // ─── Mouse handlers ─────────────────────────────────────────────────────────

  const onMouseMoveVp = useCallback((e: React.MouseEvent) => {
    if (dragRectRef.current) console.info("[mouseMove] fired, button:", e.button, "buttons:", e.buttons);
    const p = viewportToImageLive(e.clientX, e.clientY);
    const isz = imgSizeRef.current;
    if (!p || isz.w === 0) { setHoverPx(null); return; }
    setHoverPx(p.x >= 0 && p.x < isz.w && p.y >= 0 && p.y < isz.h ? p : null);

    if (panDragRef.current?.active) {
      const d = panDragRef.current;
      setPan({ x: d.px + (e.clientX - d.sx), y: d.py + (e.clientY - d.sy) });
      return;
    }

    const t = toolRef.current;
    const dr = dragRectRef.current;
    if (dr && (t === "rect" || t === "ellipse" || t === "roundrect")) {
      console.info("[mouseMove] updating drag from", dr.x1, dr.y1, "to", p.x, p.y);
      setDragRect((prev) => prev ? { ...prev, x1: p.x, y1: p.y } : prev);
    }
  }, [viewportToImageLive]);

  // Attach native mousemove to viewport to avoid React synthetic event issues
  useEffect(() => {
    const vp = viewportRef.current;
    if (!vp) return;
    const handler = (e: globalThis.MouseEvent) => {
      const p = viewportToImageLive(e.clientX, e.clientY);
      const isz = imgSizeRef.current;
      if (!p || isz.w === 0) return;

      if (panDragRef.current?.active) {
        const d = panDragRef.current;
        setPan({ x: d.px + (e.clientX - d.sx), y: d.py + (e.clientY - d.sy) });
        return;
      }

      const t = toolRef.current;
      const dr = dragRectRef.current;
      if (dr && (t === "rect" || t === "ellipse" || t === "roundrect")) {
        setDragRect((prev) => prev ? { ...prev, x1: p.x, y1: p.y } : prev);
      }
    };
    vp.addEventListener("mousemove", handler);

    const wheelHandler = (e: WheelEvent) => {
      e.preventDefault();
      const rect = vp.getBoundingClientRect();
      const s = scaleRef.current;
      const p = panRef.current;
      const factor = e.deltaY > 0 ? 0.9 : 1.1;
      const next = clamp(s * factor, 0.04, 32);
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      const ratio = next / s;
      setPan({ x: mx - ratio * (mx - p.x), y: my - ratio * (my - p.y) });
      setScale(next);
    };
    vp.addEventListener("wheel", wheelHandler, { passive: false });

    return () => {
      vp.removeEventListener("mousemove", handler);
      vp.removeEventListener("wheel", wheelHandler);
    };
  }, [viewportToImageLive]);

  // ─── Selection → crop ───────────────────────────────────────────────────────

  const getCropDataUrl = useCallback((): string | null => {
    const img = imgRef.current;
    console.info("[getCropDataUrl] img:", !!img, "img.src:", img?.src?.slice(0, 60), "imgSize.w:", imgSize.w, "tool:", tool);
    if (!img?.src || imgSize.w === 0) { console.info("[getCropDataUrl] early return: no img or imgSize"); return null; }
    let bbox: { x: number; y: number; w: number; h: number };
    let clip: (ctx: CanvasRenderingContext2D) => void;

    if (tool === "rect" || tool === "ellipse" || tool === "roundrect") {
      const dr = dragRectRef.current;
      console.info("[getCropDataUrl] dragRectRef.current:", dr);
      if (!dr) { console.info("[getCropDataUrl] early return: no dragRect"); return null; }
      const nr = normalizeDrag(dr.x0, dr.y0, dr.x1, dr.y1);
      console.info("[getCropDataUrl] normalized:", nr);
      if (nr.w < 2 || nr.h < 2) { console.info("[getCropDataUrl] early return: too small"); return null; }
      bbox = nr;
      if (tool === "rect") clip = (ctx) => ctx.rect(nr.x, nr.y, nr.w, nr.h);
      else if (tool === "ellipse") clip = (ctx) => ctx.ellipse(nr.x + nr.w / 2, nr.y + nr.h / 2, nr.w / 2, nr.h / 2, 0, 0, Math.PI * 2);
      else { const rr = Math.min(roundedRx, nr.w / 2, nr.h / 2); clip = (ctx) => ctx.roundRect(nr.x, nr.y, nr.w, nr.h, rr); }
    } else if (tool === "polygon") {
      if (polyPts.length < 3) return null;
      bbox = polyBBox(polyPts);
      if (bbox.w < 2 || bbox.h < 2) return null;
      const pts = polyPts;
      clip = (ctx) => { ctx.moveTo(pts[0].x, pts[0].y); for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i].x, pts[i].y); ctx.closePath(); };
    } else { console.info("[getCropDataUrl] early return: tool is", tool); return null; }

    cropBboxRef.current = { x: Math.round(bbox.x), y: Math.round(bbox.y), w: Math.round(bbox.w), h: Math.round(bbox.h) };
    const result = exportClipped(img, bbox, clip);
    console.info("[getCropDataUrl] result length:", result?.length);
    return result;
  }, [imgSize.w, polyPts, roundedRx, tool]);

  const showCropDialog = useCallback(() => {
    console.info("[showCropDialog] called");
    const dataUrl = getCropDataUrl();
    console.info("[showCropDialog] dataUrl:", dataUrl ? `${dataUrl.length} chars` : "null");
    if (!dataUrl) { setError(t("error.selectRegion")); return; }
    cropDataRef.current = dataUrl;
    const dir = cropDirRef.current ?? saveDirRef.current;
    if (dir) {
      invoke<number>("wps_next_counter", { dir }).then((n) => {
        setCropFilename(`crop_${n}`);
      }).catch(() => {
        cropSeqRef.current += 1;
        setCropFilename(`crop_${cropSeqRef.current}`);
      });
    } else {
      cropSeqRef.current += 1;
      setCropFilename(`crop_${cropSeqRef.current}`);
    }
    setCropDialogOpen(true);
    setError(null);
    console.info("[showCropDialog] dialog opened");
  }, [getCropDataUrl, t]);

  const saveCrop = useCallback(async () => {
    const dataUrl = cropDataRef.current;
    const dir = cropDir ?? saveDir;
    if (!dataUrl || !dir) { setError(t("error.noSelectionOrDir")); return; }
    const filename = cropFilename.trim();
    if (!filename) { setError(t("error.enterFilename")); return; }
    const path = joinPath(dir, `${filename}.png`);
    try {
      await invoke("wps_save_png", { path, data: dataUrl });
      // Save crop position sidecar JSON
      const bbox = cropBboxRef.current;
      if (bbox) {
        const jsonPath = joinPath(dir, `${filename}.json`);
        const jsonData = JSON.stringify({ type: "crop", ...bbox }, null, 2);
        await invoke("wps_save_json", { path: jsonPath, data: jsonData });
      }
      await refreshCropFrames(dir);
      setCropDialogOpen(false);
      cropDataRef.current = null;
      setDragRect(null);
      dragRectRef.current = null;
      setPolyPts([]);
      setTool("pan");
      setError(null);
    } catch (e) { setError(String(e)); }
  }, [cropFilename, cropDir, saveDir, refreshCropFrames]);

  // ─── Multi-pick save ────────────────────────────────────────────────────────

  const savePickPoints = useCallback(async () => {
    if (pickPoints.length === 0) return;
    const dir = cropDir ?? saveDir;
    if (!dir) { setError(t("error.setCropDir")); return; }
    const filename = pickSaveFilename.trim();
    if (!filename) { setError(t("error.enterFilename")); return; }
    const data = JSON.stringify({
      type: "pick",
      points: pickPoints.map((p) => ({ x: p.x, y: p.y, color: p.hex, rgba: p.rgba })),
    }, null, 2);
    const path = joinPath(dir, `${filename}.json`);
    try {
      await invoke<string>("wps_save_json", { path, data });
      await refreshCropFrames(dir);
      setPickSaveOpen(false);
      setPickPoints([]);
      setPickSaveFilename("");
      setError(null);
    } catch (e) { setError(String(e)); }
  }, [pickPoints, pickSaveFilename, cropDir, saveDir, refreshCropFrames]);

  // ─── Frame deletion ─────────────────────────────────────────────────────────

  const deleteFrame = useCallback(async (path: string) => {
    try {
      await invoke("wps_delete_file", { path });
      await refreshFrames(saveDir);
      await refreshCropFrames(cropDir);
    } catch (e) { setError(String(e)); }
  }, [saveDir, cropDir, refreshFrames, refreshCropFrames]);

  const deleteSelectedFrames = useCallback(async () => {
    if (selectedFrames.size === 0) return;
    try {
      await invoke("wps_delete_files", { paths: Array.from(selectedFrames) });
      setSelectedFrames(new Set());
      setSelectMode(false);
      await refreshFrames(saveDir);
      await refreshCropFrames(cropDir);
    } catch (e) { setError(String(e)); }
  }, [selectedFrames, saveDir, cropDir, refreshFrames, refreshCropFrames]);

  const clearAllFrames = useCallback(async (dir: string) => {
    try {
      const count = await invoke<number>("wps_clear_directory", { dir });
      if (dir === saveDir) { await refreshFrames(dir); }
      if (dir === cropDir) { await refreshCropFrames(dir); }
      setPickedLabel(t("status.cleared", count));
      window.setTimeout(() => setPickedLabel(null), 2000);
    } catch (e) { setError(String(e)); }
  }, [saveDir, cropDir, refreshFrames, refreshCropFrames]);

  const toggleFrameSelect = useCallback((path: string) => {
    setSelectedFrames((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  // ─── Reset / center view ────────────────────────────────────────────────────

  const resetView = useCallback(() => {
    const vp = viewportRef.current?.getBoundingClientRect();
    if (vp && imgSize.w > 0 && imgSize.h > 0) {
      const { scale: s, pan: p } = fitToView(imgSize.w, imgSize.h, vp.width, vp.height);
      setScale(s);
      setPan(p);
    } else {
      setScale(1);
      setPan({ x: 0, y: 0 });
    }
  }, [imgSize]);

  // ─── Keyboard shortcuts ─────────────────────────────────────────────────────

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      // If listening for a keybinding, capture it
      if (listeningActionRef.current) {
        e.preventDefault();
        e.stopPropagation();
        const action = listeningActionRef.current;
        const key = e.key.toLowerCase();
        setCustomKeybindings((prev) => {
          const next = { ...(prev ?? {}), [action]: key };
          // Persist to settings
          const draft = { ...settingsDraft, keybindings: next };
          void persistSettings(draft);
          return next;
        });
        setListeningAction(null);
        return;
      }

      // Skip when typing in inputs
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement) return;

      // Skip when dialogs are open
      if (settingsOpenRef.current || cropDialogOpenRef.current || pickSaveOpenRef.current) {
        if (e.key === "Escape") {
          setSettingsOpen(false); setCropDialogOpen(false); setPickSaveOpen(false);
        }
        return;
      }

      const key = e.key.toLowerCase();
      const action = keyMap.get(key);

      // Space → temp pan
      if (key === " " && !spaceHeldRef.current) {
        e.preventDefault();
        setSpaceHeld(true);
        return;
      }

      // Selection cancel
      if (action === "selection.cancel" || e.key === "Escape") {
        if (pickPointsRef.current.length > 0) setPickPoints([]);
        else if (toolRef.current === "polygon" && polyPtsRef.current.length > 0) setPolyPts([]);
        else if (dragRectRef.current) setDragRect(null);
        return;
      }

      // Selection confirm / pick save
      if (action === "selection.confirm" || e.key === "Enter") {
        if (pickPointsRef.current.length > 0) {
          const dir = cropDirRef.current ?? saveDirRef.current;
          if (dir) {
            invoke<number>("wps_next_counter", { dir }).then((n) => {
              setPickSaveFilename(`pick_${n}`);
            }).catch(() => {
              pickSeqRef.current += 1;
              setPickSaveFilename(`pick_${pickSeqRef.current}`);
            });
          } else {
            pickSeqRef.current += 1;
            setPickSaveFilename(`pick_${pickSeqRef.current}`);
          }
          setPickSaveOpen(true);
          return;
        }
        if (toolRef.current === "polygon" && polyPtsRef.current.length >= 3) {
          showCropDialog();
          return;
        }
        return;
      }

      // View reset
      if (action === "view.reset") { resetView(); return; }

      // Zoom
      if (action === "view.zoomIn") { setScale((s) => clamp(s * 1.2, 0.04, 32)); return; }
      if (action === "view.zoomOut") { setScale((s) => clamp(s / 1.2, 0.04, 32)); return; }

      // Capture
      if (action === "capture.screenshot") { void captureOnce({ persistFrame: true }); return; }
      if (action === "capture.toggleRecord") { setRecording((r) => !r); return; }

      // Tool shortcuts
      const toolForAction = ACTION_TOOL_MAP[action ?? ""];
      if (toolForAction) {
        setTool(toolForAction);
        setDragRect(null);
        if (toolForAction !== "polygon") setPolyPts([]);
        if (toolForAction !== "pick") setPickPoints([]);
        return;
      }
    };

    const onKeyUp = (e: KeyboardEvent) => {
      if (e.key === " ") setSpaceHeld(false);
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
    };
  }, [keyMap, showCropDialog, resetView, captureOnce, settingsDraft, persistSettings]);

  // ─── Advanced capture (scroll/panoramic) ────────────────────────────────────

  const scrollCapture = useCallback(async () => {
    if (!selectedHwnd) { setError(t("error.selectWindow")); return; }
    setError(null);
    setCaptureProgress({ current: 0, total: settingsDraft.scrollFrames ?? 5, phase: t("misc.starting") });
    try {
      const cap = await invoke<CaptureDto>("wps_scroll_capture", {
        hwnd: selectedHwnd,
        direction: settingsDraft.scrollDirection ?? "down",
        scrollAmount: settingsDraft.scrollAmount ?? 120,
        frameCount: settingsDraft.scrollFrames ?? 5,
        delayMs: settingsDraft.scrollDelayMs ?? 500,
      });
      const url = `data:image/png;base64,${cap.pngBase64}`;
      await applyDataUrl(url);
      if (saveDirRef.current) {
        seqRef.current += 1;
        const name = `scroll_${Date.now()}_${seqRef.current}.png`;
        await invoke<string>("wps_save_png", { path: joinPath(saveDirRef.current, name), data: url });
        await refreshFrames(saveDirRef.current);
      }
    } catch (e) { setError(String(e)); }
    setCaptureProgress(null);
  }, [selectedHwnd, settingsDraft, applyDataUrl, refreshFrames]);

  const panoramicCapture = useCallback(async () => {
    if (!selectedHwnd) { setError(t("error.selectWindow")); return; }
    setError(null);
    setCaptureProgress({ current: 0, total: settingsDraft.panoFrames ?? 5, phase: t("misc.starting") });
    try {
      const cap = await invoke<CaptureDto>("wps_panoramic_capture", {
        hwnd: selectedHwnd,
        direction: settingsDraft.panoDirection ?? "right",
        dragDistance: settingsDraft.panoDragDistance ?? 500,
        frameCount: settingsDraft.panoFrames ?? 5,
        delayMs: settingsDraft.panoDelayMs ?? 300,
      });
      const url = `data:image/png;base64,${cap.pngBase64}`;
      await applyDataUrl(url);
      if (saveDirRef.current) {
        seqRef.current += 1;
        const name = `pano_${Date.now()}_${seqRef.current}.png`;
        await invoke<string>("wps_save_png", { path: joinPath(saveDirRef.current, name), data: url });
        await refreshFrames(saveDirRef.current);
      }
    } catch (e) { setError(String(e)); }
    setCaptureProgress(null);
  }, [selectedHwnd, settingsDraft, applyDataUrl, refreshFrames]);

  // ─── Derived ────────────────────────────────────────────────────────────────

  const hoverLabel = hoverPx ? pickPixelLabel(hoverPx.x, hoverPx.y) : null;
  const filteredWindows = windows.filter((w) =>
    !windowFilter.trim() || w.title.toLowerCase().includes(windowFilter.toLowerCase()),
  );
  const selectedWindowTitle = selectedHwnd != null
    ? windows.find((w) => w.hwnd === selectedHwnd)?.title ?? `HWND ${selectedHwnd}`
    : null;
  const currentToolIsPan = tool === "pan" || spaceHeld;

  const handleToolChange = (t: DrawTool) => {
    setTool(t);
    setDragRect(null);
    if (t !== "polygon") setPolyPts([]);
    if (t !== "pick") setPickPoints([]);
  };

  // ─── SVG overlays ───────────────────────────────────────────────────────────

  const shapePreview = () => {
    if (!dragRect || (tool !== "rect" && tool !== "ellipse" && tool !== "roundrect")) return null;
    const nr = normalizeDrag(dragRect.x0, dragRect.y0, dragRect.x1, dragRect.y1);
    const stroke = "var(--color-primary)";
    const fill = "color-mix(in oklch, var(--color-primary) 18%, transparent)";
    const sw = 2 / scale;
    if (tool === "rect") return <rect x={nr.x} y={nr.y} width={nr.w} height={nr.h} stroke={stroke} strokeWidth={sw} fill={fill} />;
    if (tool === "ellipse") return <ellipse cx={nr.x + nr.w / 2} cy={nr.y + nr.h / 2} rx={nr.w / 2} ry={nr.h / 2} stroke={stroke} strokeWidth={sw} fill={fill} />;
    const rr = Math.min(roundedRx, nr.w / 2, nr.h / 2);
    return <rect x={nr.x} y={nr.y} width={nr.w} height={nr.h} rx={rr} ry={rr} stroke={stroke} strokeWidth={sw} fill={fill} />;
  };

  const polyPreview = () => {
    if (tool !== "polygon" || polyPts.length === 0) return null;
    const openD = polyPts.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x} ${p.y}`).join(" ");
    const fillD = polyPts.length >= 3 ? `${openD} Z` : openD;
    return (
      <>
        <path d={openD} fill="none" stroke="var(--color-primary)" strokeWidth={2 / scale} />
        {polyPts.length >= 3 && <path d={fillD} fill="color-mix(in oklch, var(--color-primary) 12%, transparent)" stroke="none" />}
        {polyPts.map((p, i) => <circle key={`${p.x}-${p.y}-${i}`} cx={p.x} cy={p.y} r={4 / scale} fill="var(--color-primary)" />)}
      </>
    );
  };

  const pickPointsOverlay = () => {
    if (pickPoints.length === 0) return null;
    return (
      <>
        {pickPoints.map((p, i) => {
          const r = 10 / scale;
          const fontSize = 9 / scale;
          return (
            <g key={`pick-${i}`}>
              <circle cx={p.x} cy={p.y} r={r} fill={p.hex} stroke="white" strokeWidth={1.5 / scale} />
              <text x={p.x} y={p.y} textAnchor="middle" dominantBaseline="central"
                fill="white" fontSize={fontSize} fontWeight="bold"
                style={{ paintOrder: "stroke", stroke: "black", strokeWidth: 2 / scale }}>
                {i + 1}
              </text>
            </g>
          );
        })}
      </>
    );
  };

  // ─── Render ─────────────────────────────────────────────────────────────────

  return (
    <TooltipProvider>
      <div className="flex min-h-0 flex-1 flex-col gap-2 bg-background p-3">
        <div className="flex min-h-0 flex-1 gap-3">
          {/* ── Left sidebar: screenshot frames ────────────────────────── */}
          <aside className="flex w-64 shrink-0 flex-col rounded-lg border border-border bg-card/80 text-sm">
            <div className="flex items-center justify-between border-b border-border px-3 py-2">
              <span className="text-xs text-foreground-tertiary">
                {t("sidebar.screenshots", frames.length)}
              </span>
              <div className="flex items-center gap-1">
                {selectMode && selectedFrames.size > 0 && (
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button variant="destructive" size="icon" className="size-6"
                        onClick={() => void deleteSelectedFrames()}>
                        <Trash2 className="size-3" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>{t("tooltip.deleteSelected", selectedFrames.size)}</TooltipContent>
                  </Tooltip>
                )}
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button variant={selectMode ? "default" : "ghost"} size="icon" className="size-6"
                      onClick={() => { setSelectMode(!selectMode); setSelectedFrames(new Set()); }}>
                      <CheckSquare className="size-3" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>{selectMode ? t("tooltip.exitSelect") : t("tooltip.multiSelect")}</TooltipContent>
                </Tooltip>
                {saveDir && (
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button variant="ghost" size="icon" className="size-6"
                        onClick={() => void clearAllFrames(saveDir)}>
                        <Trash2 className="size-3 text-destructive" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>{t("tooltip.clearAll")}</TooltipContent>
                  </Tooltip>
                )}
              </div>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto p-2">
              {frames.length === 0 ? (
                <div className="px-2 py-3 text-xs text-foreground-tertiary">
                  {t("sidebar.noFrames")}
                </div>
              ) : (
                <div className="space-y-1">
                  {frames.map((frame) => (
                    <div key={frame.path} className="group relative">
                      <button type="button"
                        className={cn(
                          "w-full rounded-md border px-2 py-2 text-left text-xs",
                          selectedFramePath === frame.path
                            ? "border-primary bg-primary/10 text-foreground"
                            : selectedFrames.has(frame.path)
                              ? "border-primary/50 bg-primary/5 text-foreground"
                              : "border-border bg-surface text-foreground-secondary hover:bg-surface-hover",
                        )}
                        onClick={() => {
                          if (selectMode) { toggleFrameSelect(frame.path); return; }
                          void loadFrameFromPath(frame.path);
                        }}
                      >
                        {selectMode && (
                          <span className="mr-1.5 inline-block">
                            {selectedFrames.has(frame.path)
                              ? <SquareCheck className="size-3 text-primary" />
                              : <Square className="size-3 text-foreground-tertiary" />}
                          </span>
                        )}
                        <div className="truncate font-medium">{frame.name}</div>
                        <div className="mt-1 text-[11px] text-foreground-tertiary">{formatTime(frame.modifiedMs)}</div>
                      </button>
                      {!selectMode && (
                        <button
                          className="absolute right-1 top-1 hidden rounded p-0.5 text-foreground-tertiary hover:bg-destructive/10 hover:text-destructive group-hover:block"
                          onClick={(e) => { e.stopPropagation(); void deleteFrame(frame.path); }}
                        >
                          <X className="size-3" />
                        </button>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
          </aside>

          {/* ── Main viewport ──────────────────────────────────────────── */}
          <div className="relative flex min-h-0 min-w-0 flex-1 flex-col rounded-lg border border-border bg-card">
            <div
              ref={viewportRef}
              className={cn(
                "relative min-h-0 flex-1 overflow-hidden bg-surface select-none",
                currentToolIsPan ? "cursor-grab" : "cursor-crosshair",
              )}
              onWheel={onWheel}
              onMouseMove={onMouseMoveVp}
              onMouseDown={(e) => {
                const p = viewportToImageLive(e.clientX, e.clientY);
                if (!p) return;

                if (e.button === 1 || (currentToolIsPan && e.button === 0)) {
                  panDragRef.current = { active: true, sx: e.clientX, sy: e.clientY, px: pan.x, py: pan.y };
                  e.preventDefault();
                  return;
                }
                if (e.button !== 0) return;

                if (tool === "pick") {
                  if (e.altKey) {
                    const pixel = getPixelAt(p.x, p.y);
                    if (pixel) setPickPoints((prev) => [...prev, pixel]);
                  } else {
                    const label = pickPixelLabel(p.x, p.y);
                    if (label) {
                      const hex = label.split(" · ")[0];
                      void writeText(`${hex} ${label}`);
                      setPickedLabel(t("status.copied", hex));
                      window.setTimeout(() => setPickedLabel(null), 2000);
                    }
                  }
                  return;
                }

                if (tool === "rect" || tool === "ellipse" || tool === "roundrect") {
                  console.info("[onMouseDown] starting drag at", p.x, p.y, "tool:", tool);
                  const dr = { x0: p.x, y0: p.y, x1: p.x, y1: p.y };
                  dragRectRef.current = dr;
                  setDragRect(dr);
                }
                if (tool === "polygon") setPolyPts((prev) => [...prev, { x: p.x, y: p.y }]);
              }}
              onMouseUp={() => {
                panDragRef.current = null;
                const dr = dragRectRef.current;
                console.info("[onMouseUp] tool:", tool, "dragRectRef:", dr, "dragRect state:", dragRect);
                if ((tool === "rect" || tool === "ellipse" || tool === "roundrect") && dr
                  && Math.abs(dr.x1 - dr.x0) > 2 && Math.abs(dr.y1 - dr.y0) > 2) {
                  console.info("[onMouseUp] calling showCropDialog via rAF");
                  requestAnimationFrame(() => showCropDialog());
                } else {
                  console.info("[onMouseUp] condition not met, skipping crop dialog");
                }
              }}
              onMouseLeave={() => { panDragRef.current = null; setHoverPx(null); }}
              onContextMenu={(e) => {
                if (tool === "polygon") { e.preventDefault(); setPolyPts((prev) => prev.slice(0, -1)); }
              }}
            >
              {/* ── Centered floating toolbar ───────────────────────────── */}
              <div className="pointer-events-none absolute top-3 left-1/2 z-20 -translate-x-1/2">
                <div className="pointer-events-auto inline-flex items-center gap-1.5 rounded-full border border-border bg-background/95 px-3 py-1.5 shadow-lg backdrop-blur-sm">
                  {/* Window info */}
                  <span className="max-w-32 truncate text-xs text-foreground-secondary">
                    {selectedWindowTitle ?? t("misc.noWindow")}
                  </span>
                  <div className="w-px h-5 bg-border" />
                  {/* Capture actions */}
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button variant="outline" size="icon" className="size-7"
                        onClick={() => void captureOnce({ persistFrame: true })}>
                        <Camera className="size-3.5" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>{t("tooltip.capture")}</TooltipContent>
                  </Tooltip>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button variant={recording ? "destructive" : "outline"} size="icon" className="size-7"
                        onClick={() => setRecording((r) => !r)}>
                        <CircleDot className="size-3.5" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>{recording ? t("tooltip.stopRecording") : t("tooltip.startRecording")}</TooltipContent>
                  </Tooltip>
                  <div className="w-px h-5 bg-border" />
                  {/* Tools */}
                  {([
                    { id: "pan" as DrawTool, icon: <Move className="size-3.5" />, labelKey: "tooltip.pan" },
                    { id: "pick" as DrawTool, icon: <Pipette className="size-3.5" />, labelKey: "tooltip.pick" },
                    { id: "rect" as DrawTool, icon: <Square className="size-3.5" />, labelKey: "tooltip.rect" },
                    { id: "ellipse" as DrawTool, icon: <Circle className="size-3.5" />, labelKey: "tooltip.ellipse" },
                    { id: "roundrect" as DrawTool, icon: <SquareRoundCorner className="size-3.5" />, labelKey: "tooltip.roundRect" },
                    { id: "polygon" as DrawTool, icon: <Pentagon className="size-3.5" />, labelKey: "tooltip.polygon" },
                  ]).map(({ id, icon, labelKey }) => (
                    <Tooltip key={id}>
                      <TooltipTrigger asChild>
                        <Button variant={tool === id ? "default" : "outline"} size="icon" className="size-7"
                          onClick={() => handleToolChange(id)}>
                          {icon}
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>{t(labelKey as Parameters<typeof t>[0])}</TooltipContent>
                    </Tooltip>
                  ))}
                  <div className="w-px h-5 bg-border" />
                  {/* Actions */}
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button variant="outline" size="icon" className="size-7" onClick={resetView}>
                        <Maximize className="size-3.5" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>{t("tooltip.fitView")}</TooltipContent>
                  </Tooltip>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button size="icon" className="size-7"
                        disabled={!imageSrc || !["rect", "ellipse", "roundrect", "polygon"].includes(tool)}
                        onClick={() => showCropDialog()}>
                        <Download className="size-3.5" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>{t("tooltip.saveSelection")}</TooltipContent>
                  </Tooltip>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button variant="outline" size="icon" className="size-7"
                        disabled={!imageSrc || !saveDir}
                        onClick={() => {
                          if (!imageSrc || !saveDir) return;
                          const name = `full_${Date.now()}.png`;
                          invoke<string>("wps_save_png", { path: joinPath(saveDir, name), data: imageSrc })
                            .then(() => refreshFrames(saveDir))
                            .catch((e) => setError(String(e)));
                        }}>
                        <Save className="size-3.5" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>{t("tooltip.saveFull")}</TooltipContent>
                  </Tooltip>
                  <div className="w-px h-5 bg-border" />
                  {/* Advanced capture */}
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button variant="outline" size="icon" className="size-7"
                        disabled={!selectedHwnd || !!captureProgress}
                        onClick={() => void scrollCapture()}>
                        <Scissors className="size-3.5" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>{t("tooltip.scrollCapture")}</TooltipContent>
                  </Tooltip>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button variant="outline" size="icon" className="size-7"
                        disabled={!selectedHwnd || !!captureProgress}
                        onClick={() => void panoramicCapture()}>
                        <Maximize className="size-3.5" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>{t("tooltip.panoCapture")}</TooltipContent>
                  </Tooltip>
                  <div className="w-px h-5 bg-border" />
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button variant="outline" size="icon" className="size-7"
                        onClick={() => setSettingsOpen(true)}>
                        <Settings className="size-3.5" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>{t("tooltip.settings")}</TooltipContent>
                  </Tooltip>
                </div>
              </div>

              {/* ── Capture progress overlay ────────────────────────────── */}
              {captureProgress && (
                <div className="pointer-events-none absolute inset-x-0 top-16 z-30 flex justify-center">
                  <div className="rounded-lg border border-border bg-background/95 px-4 py-3 shadow-lg backdrop-blur-sm">
                    <div className="text-sm font-medium">{captureProgress.phase}</div>
                    <div className="mt-1 h-2 w-48 overflow-hidden rounded-full bg-surface">
                      <div className="h-full bg-primary transition-all"
                        style={{ width: `${(captureProgress.current / captureProgress.total) * 100}%` }} />
                    </div>
                    <div className="mt-1 text-xs text-foreground-tertiary">
                      {captureProgress.current} / {captureProgress.total}
                    </div>
                  </div>
                </div>
              )}

              {/* ── Image + overlays ────────────────────────────────────── */}
              {!imageSrc && (
                <div className="flex h-full items-center justify-center text-foreground-tertiary">
                  {t("sidebar.selectWindow")}
                </div>
              )}
              {imageSrc && imgSize.w > 0 && (
                <div className="will-change-transform"
                  style={{
                    transform: `translate(${pan.x}px, ${pan.y}px) scale(${scale})`,
                    transformOrigin: "0 0", display: "inline-block",
                  }}>
                  <div className="relative leading-0">
                    <img ref={imgRef} src={imageSrc} alt="capture"
                      width={imgSize.w} height={imgSize.h} draggable={false}
                      className="block max-w-none select-none" />
                    <svg className="pointer-events-none absolute left-0 top-0 overflow-visible"
                      width={imgSize.w} height={imgSize.h}>
                      {shapePreview()}
                      {polyPreview()}
                      {pickPointsOverlay()}
                    </svg>
                  </div>
                </div>
              )}
            </div>

            {/* ── Status bar ────────────────────────────────────────────── */}
            <div className="flex items-center justify-between border-t border-border px-2 py-1 text-xs text-foreground-secondary">
              <span>
                {pickPoints.length > 0
                  ? t("status.pickHint", pickPoints.length)
                  : (hoverLabel ?? t("status.inspectHint"))}
              </span>
              <span>{pickedLabel}</span>
            </div>
          </div>

          {/* ── Right sidebar: crop directory files ─────────────────────── */}
          <aside className="flex w-72 shrink-0 flex-col rounded-lg border border-border bg-card/80 text-sm">
            <div className="flex items-center justify-between border-b border-border px-3 py-2">
              <span className="text-xs text-foreground-tertiary">
                {cropDir ? t("sidebar.cropDir", cropFrames.length) : t("setting.cropDir")}
              </span>
              {cropDir && (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button variant="ghost" size="icon" className="size-6"
                      onClick={() => void clearAllFrames(cropDir)}>
                      <Trash2 className="size-3 text-destructive" />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Clear all</TooltipContent>
                </Tooltip>
              )}
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto p-2">
              {!cropDir ? (
                <div className="px-2 py-3 text-xs text-foreground-tertiary">
                  {t("sidebar.noCropDir")}
                </div>
              ) : cropFrames.length === 0 ? (
                <div className="px-2 py-3 text-xs text-foreground-tertiary">
                  {t("sidebar.noCropFiles")}
                </div>
              ) : (
                <div className="space-y-1">
                  {cropFrames.map((frame) => (
                    <ContextMenu key={frame.path}>
                      <ContextMenuTrigger asChild>
                        <button type="button"
                          className="w-full rounded-md border px-2 py-2 text-left text-xs border-border bg-surface text-foreground-secondary hover:bg-surface-hover"
                          onClick={() => { if (!frame.name.endsWith(".json")) void loadFrameFromPath(frame.path); }}>
                          <div className="flex items-center gap-1.5">
                            {frame.name.endsWith(".json") && <FileText className="size-3 shrink-0 text-foreground-tertiary" />}
                            <span className="truncate font-medium">{frame.name}</span>
                          </div>
                          <div className="mt-1 flex items-center justify-between text-[11px] text-foreground-tertiary">
                            <span>{formatTime(frame.modifiedMs)}</span>
                            <span>{formatSize(frame.sizeBytes)}</span>
                          </div>
                        </button>
                      </ContextMenuTrigger>
                      <ContextMenuContent>
                        <ContextMenuLabel>{t("dialog.fileOperations")}</ContextMenuLabel>
                        <ContextMenuSeparator />
                        <ContextMenuItem onClick={() => void writeText(frame.name)}>
                          <Copy className="size-4" /> {t("menu.copyFilename")}
                        </ContextMenuItem>
                        <ContextMenuItem onClick={() => void writeText(frame.path)}>
                          <Copy className="size-4" /> {t("menu.copyPath")}
                        </ContextMenuItem>
                        {frame.name.endsWith(".json") && (
                          <ContextMenuItem onClick={async () => {
                            try {
                              const text = await invoke<string>("wps_read_text_file", { path: frame.path });
                              const json = JSON.parse(text) as Record<string, unknown>;
                              const baseName = frame.name.replace(/\.[^.]+$/, "");
                              const snippet = generateJsSnippet(json, baseName);
                              await writeText(snippet);
                            } catch (e) { console.warn("Failed to copy JS snippet:", e); }
                          }}>
                            <Copy className="size-4" /> {t("menu.copyJsCode")}
                          </ContextMenuItem>
                        )}
                        <ContextMenuSeparator />
                        <ContextMenuItem onClick={() => void invoke("wps_reveal_in_explorer", { path: frame.path })}>
                          <FolderOpen className="size-4" /> {t("menu.revealInExplorer")}
                        </ContextMenuItem>
                        <ContextMenuSeparator />
                        <ContextMenuItem variant="destructive"
                          onClick={() => void deleteFrame(frame.path)}>
                          <Trash2 className="size-4" /> {t("menu.delete")}
                        </ContextMenuItem>
                      </ContextMenuContent>
                    </ContextMenu>
                  ))}
                </div>
              )}
            </div>
          </aside>
        </div>

        {/* ── Settings dialog ──────────────────────────────────────────── */}
        <Dialog open={settingsOpen} onOpenChange={setSettingsOpen}>
          <DialogContent className="max-w-2xl max-h-[80vh] overflow-y-auto">
            <DialogHeader>
              <DialogTitle>{t("dialog.settings")}</DialogTitle>
            </DialogHeader>
            <div className="space-y-6">
              {/* Language */}
              <div>
                <Label className="mb-1.5">{t("setting.language")}</Label>
                <Select value={lang} onValueChange={(v) => {
                  const newLang = v as Lang;
                  setLang(newLang);
                  onLangPersist(newLang);
                  setSettingsDraft((s) => ({ ...s, language: newLang }));
                }}>
                  <SelectTrigger className="w-40 h-9 text-sm"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="zh">中文</SelectItem>
                    <SelectItem value="en">English</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              {/* Window selection */}
              <div>
                <Label className="mb-1.5">{t("setting.window")}</Label>
                <div className="flex gap-2">
                  <Input type="text" placeholder={t("placeholder.filterWindows")} value={windowFilter}
                    onChange={(e) => setWindowFilter(e.target.value)} className="flex-1 h-9 text-sm" />
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button variant="outline" size="icon" className="size-9"
                        onClick={() => void refreshWindows()}>
                        <RefreshCw className="size-4" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>{t("tooltip.refreshWindows")}</TooltipContent>
                  </Tooltip>
                </div>
                <div className="mt-2">
                  <Select value={selectedHwnd != null ? String(selectedHwnd) : ""}
                    onValueChange={(value) => {
                      const hwnd = value ? Number(value) : null;
                      setSelectedHwnd(hwnd);
                      if (hwnd !== null) void persistHwnd(hwnd);
                    }}>
                    <SelectTrigger className="w-full h-9 text-sm">
                      <SelectValue placeholder={t("placeholder.selectWindow")} />
                    </SelectTrigger>
                    <SelectContent>
                      {filteredWindows.map((w) => (
                        <SelectItem key={w.hwnd} value={String(w.hwnd)}>{w.title}</SelectItem>
                      ))}
                      {filteredWindows.length === 0 && (
                        <div className="px-2 py-1.5 text-xs text-foreground-tertiary">{t("misc.noMatchingWindows")}</div>
                      )}
                    </SelectContent>
                  </Select>
                </div>
              </div>

              {/* Directories */}
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label className="mb-1.5">{t("setting.screenshotDir")}</Label>
                  <div className="flex gap-2">
                    <Input className="flex-1" value={settingsDraft.saveDir ?? ""} placeholder={t("placeholder.notSet")}
                      onChange={(e) => setSettingsDraft((s) => ({ ...s, saveDir: e.target.value.trim() || null }))} />
                    <Button variant="outline" onClick={async () => {
                      const dir = await open({ title: t("setting.screenshotDir"), directory: true });
                      if (typeof dir === "string") setSettingsDraft((s) => ({ ...s, saveDir: dir }));
                    }}>{t("btn.browse")}</Button>
                  </div>
                </div>
                <div>
                  <Label className="mb-1.5">{t("setting.cropDir")}</Label>
                  <div className="flex gap-2">
                    <Input className="flex-1" value={settingsDraft.cropDir ?? ""} placeholder={t("placeholder.notSet")}
                      onChange={(e) => setSettingsDraft((s) => ({ ...s, cropDir: e.target.value.trim() || null }))} />
                    <Button variant="outline" onClick={async () => {
                      const dir = await open({ title: t("setting.cropDir"), directory: true });
                      if (typeof dir === "string") setSettingsDraft((s) => ({ ...s, cropDir: dir }));
                    }}>{t("btn.browse")}</Button>
                  </div>
                </div>
              </div>

              {/* Capture settings */}
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <Label className="mb-1.5">{t("setting.captureInterval")}</Label>
                  <Input type="number" min={50} step={50} value={settingsDraft.intervalMs}
                    onChange={(e) => setSettingsDraft((s) => ({ ...s, intervalMs: Number(e.target.value) || 500 }))} />
                </div>
                <div>
                  <Label className="mb-1.5">{t("setting.roundedRadius")}</Label>
                  <input type="range" min={0} max={128} value={settingsDraft.roundedRx}
                    onChange={(e) => setSettingsDraft((s) => ({ ...s, roundedRx: Number(e.target.value) }))}
                    className="w-full accent-primary" />
                  <div className="text-xs text-muted-foreground">{t("misc.unitPx", settingsDraft.roundedRx)}</div>
                </div>
              </div>

              {/* Scroll capture settings */}
              <div className="rounded-lg border border-border p-4">
                <Label className="mb-3 block text-sm font-medium">{t("setting.scrollCapture")}</Label>
                <div className="grid grid-cols-4 gap-3">
                  <div>
                    <Label className="mb-1 text-xs">{t("setting.direction")}</Label>
                    <Select value={settingsDraft.scrollDirection ?? "down"}
                      onValueChange={(v) => setSettingsDraft((s) => ({ ...s, scrollDirection: v }))}>
                      <SelectTrigger className="h-8 text-xs"><SelectValue /></SelectTrigger>
                      <SelectContent>
                        <SelectItem value="down">{t("dir.down")}</SelectItem>
                        <SelectItem value="up">{t("dir.up")}</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                  <div>
                    <Label className="mb-1 text-xs">{t("setting.scrollAmount")}</Label>
                    <Input type="number" min={1} className="h-8 text-xs" value={settingsDraft.scrollAmount ?? 120}
                      onChange={(e) => setSettingsDraft((s) => ({ ...s, scrollAmount: Number(e.target.value) || 120 }))} />
                  </div>
                  <div>
                    <Label className="mb-1 text-xs">{t("setting.frames")}</Label>
                    <Input type="number" min={2} max={100} className="h-8 text-xs" value={settingsDraft.scrollFrames ?? 5}
                      onChange={(e) => setSettingsDraft((s) => ({ ...s, scrollFrames: Number(e.target.value) || 5 }))} />
                  </div>
                  <div>
                    <Label className="mb-1 text-xs">{t("setting.delay")}</Label>
                    <Input type="number" min={50} className="h-8 text-xs" value={settingsDraft.scrollDelayMs ?? 500}
                      onChange={(e) => setSettingsDraft((s) => ({ ...s, scrollDelayMs: Number(e.target.value) || 500 }))} />
                  </div>
                </div>
              </div>

              {/* Panoramic capture settings */}
              <div className="rounded-lg border border-border p-4">
                <Label className="mb-3 block text-sm font-medium">{t("setting.panoCapture")}</Label>
                <div className="grid grid-cols-4 gap-3">
                  <div>
                    <Label className="mb-1 text-xs">{t("setting.direction")}</Label>
                    <Select value={settingsDraft.panoDirection ?? "right"}
                      onValueChange={(v) => setSettingsDraft((s) => ({ ...s, panoDirection: v }))}>
                      <SelectTrigger className="h-8 text-xs"><SelectValue /></SelectTrigger>
                      <SelectContent>
                        <SelectItem value="right">{t("dir.right")}</SelectItem>
                        <SelectItem value="left">{t("dir.left")}</SelectItem>
                        <SelectItem value="down">{t("dir.down")}</SelectItem>
                        <SelectItem value="up">{t("dir.up")}</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                  <div>
                    <Label className="mb-1 text-xs">{t("setting.dragDistance")}</Label>
                    <Input type="number" min={10} className="h-8 text-xs" value={settingsDraft.panoDragDistance ?? 500}
                      onChange={(e) => setSettingsDraft((s) => ({ ...s, panoDragDistance: Number(e.target.value) || 500 }))} />
                  </div>
                  <div>
                    <Label className="mb-1 text-xs">{t("setting.frames")}</Label>
                    <Input type="number" min={2} max={50} className="h-8 text-xs" value={settingsDraft.panoFrames ?? 5}
                      onChange={(e) => setSettingsDraft((s) => ({ ...s, panoFrames: Number(e.target.value) || 5 }))} />
                  </div>
                  <div>
                    <Label className="mb-1 text-xs">{t("setting.delay")}</Label>
                    <Input type="number" min={50} className="h-8 text-xs" value={settingsDraft.panoDelayMs ?? 300}
                      onChange={(e) => setSettingsDraft((s) => ({ ...s, panoDelayMs: Number(e.target.value) || 300 }))} />
                  </div>
                </div>
              </div>

              {/* Keyboard shortcuts */}
              <div className="rounded-lg border border-border p-4">
                <div className="mb-3 flex items-center justify-between">
                  <Label className="text-sm font-medium">{t("setting.shortcuts")}</Label>
                  <Button variant="ghost" size="sm" className="h-7 text-xs"
                    onClick={() => {
                      setCustomKeybindings(null);
                      const draft = { ...settingsDraft, keybindings: null };
                      void persistSettings(draft);
                    }}>
                    {t("btn.resetDefaults")}
                  </Button>
                </div>
                <div className="grid grid-cols-2 gap-x-4 gap-y-1.5">
                  {(Object.keys(DEFAULT_KEYBINDINGS) as KeyAction[]).map((action) => {
                    const currentKey = (customKeybindings ?? {})[action] ?? DEFAULT_KEYBINDINGS[action];
                    const isListening = listeningAction === action;
                    const labelKey = getShortcutLabelKey(action);
                    return (
                      <div key={action} className="flex items-center justify-between gap-2">
                        <span className="text-xs text-foreground-secondary">
                          {labelKey ? t(labelKey) : action}
                        </span>
                        <button
                          className={cn(
                            "min-w-16 rounded-md border px-2 py-0.5 text-center text-xs font-mono",
                            isListening
                              ? "border-primary bg-primary/10 text-primary animate-pulse"
                              : "border-border bg-surface text-foreground hover:bg-surface-hover",
                          )}
                          onClick={() => setListeningAction(isListening ? null : action)}
                        >
                          {isListening ? t("btn.pressKey") : formatKey(currentKey)}
                        </button>
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>
            <DialogFooter>
              <Button variant="outline" onClick={() => setSettingsOpen(false)}>{t("btn.cancel")}</Button>
              <Button onClick={async () => {
                const saved = await persistSettings(settingsDraft);
                if (saved) { setSettingsOpen(false); setError(null); }
              }}>{t("btn.save")}</Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>

        {/* ── Crop save dialog ─────────────────────────────────────────── */}
        <Dialog open={cropDialogOpen} onOpenChange={setCropDialogOpen}>
          <DialogContent className="max-w-md">
            <DialogHeader>
              <DialogTitle>{t("dialog.saveSelection")}</DialogTitle>
            </DialogHeader>
            <div className="space-y-4">
              <div>
                <Label className="mb-1.5">{t("dialog.filename")}</Label>
                <div className="flex gap-2">
                  <Input className="flex-1" placeholder={t("placeholder.enterFilename")} value={cropFilename}
                    onChange={(e) => setCropFilename(e.target.value)} autoFocus />
                  <span className="flex items-center text-sm text-muted-foreground">.png</span>
                </div>
                <div className="mt-1.5 text-xs text-muted-foreground">
                  {t("dialog.savingTo", cropDir ?? saveDir ?? t("dialog.noDirSet"))}
                </div>
              </div>
            </div>
            <DialogFooter>
              <Button variant="outline" onClick={() => { setCropDialogOpen(false); setDragRect(null); dragRectRef.current = null; setPolyPts([]); }}>{t("btn.cancel")}</Button>
              <Button onClick={() => void saveCrop()} disabled={!(cropDir ?? saveDir) || !cropFilename.trim()}>{t("btn.save")}</Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>

        {/* ── Pick points save dialog ──────────────────────────────────── */}
        <Dialog open={pickSaveOpen} onOpenChange={setPickSaveOpen}>
          <DialogContent className="max-w-md">
            <DialogHeader>
              <DialogTitle>{t("dialog.savePickPoints")}</DialogTitle>
            </DialogHeader>
            <div className="space-y-4">
              <div>
                <Label className="mb-1.5">{t("dialog.filename")}</Label>
                <div className="flex gap-2">
                  <Input className="flex-1" placeholder={t("placeholder.enterFilename")} value={pickSaveFilename}
                    onChange={(e) => setPickSaveFilename(e.target.value)} autoFocus />
                  <span className="flex items-center text-sm text-muted-foreground">.json</span>
                </div>
                <div className="mt-1.5 text-xs text-muted-foreground">
                  {t("dialog.savingPointsTo", pickPoints.length, cropDir ?? saveDir ?? t("dialog.noDirSet"))}
                </div>
                <div className="mt-2 max-h-32 overflow-y-auto rounded-md border border-border bg-surface p-2 text-xs">
                  {pickPoints.map((p, i) => (
                    <div key={i} className="flex items-center gap-2 py-0.5">
                      <span className="size-3 rounded-full border border-border" style={{ backgroundColor: p.hex }} />
                      <span>#{i + 1}</span>
                      <span className="text-foreground-tertiary">({p.x}, {p.y})</span>
                      <span className="font-mono">{p.hex}</span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
            <DialogFooter>
              <Button variant="outline" onClick={() => setPickSaveOpen(false)}>{t("btn.cancel")}</Button>
              <Button onClick={() => void savePickPoints()} disabled={!(cropDir ?? saveDir) || !pickSaveFilename.trim()}>{t("btn.saveJson")}</Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>

        {/* ── Error banner ─────────────────────────────────────────────── */}
        {error && (
          <div className="rounded-md border border-destructive/35 bg-destructive/10 px-3 py-2 text-sm text-foreground">
            {error}
          </div>
        )}
      </div>
    </TooltipProvider>
  );
}
