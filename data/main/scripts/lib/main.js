/**
 * Helpers for scripts.
 */
function roiObj(x, y, w, h) {
  return { x, y, width: w, height: h };
}

registerLibrary("roi", function roi(args) {
  return roiObj(
    args.x | 0,
    args.y | 0,
    args.width | 0,
    args.height | 0
  );
});

registerLibrary("findFirst", async function findFirst(args) {
  const names = args.names || [];
  const roi = args.roi;
  const threshold = args.threshold != null ? args.threshold : 0.8;
  for (let i = 0; i < names.length; i++) {
    const name = names[i];
    const opts = roi ? { roi, threshold, grayscale: true } : { threshold, grayscale: true };
    const m = await ctx.findTemplate(name, opts);
    if (m) {
      return { name, match: m };
    }
  }
  return null;
});

registerLibrary("clickMatchCenter", async function clickMatchCenter(args) {
  const m = args.match;
  if (!m) {
    return { ok: false };
  }
  const w = m.width != null ? m.width : m.w;
  const h = m.height != null ? m.height : m.h;
  const x = Math.round(m.x + (w || 0) / 2);
  const y = Math.round(m.y + (h || 0) / 2);
  await ctx.click(x, y);
  return { ok: true, x, y };
});

registerLibrary("longPressCenter", async function longPressCenter(args) {
  const m = args.match;
  if (!m) {
    return { ok: false };
  }
  const w = m.width != null ? m.width : m.w;
  const h = m.height != null ? m.height : m.h;
  const x = Math.round(m.x + (w || 0) / 2);
  const y = Math.round(m.y + (h || 0) / 2);
  const repeats = args.repeats != null ? args.repeats : 3;
  const gapMs = args.gapMs != null ? args.gapMs : 450;
  const pre = args.preDelayMs != null ? args.preDelayMs : 0;
  const post = args.postDelayMs != null ? args.postDelayMs : 0;
  await ctx.sleep(pre);
  for (let i = 0; i < repeats; i++) {
    await ctx.click(x, y);
    await ctx.sleep(gapMs);
  }
  await ctx.sleep(post);
  return { ok: true, x, y };
});


registerLibrary("isMainScreen", async function isMainScreen(args) {
  // Find template in region
  // const match = await ctx.findTemplate("主界面", { roi: { x: 0, y: 1080 - 300, width: 300, height: 300 }, threshold: 0.95, greenMask: true });
  // if (!match) {
  //   return false;
  // }
  const ocrRes = await ctx.ocr(31, 940, 200, 200);
  if (ocrRes && ocrRes.trim().toLowerCase().indexOf('enter') !== -1) {
    return true;
  }
  return false;
});