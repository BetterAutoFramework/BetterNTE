/**
 * anti_detect — 人类行为模拟库
 * 包装 ctx.click/sleep/keyPress 等操作，添加随机延迟和偏移
 */

function jitter(base, pct) {
  return base * (1 + (Math.random() * 2 - 1) * pct);
}

function randInt(min, max) {
  return Math.floor(Math.random() * (max - min + 1)) + min;
}

function bezierPoints(p0, p1, p2, p3, steps) {
  const points = [];
  for (let i = 1; i <= steps; i++) {
    const t = i / steps;
    const u = 1 - t;
    const x = u * u * u * p0.x + 3 * u * u * t * p1.x + 3 * u * t * t * p2.x + t * t * t * p3.x;
    const y = u * u * u * p0.y + 3 * u * u * t * p1.y + 3 * u * t * t * p2.y + t * t * t * p3.y;
    points.push({ x: Math.round(x), y: Math.round(y) });
  }
  return points;
}

// ── click: 随机偏移 ±3px + 前置随机延迟 20-50ms ──
registerLibrary("click", async function (args) {
  const ox = randInt(-3, 3);
  const oy = randInt(-3, 3);
  const delay = randInt(20, 50);
  await ctx.sleep(delay);
  await ctx.click(args.x + ox, args.y + oy);
});

// ── sleep: 原 ms 基础上 ±5% 随机抖动 ──
registerLibrary("sleep", async function (args) {
  const ms = Math.round(jitter(args.ms, 0.05));
  await ctx.sleep(ms);
});

// ── keyPress: 按键时长 ±15% 抖动 ──
registerLibrary("keyPress", async function (args) {
  if (args.duration) {
    const dur = Math.round(jitter(args.duration, 0.15));
    await ctx.keyPress(args.key, dur);
  } else {
    await ctx.keyPress(args.key);
  }
});

// ── mouseMove: 贝塞尔曲线移动 ──
registerLibrary("mouseMove", async function (args) {
  const frame = await ctx.capture();
  const cx = frame.width / 2;
  const cy = frame.height / 2;
  const p0 = { x: cx, y: cy };
  const p3 = { x: args.x, y: args.y };
  const dx = p3.x - p0.x;
  const dy = p3.y - p0.y;
  const p1 = { x: p0.x + dx * 0.3 + randInt(-50, 50), y: p0.y + dy * 0.3 + randInt(-50, 50) };
  const p2 = { x: p0.x + dx * 0.7 + randInt(-30, 30), y: p0.y + dy * 0.7 + randInt(-30, 30) };
  const points = bezierPoints(p0, p1, p2, p3, 5);
  for (const pt of points) {
    await ctx.mouseMove(pt.x, pt.y);
    await ctx.sleep(randInt(8, 25));
  }
});

// ── swipe: 滑动加随机路径偏移 ──
registerLibrary("swipe", async function (args) {
  const ox1 = randInt(-3, 3);
  const oy1 = randInt(-3, 3);
  const ox2 = randInt(-5, 5);
  const oy2 = randInt(-5, 5);
  await ctx.swipe(
    args.x1 + ox1, args.y1 + oy1,
    args.x2 + ox2, args.y2 + oy2,
    args.duration
  );
});

// ── randomDelay: 等待 min~max 随机时间 ──
registerLibrary("randomDelay", async function (args) {
  const ms = randInt(args.min, args.max);
  await ctx.sleep(ms);
});
