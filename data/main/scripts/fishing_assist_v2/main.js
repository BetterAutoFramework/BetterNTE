/**
 * 内部调参（不暴露给前端）。需要微调时直接改这里的默认值。
 */
const FISHING_ASSIST_TUNING = {
  controlMode: "pulse",
  deadzone: 10,
  deadzoneHold: 8,
  maxTry: 10,
  factor: 1.2,
  capMs: 380,
  floorMs: 40,
  cursorPxPerSec: 260,
  trackTapCapMs: 95,
  trackTapScale: 0.42,
  trackTapFloorMs: 22,
  barThreshold: 0.88,
  incompleteDebounceFrames: 4,
  controlPollMs: 10,
  longPulseTailMs: 12,
  longPulseThresholdMs: 200,
  roiTrackEnabled: true,
  roiTrackPaddingX: 150,
  roiTrackPaddingY: 26,
  roiTrackPlayerExtraX: 80,
  roiResyncEvery: 10,
  templateLeftName: "左侧",
  templateRightName: "右侧",
  templatePlayerName: "玩家",
  pulseTapMaxMs: 72,
  pulseTapAccelMaxMs: 192,
  pulseTapMinMs: 16,
  pressDurationScale: 1.0,
  pressDurationScaleMedium: 1.4,
  pressDurationScaleLarge: 2.0,
  predictiveEnabled: true,
  predictiveGain: 0.55,
  predictiveMaxPx: 260,
  pulseAccelStep: 0.15,
  pulseAccelCap: 1.8,
  pulseAccelDecay: 0.22,
  pulseAccelTrendPx: 8,
  pulseFastRestMs: 4,
  templateBarMinConfidence: 0.94,
  templatePlayerMinConfidence: 0.98,
  templateBarMinWidthPx: 60,
  templateBarMaxWidthPx: 230,
  templateYMaxDeltaPx: 20,
  templatePlayerOutsideMarginPx: 280,
  debugTrace: true,
  debugTraceInterval: 5,
  fishEscapeRoi: { x: 849, y: 496, width: 302, height: 184 },
  fishEscapeTemplateThreshold: 0.95,
  offsetSanityProbePx: 280,
  sanityProbeEvery: 6,
  endProbeEveryLoops: 8,
  frozenTripletLoops: 8,
  staleOffsetLoops: 6,
  ocrProbeEveryLoops: 3,
  startUseOcr: false,
  startFPressCooldownMs: 700,
  startAfterFWaitMs: 260,
  startMinigameProbeEveryLoops: 2,
  baitBuyCount: 3,
};

/** 灵敏度预设：conservative=保守稳定, balanced=均衡, aggressive=激进快速 */
const SENSITIVITY_PRESETS = {
  conservative: {
    factor: 0.9, capMs: 300, floorMs: 50, cursorPxPerSec: 220,
    pulseTapMaxMs: 60, pulseTapAccelMaxMs: 150, predictiveGain: 0.4,
    pulseAccelStep: 0.10, pulseAccelCap: 1.4, pressDurationScaleMedium: 1.2,
    pressDurationScaleLarge: 1.6,
  },
  balanced: {},
  aggressive: {
    factor: 1.5, capMs: 450, floorMs: 30, cursorPxPerSec: 320,
    pulseTapMaxMs: 90, pulseTapAccelMaxMs: 220, predictiveGain: 0.7,
    pulseAccelStep: 0.20, pulseAccelCap: 2.2, pressDurationScaleMedium: 1.6,
    pressDurationScaleLarge: 2.4,
  },
};

/**
 * 合并前端参数与内部默认值。前端 params 优先，未传则用 TUNING 默认。
 */
function taskParams() {
  const cfg = typeof globalThis.config === "object" && globalThis.config !== null
    ? globalThis.config
    : {};
  const preset = SENSITIVITY_PRESETS[cfg.sensitivity] || {};
  return { ...FISHING_ASSIST_TUNING, ...preset, ...cfg };
}

/** `pulse` | `track` | `hold`; empty defaults to pulse. */
function normalizeControlMode(raw) {
  if (raw == null || raw === "") return "pulse";
  const s = String(raw).trim().toLowerCase();
  if (s === "track" || s === "tap" || s === "soft") return "track";
  if (s === "hold" || s === "keydown") return "hold";
  if (s === "pulse" || s === "ma" || s === "maa") return "pulse";
  return "pulse";
}

async function playGame() {
  const c = taskParams();
  const controlMode = normalizeControlMode(c.control_mode || c.controlMode);
  const deadzone =
    controlMode === "hold" ? c.deadzoneHold : c.deadzone;
  const maxTryItem = c.max_retry ?? c.maxTry;
  const factor = c.factor;
  const capMs = c.capMs;
  const floorMs = c.floorMs;
  const cursorPxPerSec = c.cursorPxPerSec;
  /** track: max ms per loop tap (small because next observation is ~≥300ms away). */
  const trackTapCapMs = c.trackTapCapMs;
  const trackTapScale = c.trackTapScale;
  const trackTapFloorMs = c.trackTapFloorMs;
  const barThreshold = c.barThreshold;
  const incompleteDebounceFrames = c.incompleteDebounceFrames;
  const controlPollMs = c.controlPollMs;
  const longPulseTailMs = c.longPulseTailMs;
  const longPulseThresholdMs = c.longPulseThresholdMs;
  const roiTrackEnabled = c.roiTrackEnabled !== false;
  const roiTrackPaddingX = c.roiTrackPaddingX;
  const roiTrackPaddingY = c.roiTrackPaddingY;
  const roiTrackPlayerExtraX = c.roiTrackPlayerExtraX;
  const roiResyncEvery = Math.max(1, c.roiResyncEvery);

  const visionMode = "template";
  const templateLeftName = c.templateLeftName || "左侧";
  const templateRightName = c.templateRightName || "右侧";
  const templatePlayerName = c.templatePlayerName || "玩家";
  const pulseTapMaxMs = Math.max(
    20,
    Math.min(200, c.pulseTapMaxMs != null ? Number(c.pulseTapMaxMs) : 80)
  );
  const pulseTapAccelMaxMs = Math.max(
    pulseTapMaxMs,
    c.pulseTapAccelMaxMs != null ? Number(c.pulseTapAccelMaxMs) : 96
  );
  const pulseTapMinMs = Math.max(
    1,
    c.pulseTapMinMs != null ? Number(c.pulseTapMinMs) : 16
  );
  const pressDurationScale =
    c.pressDurationScale != null ? Number(c.pressDurationScale) : 1.0;
  const pressDurationScaleMedium =
    c.pressDurationScaleMedium != null
      ? Number(c.pressDurationScaleMedium)
      : 1.4;
  const pressDurationScaleLarge =
    c.pressDurationScaleLarge != null
      ? Number(c.pressDurationScaleLarge)
      : 2.0;
  const predictiveEnabled = c.predictiveEnabled !== false;
  const predictiveGain =
    c.predictiveGain != null ? Number(c.predictiveGain) : 0.55;
  const predictiveMaxPx = Math.max(
    40,
    c.predictiveMaxPx != null ? Number(c.predictiveMaxPx) : 260
  );
  const pulseAccelStep =
    c.pulseAccelStep != null ? Number(c.pulseAccelStep) : 0.15;
  const pulseAccelCap =
    c.pulseAccelCap != null ? Number(c.pulseAccelCap) : 1.8;
  const pulseAccelDecay =
    c.pulseAccelDecay != null ? Number(c.pulseAccelDecay) : 0.22;
  const pulseAccelTrendPx =
    c.pulseAccelTrendPx != null ? Number(c.pulseAccelTrendPx) : 8;
  const pulseFastRestMs = Math.max(
    0,
    c.pulseFastRestMs != null ? Number(c.pulseFastRestMs) : 8
  );
  const templateBarMinConfidence =
    c.templateBarMinConfidence != null
      ? Number(c.templateBarMinConfidence)
      : 0.94;
  const templatePlayerMinConfidence =
    c.templatePlayerMinConfidence != null
      ? Number(c.templatePlayerMinConfidence)
      : 0.98;
  const templateBarMinWidthPx = Math.max(
    1,
    c.templateBarMinWidthPx != null ? Number(c.templateBarMinWidthPx) : 60
  );
  const templateBarMaxWidthPx = Math.max(
    templateBarMinWidthPx,
    c.templateBarMaxWidthPx != null ? Number(c.templateBarMaxWidthPx) : 230
  );
  const templateYMaxDeltaPx = Math.max(
    0,
    c.templateYMaxDeltaPx != null ? Number(c.templateYMaxDeltaPx) : 20
  );
  const templatePlayerOutsideMarginPx = Math.max(
    0,
    c.templatePlayerOutsideMarginPx != null
      ? Number(c.templatePlayerOutsideMarginPx)
      : 280
  );

  const debugTrace = c.debugTrace === true;
  const debugTraceInterval = Math.max(1, Number(c.debugTraceInterval));
  const fishEscapeRoi = c.fishEscapeRoi || {
    x: 849,
    y: 496,
    width: 302,
    height: 184,
  };
  const fishEscapeThr =
    c.fishEscapeTemplateThreshold != null
      ? Number(c.fishEscapeTemplateThreshold)
      : 0.95;
  const offsetSanityProbePx =
    c.offsetSanityProbePx != null ? Number(c.offsetSanityProbePx) : 280;
  const sanityProbeEvery = Math.max(
    1,
    Number(c.sanityProbeEvery != null ? c.sanityProbeEvery : 6)
  );
  const endProbeEveryLoops = Math.max(
    1,
    Number(c.endProbeEveryLoops != null ? c.endProbeEveryLoops : 8)
  );
  const frozenTripletLoops = Math.max(
    4,
    Number(c.frozenTripletLoops != null ? c.frozenTripletLoops : 8)
  );
  const staleOffsetLoops = Math.max(
    3,
    Number(c.staleOffsetLoops != null ? c.staleOffsetLoops : 6)
  );

  function fmtMatch(m) {
    if (!m) return "null";
    const c =
      m.confidence != null && typeof m.confidence === "number"
        ? m.confidence.toFixed(3)
        : "?";
    return `x=${m.x} y=${m.y} w=${m.width} h=${m.height} conf=${c}`;
  }

  async function releaseKeys() {
    await ctx.keyUp("a");
    await ctx.keyUp("d");
  }

  let keyPressInFlightA = false;
  let keyPressInFlightD = false;
  function fireKeyPress(key, durationMs) {
    if (key === "a" && keyPressInFlightA) return false;
    if (key === "d" && keyPressInFlightD) return false;
    if (key === "a") keyPressInFlightA = true;
    if (key === "d") keyPressInFlightD = true;
    Promise.resolve(ctx.keyPress(key, durationMs))
      .catch((e) => {
        ctx.logWarn(`控条异步按键失败: ${String(e)}`);
      })
      .finally(() => {
        if (key === "a") keyPressInFlightA = false;
        if (key === "d") keyPressInFlightD = false;
      });
    return true;
  }

  function scaledDurationMs(baseMs, absOffset, floorMs) {
    const scale =
      absOffset >= 120
        ? pressDurationScaleLarge
        : absOffset >= 60
          ? pressDurationScaleMedium
          : pressDurationScale;
    return Math.max(floorMs, Math.floor(baseMs * scale));
  }

  await releaseKeys();

  if (controlMode === "pulse") {
    ctx.logInfo(
      `控条：快速点按（单次≤${pulseTapMaxMs}ms）。`
    );
  } else if (controlMode === "hold") {
    ctx.logInfo(
      `控条：长按 A/D（识别间隙内保持按下，跟手更好；易甩过头可调 deadzoneHold）。`
    );
  } else if (controlMode === "track") {
    ctx.logInfo(
      `控条：跟踪模式（每轮短点 A/D，单次≤${pulseTapMaxMs}ms & cap≈${trackTapCapMs}ms）。`
    );
  }
  ctx.logInfo(
    `识别：V2纯模板模式（${templateLeftName}/${templateRightName}/${templatePlayerName}，仅透明像素过滤）`
  );
  if (roiTrackEnabled) {
    ctx.logInfo(
      `模板ROI跟踪: 开启（paddingX=${roiTrackPaddingX}, paddingY=${roiTrackPaddingY}, ${roiResyncEvery}轮全局重扫）`
    );
  } else {
    ctx.logInfo("模板ROI跟踪: 关闭（每轮全局ROI）");
  }
  if (debugTrace) {
    ctx.logInfo(
      `[排查] debugTrace=每${debugTraceInterval}轮一条控条摘要`
    );
  }

  let maxTryLeft = maxTryItem;
  let incompleteStreak = 0;
  let controlLoop = 0;
  /** hold only: -1 = D held, 0 = none, 1 = A held */
  let heldDir = 0;
  let pulseDir = 0;
  let pulseBoost = 1.0;
  let prevAbsOffset = 0;
  let invalidTripletStreak = 0;
  let freezeStreak = 0;
  let lastTripletSig = "";
  let staleOffsetStreak = 0;
  let lastOffset = null;
  let prevPredictOffset = null;
  let leftLock = null;
  let rightLock = null;
  let playerLock = null;

  const roiBar = { x: 577, y: 55, width: 784, height: 100 };
  const roiLevel = { x: 688, y: 98, width: 212, height: 150 };
  const barBaseOpts = {
    roi: roiBar,
    useAlphaMask: true,
    threshold: barThreshold,
    grayscale: true,
  };
  const clampRoi = (roi) => {
    const x = Math.max(roiBar.x, roi.x);
    const y = Math.max(roiBar.y, roi.y);
    const right = Math.min(roiBar.x + roiBar.width, roi.x + roi.width);
    const bottom = Math.min(roiBar.y + roiBar.height, roi.y + roi.height);
    if (right <= x || bottom <= y) return null;
    return { x, y, width: right - x, height: bottom - y };
  };
  const trackedRoi = (m, extraX = 0) => {
    if (!m || !roiTrackEnabled) return null;
    return clampRoi({
      x: Math.floor(m.x - roiTrackPaddingX - extraX),
      y: Math.floor(m.y - roiTrackPaddingY),
      width: Math.floor(m.width + (roiTrackPaddingX + extraX) * 2),
      height: Math.floor(m.height + roiTrackPaddingY * 2),
    });
  };
  const entryOpts = (lock, extraX = 0) => ({
    ...barBaseOpts,
    roi: trackedRoi(lock, extraX) || roiBar,
  });
  const confOf = (m) =>
    m && typeof m.confidence === "number" ? m.confidence : 0;
  function validateTemplateTriplet(left, right, player) {
    if (!left || !right || !player) {
      return { ok: false, reason: "missing_part" };
    }
    const lc = confOf(left);
    const rc = confOf(right);
    const pc = confOf(player);
    if (lc < templateBarMinConfidence || rc < templateBarMinConfidence) {
      return { ok: false, reason: "bar_conf_low" };
    }
    if (pc < templatePlayerMinConfidence) {
      return { ok: false, reason: "player_conf_low" };
    }
    const leftX = left.x;
    const rightX = right.x;
    const barWidth = rightX - leftX;
    if (barWidth < templateBarMinWidthPx || barWidth > templateBarMaxWidthPx) {
      return { ok: false, reason: "bar_width_outlier" };
    }
    if (leftX >= rightX) {
      return { ok: false, reason: "bar_inverted" };
    }
    const barY = (left.y + right.y) / 2;
    if (
      Math.abs(left.y - right.y) > templateYMaxDeltaPx ||
      Math.abs(player.y - barY) > templateYMaxDeltaPx
    ) {
      return { ok: false, reason: "y_drift" };
    }
    const playerCenter = player.x + player.width / 2;
    if (
      playerCenter < leftX - templatePlayerOutsideMarginPx ||
      playerCenter > rightX + templatePlayerOutsideMarginPx
    ) {
      return { ok: false, reason: "player_outside" };
    }
    return { ok: true, reason: "ok" };
  }

  /** 「鱼儿溜走了」或垂钓等级；复用于双缺失、不完整 debounce、大偏移兜底。 */
  async function tryEndMinigameUi() {
    const [escapedTpl, levelTpl] = await ctx.findTemplateBatch([
      {
        name: "鱼儿溜走了",
        roi: fishEscapeRoi,
        threshold: fishEscapeThr,
        grayscale: true,
      },
      { name: "垂钓等级", ...barBaseOpts, roi: roiLevel },
    ]);
    if (escapedTpl) {
      ctx.logWarn("钓鱼失败：鱼儿溜走了（模板判定）");
      heldDir = 0;
      await releaseKeys();
      return { ok: true, reason: "fish_escaped" };
    }
    if (levelTpl) {
      ctx.logInfo(`垂钓等级 x=${levelTpl.x}`);
      heldDir = 0;
      await releaseKeys();
      return { ok: true, reason: "level_ui" };
    }
    return null;
  }

  while (true) {
    controlLoop += 1;

    let left = null;
    let right = null;
    let player = null;
    const globalResync =
      !roiTrackEnabled ||
      controlLoop % roiResyncEvery === 0 ||
      !leftLock ||
      !rightLock ||
      !playerLock;
    const leftOpts = globalResync ? barBaseOpts : entryOpts(leftLock, 0);
    const rightOpts = globalResync ? barBaseOpts : entryOpts(rightLock, 0);
    const playerOpts = globalResync
      ? barBaseOpts
      : entryOpts(playerLock, roiTrackPlayerExtraX);
    const tripleEntries = [
      { name: templateLeftName, ...leftOpts },
      { name: templateRightName, ...rightOpts },
      { name: templatePlayerName, ...playerOpts },
    ];
    const arr = await ctx.findTemplateBatch(tripleEntries);
    left = arr[0];
    right = arr[1];
    player = arr[2];
    const validTriplet = validateTemplateTriplet(left, right, player);
    if (!validTriplet.ok) {
      invalidTripletStreak += 1;
      if (debugTrace && controlLoop % debugTraceInterval === 0) {
        ctx.logWarn(
          `[控条#${controlLoop}] 模板命中被过滤 reason=${validTriplet.reason} | L=${fmtMatch(
            left
          )} R=${fmtMatch(right)} P=${fmtMatch(player)}`
        );
      }
      // Keep control continuity: use previous stable locks first.
      if (leftLock && rightLock && playerLock) {
        left = leftLock;
        right = rightLock;
        player = playerLock;
      } else {
        left = null;
        right = null;
        player = null;
      }
      // Only clear locks after consecutive invalid frames.
      if (invalidTripletStreak >= 3) {
        leftLock = null;
        rightLock = null;
        playerLock = null;
        pulseDir = 0;
        pulseBoost = 1.0;
        prevAbsOffset = 0;
      }
    } else {
      invalidTripletStreak = 0;
    }

    if (debugTrace && controlLoop % debugTraceInterval === 0) {
      ctx.logInfo(
        `[控条#${controlLoop}] template=only | 模板 L=${fmtMatch(
          left
        )} R=${fmtMatch(right)} P=${fmtMatch(player)}`
      );
    }

    if (left) leftLock = left;
    if (right) rightLock = right;
    if (player) playerLock = player;

    // Detect frozen control triplet; stale lock can hide end UI transition.
    if (left && right && player) {
      const sig = `${Math.round(left.x)}:${Math.round(left.y)}|${Math.round(
        right.x
      )}:${Math.round(right.y)}|${Math.round(player.x)}:${Math.round(player.y)}`;
      if (sig === lastTripletSig) {
        freezeStreak += 1;
      } else {
        freezeStreak = 0;
        lastTripletSig = sig;
      }
      if (freezeStreak >= frozenTripletLoops) {
        const finFreeze = await tryEndMinigameUi();
        if (finFreeze) return finFreeze;
        // Force unlock and reacquire after long freeze.
        leftLock = null;
        rightLock = null;
        playerLock = null;
        pulseDir = 0;
        pulseBoost = 1.0;
        prevAbsOffset = 0;
        freezeStreak = 0;
      }
    } else {
      freezeStreak = 0;
      lastTripletSig = "";
    }

    const haveBar = left && right;
    const havePlayer = !!player;

    // Periodic end-of-minigame probe, even if bar/player appears present.
    if (controlLoop % endProbeEveryLoops === 0) {
      const finPeriodic = await tryEndMinigameUi();
      if (finPeriodic) return finPeriodic;
    }

    /** End-of-minigame when bar + player markers both missing (save CPU). */
    if (!haveBar && !havePlayer) {
      const fin = await tryEndMinigameUi();
      if (fin) return fin;
    }

    if (!haveBar || !havePlayer) {
      incompleteStreak += 1;
      if (incompleteStreak < incompleteDebounceFrames) {
        if (
          debugTrace &&
          incompleteStreak === incompleteDebounceFrames - 1
        ) {
          ctx.logInfo(
            `[控条#${controlLoop}] 不完整 debounce 将满 (${incompleteDebounceFrames}帧) haveBar=${haveBar} havePlayer=${havePlayer}`
          );
        }
        await ctx.anti_detect.sleep({ms: controlPollMs});
        continue;
      }
      const finIncomplete = await tryEndMinigameUi();
      if (finIncomplete) return finIncomplete;
      incompleteStreak = 0;
      maxTryLeft -= 1;
      ctx.logWarn(
        `识别不完整（绿条或光标），剩余尝试: ${maxTryLeft} | haveBar=${haveBar} havePlayer=${havePlayer} | L/R/P=${fmtMatch(left)}/${fmtMatch(right)}/${fmtMatch(player)}`
      );
      if (maxTryLeft <= 0) {
        ctx.logError("尝试次数用尽，控条放弃");
        heldDir = 0;
        await releaseKeys();
        return { ok: true, reason: "control_give_up" };
      }
      leftLock = null;
      rightLock = null;
      playerLock = null;
      heldDir = 0;
      await releaseKeys();
      await ctx.anti_detect.keyPress({key: "f"});
      await ctx.anti_detect.sleep({ms: Math.max(floorMs, 120)});
      continue;
    }

    incompleteStreak = 0;

    const leftBound = left.x;
    const rightBound = right.x;
    const barCenter = (leftBound + rightBound) / 2;
    const playerCenter = player.x + player.width / 2;
    const offset = playerCenter - barCenter;
    if (lastOffset != null && Math.abs(offset - lastOffset) <= 2) {
      staleOffsetStreak += 1;
    } else {
      staleOffsetStreak = 0;
      lastOffset = offset;
    }
    if (staleOffsetStreak >= staleOffsetLoops && Math.abs(offset) > deadzone * 2) {
      ctx.logWarn(
        `[控条#${controlLoop}] 偏移长时间冻结(${offset.toFixed(
          1
        )}px)，清空锁定并重采样`
      );
      leftLock = null;
      rightLock = null;
      playerLock = null;
      pulseDir = 0;
      pulseBoost = 1.0;
      prevAbsOffset = 0;
      staleOffsetStreak = 0;
      lastOffset = null;
      await ctx.anti_detect.sleep({ms: controlPollMs});
      continue;
    }

    if (
      haveBar &&
      havePlayer &&
      Math.abs(offset) >= offsetSanityProbePx &&
      controlLoop % sanityProbeEvery === 0
    ) {
      const finOff = await tryEndMinigameUi();
      if (finOff) return finOff;
    }

    if (controlMode === "pulse") {
      let controlOffset = offset;
      if (predictiveEnabled && prevPredictOffset != null) {
        const velocity = offset - prevPredictOffset;
        const predicted = offset + velocity * predictiveGain;
        controlOffset = Math.max(
          -predictiveMaxPx,
          Math.min(predictiveMaxPx, predicted)
        );
        // Avoid overreacting near center when prediction flips sign.
        if (
          Math.abs(offset) <= deadzone * 2 &&
          Math.sign(controlOffset) !== Math.sign(offset)
        ) {
          controlOffset = offset;
        }
      }
      prevPredictOffset = offset;

      const absOffset = Math.abs(controlOffset);
      const baseMs = (absOffset / cursorPxPerSec) * 1000.0;
      const baseDurationMs = Math.min(
        pulseTapMaxMs,
        capMs,
        Math.max(floorMs, Math.floor(baseMs * factor))
      );
      const dynamicBoostCap = Math.max(
        1.0,
        Math.min(pulseAccelCap, 1.0 + absOffset / 95.0)
      );
      const dynamicDurationCap = Math.max(
        pulseTapMaxMs,
        Math.min(pulseTapAccelMaxMs, Math.floor(pulseTapMaxMs + absOffset * 0.28))
      );

      let pulsed = false;
      if (controlOffset > deadzone) {
        if (pulseDir === 1) {
          if (absOffset >= prevAbsOffset + pulseAccelTrendPx) {
            pulseBoost = Math.min(
              dynamicBoostCap,
              pulseBoost + pulseAccelStep * 1.5
            );
          } else if (absOffset >= prevAbsOffset - pulseAccelTrendPx) {
            pulseBoost = Math.min(dynamicBoostCap, pulseBoost + pulseAccelStep);
          } else {
            pulseBoost = Math.max(
              1.0,
              pulseBoost - pulseAccelDecay
            );
          }
        } else {
          pulseDir = 1;
          pulseBoost = 1.0;
        }
        const durationMs = Math.min(
          dynamicDurationCap,
          capMs,
          Math.max(
            pulseTapMinMs,
            Math.floor(baseDurationMs * pulseBoost)
          )
        );
        pulsed = true;
        const asyncDurationMs = Math.max(
          pulseTapMinMs,
          scaledDurationMs(durationMs, absOffset, pulseTapMinMs)
        );
        ctx.logDebug(
          `控条[点按] offset=${offset.toFixed(1)}px pred=${controlOffset.toFixed(1)}px → A ${asyncDurationMs}ms (boost=${pulseBoost.toFixed(2)})`
        );
        const fired = fireKeyPress("a", asyncDurationMs);
        if (!fired) {
          ctx.logDebug("控条[点按] A 忙，跳过本轮");
        }
      } else if (controlOffset < -deadzone) {
        if (pulseDir === -1) {
          if (absOffset >= prevAbsOffset + pulseAccelTrendPx) {
            pulseBoost = Math.min(
              dynamicBoostCap,
              pulseBoost + pulseAccelStep * 1.5
            );
          } else if (absOffset >= prevAbsOffset - pulseAccelTrendPx) {
            pulseBoost = Math.min(dynamicBoostCap, pulseBoost + pulseAccelStep);
          } else {
            pulseBoost = Math.max(
              1.0,
              pulseBoost - pulseAccelDecay
            );
          }
        } else {
          pulseDir = -1;
          pulseBoost = 1.0;
        }
        const durationMs = Math.min(
          dynamicDurationCap,
          capMs,
          Math.max(
            pulseTapMinMs,
            Math.floor(baseDurationMs * pulseBoost)
          )
        );
        pulsed = true;
        const asyncDurationMs = Math.max(
          pulseTapMinMs,
          scaledDurationMs(durationMs, absOffset, pulseTapMinMs)
        );
        ctx.logDebug(
          `控条[点按] offset=${offset.toFixed(1)}px pred=${controlOffset.toFixed(1)}px → D ${asyncDurationMs}ms (boost=${pulseBoost.toFixed(2)})`
        );
        const fired = fireKeyPress("d", asyncDurationMs);
        if (!fired) {
          ctx.logDebug("控条[点按] D 忙，跳过本轮");
        }
      } else if (controlLoop % 6 === 0) {
        pulseDir = 0;
        pulseBoost = 1.0;
        ctx.logDebug(
          `控条[点按] 死区内 offset=${offset.toFixed(1)}px（节流）`
        );
      }
      if (absOffset <= deadzone * 2) {
        pulseBoost = Math.max(1.0, pulseBoost - pulseAccelDecay * 1.5);
      }
      prevAbsOffset = absOffset;

      let restMs = controlPollMs;
      if (pulsed) {
        restMs = Math.min(controlPollMs, pulseFastRestMs);
      }
      if (pulsed && baseDurationMs >= longPulseThresholdMs) {
        restMs = Math.min(restMs, longPulseTailMs);
      }
      await ctx.anti_detect.sleep({ms: restMs});
    } else if (controlMode === "hold") {
      if (Math.abs(offset) <= deadzone) {
        if (heldDir !== 0) {
          await releaseKeys();
          heldDir = 0;
        }
      } else if (offset > deadzone) {
        if (heldDir !== 1) {
          await ctx.keyDown("a");
          await ctx.keyUp("d");
          heldDir = 1;
          ctx.logDebug(
            `控条[长按] 偏右 ${offset.toFixed(1)}px → 按住 A`
          );
        }
      } else if (offset < -deadzone) {
        if (heldDir !== -1) {
          await ctx.keyDown("d");
          await ctx.keyUp("a");
          heldDir = -1;
          ctx.logDebug(
            `控条[长按] 偏左 ${(-offset).toFixed(1)}px → 按住 D`
          );
        }
      }
      if (heldDir !== 0 && controlLoop % 20 === 0) {
        ctx.logDebug(
          `控条[长按] offset=${offset.toFixed(1)}px`
        );
      }
      await ctx.anti_detect.sleep({ms: controlPollMs});
    } else {
      const absOffset = Math.abs(offset);
      const baseMs = (absOffset / cursorPxPerSec) * 1000.0;
      const durationMs = Math.min(
        pulseTapMaxMs,
        trackTapCapMs,
        Math.max(
          trackTapFloorMs,
          Math.floor(baseMs * factor * trackTapScale)
        )
      );

      let pulsed = false;
      if (offset > deadzone) {
        pulsed = true;
        const asyncDurationMs = Math.max(
          trackTapFloorMs,
          scaledDurationMs(durationMs, absOffset, trackTapFloorMs)
        );
        ctx.logDebug(
          `控条[跟踪] 偏右 ${offset.toFixed(1)}px → 短点 A ${asyncDurationMs}ms`
        );
        fireKeyPress("a", asyncDurationMs);
      } else if (offset < -deadzone) {
        pulsed = true;
        const asyncDurationMs = Math.max(
          trackTapFloorMs,
          scaledDurationMs(durationMs, absOffset, trackTapFloorMs)
        );
        ctx.logDebug(
          `控条[跟踪] 偏左 ${(-offset).toFixed(1)}px → 短点 D ${asyncDurationMs}ms`
        );
        fireKeyPress("d", asyncDurationMs);
      } else if (controlLoop % 6 === 0) {
        ctx.logDebug(
          `控条[跟踪] 死区内 offset=${offset.toFixed(1)}px（节流）`
        );
      }

      let restMs = controlPollMs;
      if (pulsed && durationMs >= longPulseThresholdMs) {
        restMs = Math.min(controlPollMs, longPulseTailMs);
      }
      await ctx.anti_detect.sleep({ms: restMs});
    }
  }
}

// 出售鱼获
async function sellFish() {
  while (true) {
    const seaFisher = await ctx.findTemplate("海上钓客", { roi: { x: 22, y: 15, width: 371, height: 182 }, threshold: 0.95, grayscale: true });
    if (seaFisher) {
      await ctx.sleep()
      ctx.logInfo("检测到海上钓客，点击");
      
      const fishMarket = await ctx.findTemplate("鱼获市场", { roi: { x: 248, y: 239, width: 264, height: 167 }, threshold: 0.95, grayscale: true });
      const guildFishCave = await ctx.findTemplate("归流鱼舱", { roi: { x: 220, y: 175, width: 383, height: 245 }, threshold: 0.95, grayscale: true });

      if (fishMarket) {
        await ctx.anti_detect.click({x: 1540, y: 967});
        await ctx.anti_detect.sleep({ms: 2000});
        await ctx.anti_detect.click({x: 964, y: 1070});
        await ctx.anti_detect.sleep({ms: 2000});
        
        await ctx.anti_detect.click({x: 152, y: 409});
        await ctx.anti_detect.sleep({ms: 1000});
      }
      
      if (guildFishCave) {
        await ctx.anti_detect.click({x: 1054, y: 961});
        await ctx.anti_detect.sleep({ms: 2000});
        await ctx.anti_detect.click({x: 1163, y: 703});
        await ctx.anti_detect.sleep({ms: 2000});
        await ctx.anti_detect.click({x: 892, y: 961});
        await ctx.anti_detect.sleep({ms: 2000});
        await ctx.anti_detect.keyPress({key: 'esc'});
        await ctx.anti_detect.sleep({ms: 2000});
        return true;
      }
    }
  }
}

async function prepareBait() {
  const tuning = taskParams();
  const baitBuyCount = Math.max(1, Math.min(10, Number(tuning.baitBuyCount ?? 3)));
  let baitLoop = 0;
  let lastShopBuyAt = 0;
  const ocrProbeEveryLoops = Math.max(1, Number(tuning.ocrProbeEveryLoops ?? 3));
  while (true) {
    baitLoop += 1;
    const isChangeBait = await ctx.findTemplate("更换鱼饵界面", { roi: { x: 877, y: 335, width: 301, height: 174 }, threshold: 0.95, grayscale: true });
    if (isChangeBait) {
      const defaultBait = await ctx.findTemplate("默认鱼饵", { roi: { x: 667, y: 488, width: 190, height: 186 }, threshold: 0.95, grayscale: true });
      if (defaultBait) {
        ctx.logInfo("检测到默认鱼饵，点击");
        await ctx.anti_detect.click({x: 711, y: 541});
        await ctx.anti_detect.sleep({ms: 1000});
      }
      const isWanNengBait = await ctx.findTemplate("万能鱼饵", { roi: { x: 711, y: 243, width: 420, height: 211 }, threshold: 0.95, grayscale: true });
      if (isWanNengBait) {
        ctx.logInfo("检测到万能鱼饵，点击");
        await ctx.anti_detect.click({x: 1337, y: 550});
        await ctx.anti_detect.sleep({ms: 1000});
      }
    }
    // 开始切换鱼饵
    const buyBaitButton = await ctx.findTemplate("更换鱼饵确认按钮", { roi: { x: 1101, y: 682, width: 236, height: 147 }, threshold: 0.95, grayscale: true });
    if (buyBaitButton) {
      ctx.logInfo("检测到更换鱼饵确认按钮，点击");
      await ctx.anti_detect.click({x: 1174, y: 711});
      await ctx.anti_detect.sleep({ms: 1000});
    }
    const changeBaitButton = await ctx.findTemplate("更换按钮", {
      roi: { x: 989, y: 665, width: 453, height: 184 },
      threshold: 0.95,
      grayscale: true,
    });
    if (changeBaitButton) {
      ctx.logInfo("检测到更换鱼饵按钮，点击333");
      await ctx.anti_detect.click({x: 1174, y: 711});
      await ctx.anti_detect.sleep({ms: 1000});
      return true;
    }
    const match = await ctx.findTemplate("万能鱼饵2", { roi: { x: 37, y: 159, width: 752, height: 992 }, threshold: 0.90, grayscale: true });
    if (match) {
      ctx.logInfo("检测到万能鱼饵选择，点击" + JSON.stringify(match));
      await ctx.anti_detect.click({x: match.x + 20, y: match.y + 20})
      await ctx.anti_detect.sleep({ms: 1000})
    }
    const isShop = await ctx.findTemplate("渔具商店", { roi: { x: 25, y: 26, width: 354, height: 166 }, threshold: 0.95, grayscale: true });
    if (isShop) {
      const now = Date.now();
      if (now - lastShopBuyAt >= 2600) {
        ctx.logInfo("检测到渔具商店2222，点击" + JSON.stringify(isShop));
        await ctx.anti_detect.sleep({ms: 1500})
        const match = await ctx.findTemplate("万能鱼饵2", { threshold: 0.90, grayscale: true });
        ctx.logInfo("mei检测到万能鱼饵选择，点击" + JSON.stringify(match));
        if (match) {
          ctx.logInfo("检测到万能鱼饵选择，点击" + JSON.stringify(match));
          await ctx.anti_detect.click({x: match.x + 20, y: match.y + 20})
          await ctx.anti_detect.sleep({ms: 1000})
        }
        await ctx.anti_detect.sleep({ms: 1500})
        // 点击购买鱼饵（次数由 baitBuyCount 控制）
        for (let i = 0; i < baitBuyCount; i++) {
          await ctx.anti_detect.click({x: 1772, y: 950});
          await ctx.anti_detect.sleep({ms: 1500});
        }

        //点击购买按钮
        await ctx.anti_detect.click({x: 1618, y: 1041})
        await ctx.anti_detect.sleep({ms: 500});
        lastShopBuyAt = now;
      }
    }
    const shouldProbeOcr = baitLoop % ocrProbeEveryLoops === 0;
    const noMoneyText = shouldProbeOcr
      ? await ctx.ocr(793, 495, 438, 183)
      : "";
    const noMoneyTpl = await ctx.findTemplate("货币不足无法购买", {
      roi: { x: 793, y: 495, width: 438, height: 183 },
      threshold: 0.95,
      useAlphaMask: true,
      grayscale: true,
    });
    if (String(noMoneyText).indexOf("货币不足无法购买") >= 0 || noMoneyTpl) {
      ctx.logInfo("货币不足无法购买");
      await ctx.anti_detect.sleep({ms: 1000})
      await ctx.anti_detect.keyPress({key: 'esc'});
      await ctx.anti_detect.sleep({ms: 1000})
      await ctx.anti_detect.click({x: 1523, y: 988})
      await sellFish();
      return false;
    }

    // Find template in region
    const buySuccess = await ctx.findTemplate("购买成功", { roi: { x: 916, y: 317, width: 192, height: 437 }, threshold: 0.95, grayscale: true });
    if (buySuccess) {
      ctx.logInfo("检测到购买成功，点击");
      await ctx.anti_detect.click({x: 965, y: 960})
      await ctx.anti_detect.sleep({ms: 1500})
      await ctx.anti_detect.keyPress({key: 'esc'})
      await ctx.anti_detect.sleep({ms: 1000})
      break
    }
  }
}


async function start1() {
  const tuning = taskParams();
  ctx.logInfo("钓鱼辅助V2 启动");
  let lastFPressAt = 0;
  let minigameProbeStreak = 0;
  let hookedStreak = 0;
  let auxBarPlayerStreak = 0;
  let interactStreak = 0;
  const fPressCooldownMs = Math.max(200, Number(tuning.startFPressCooldownMs ?? 700));
  const afterFWaitMs = Math.max(80, Number(tuning.startAfterFWaitMs ?? 260));
  let startLoop = 0;
  const ocrProbeEveryLoops = Math.max(1, Number(tuning.ocrProbeEveryLoops ?? 3));
  const startUseOcr = tuning.startUseOcr === true;
  const minigameProbeEveryLoops = Math.max(1, Number(tuning.startMinigameProbeEveryLoops ?? 2));
  const baitBuyCount = Math.max(1, Math.min(10, Number(tuning.baitBuyCount ?? 3)));

  while (true) {
    startLoop += 1;
    const shouldProbeOcr = startUseOcr && startLoop % ocrProbeEveryLoops === 0;
    
    const text = await ctx.ocr(1767, 545, 188, 133);
    if (text.indexOf('向左溜鱼') !== -1) {
      await ctx.anti_detect.keyPress({key: "f"});
      await ctx.anti_detect.sleep({ms: 300});
    }
    const [interact, prepare, levelTpl] = await ctx.findTemplateBatch([
      { name: "吊杆-removebg-preview", roi: { x: 1369, y: 948, width: 170, height: 166 }, greenMask: true, threshold: 0.97, grayscale: true },
      { name: "钓鱼准备", roi: { x: 1241, y: 66, width: 289, height: 157 }, threshold: 0.95, grayscale: true },
      { name: "垂钓等级", threshold: 0.88, grayscale: true, roi: { x: 688, y: 98, width: 212, height: 150 } },
    ]);
    if (prepare) {
      ctx.logInfo("检测到钓鱼准备，点击" + JSON.stringify(prepare));
      const noBaitText = shouldProbeOcr
        ? await ctx.ocr(1642, 815, 286, 182)
        : "";
      const noBaitTpl = await ctx.findTemplate("未选择鱼饵", {
        roi: { x: 1642, y: 815, width: 286, height: 182 },
        threshold: 0.92,
        grayscale: true,
      });
      if (String(noBaitText).indexOf("未选择") !== -1 || noBaitTpl) { // 未选择鱼饵
        await ctx.anti_detect.click({x: 1724, y: 773})
        await ctx.anti_detect.sleep({ms: 2000})
        await prepareBait();
        await ctx.anti_detect.sleep({ms: 2000})
      }
    
      await ctx.anti_detect.click({x: 1617, y: 961})
      await ctx.anti_detect.sleep({ms: 1000})
      continue;
    }
    if (levelTpl) {
      await ctx.click(970, 1014)
      await ctx.sleep(1000)
    }
    const needBaitText = shouldProbeOcr
      ? await ctx.ocr(760, 492, 501, 189)
      : "";
    const needBaitTpl = await ctx.findTemplate("需要准备鱼饵", {
      roi: { x: 760, y: 492, width: 501, height: 189 },
      threshold: 0.94,
      grayscale: true,
    });
    if (
      String(needBaitText).indexOf("需要装备鱼饵才可以钓鱼") >= 0 ||
      needBaitTpl
    ) {
      await ctx.anti_detect.click({x: 1648, y: 974})
      await ctx.anti_detect.sleep({ms: 1000})
      await prepareBait()
    }
  
    if (interact) {
      interactStreak += 1;
      const now = Date.now();
      if (now - lastFPressAt >= fPressCooldownMs) {
        ctx.logInfo("检测到钓鱼交互，按 F");
        await ctx.anti_detect.keyPress({key: "f"});
        await ctx.anti_detect.sleep({ms: 300});
        lastFPressAt = now;
        await ctx.anti_detect.sleep({ms: afterFWaitMs});
      }
    } else {
      interactStreak = 0;
    }

    const c1 = await ctx.countColor('#FFFFFF', { roi: { x: 1708, y: 892, width: 257, height: 327 }});
    const c2 = await ctx.countColor('#207CFF', { roi: { x: 1708, y: 892, width: 257, height: 327 }});
    const c5 = await ctx.countColor('#2CCCAF', { roi: { x: 565, y: 11, width: 888, height: 232 }});

    if (c1 >= 300 && c2 >= 300) {
      hookedStreak += 1;
      break
    } else if (c5 >= 100) {
      hookedStreak += 1;
      break
    } else {
      hookedStreak = 0;
    }
    
    // Auxiliary gate: stamina bar + player marker improve minigame entry accuracy.
    let auxValid = false;
    if (startLoop % minigameProbeEveryLoops === 0) {
      const [leftProbe, rightProbe, playerProbe] = await ctx.findTemplateBatch([
        { name: "左侧", roi: { x: 577, y: 55, width: 784, height: 100 }, threshold: 0.95, useAlphaMask: true, grayscale: true },
        { name: "右侧", roi: { x: 577, y: 55, width: 784, height: 100 }, threshold: 0.95, useAlphaMask: true, grayscale: true },
        { name: "玩家", roi: { x: 577, y: 55, width: 784, height: 100 }, threshold: 0.98, useAlphaMask: true, grayscale: true },
      ]);
      if (leftProbe && rightProbe && playerProbe) {
        const lx = leftProbe.x;
        const rx = rightProbe.x;
        const py = playerProbe.y;
        const barWidth = rx - lx;
        const confOk =
          (leftProbe.confidence || 0) >= 0.97 &&
          (rightProbe.confidence || 0) >= 0.97 &&
          (playerProbe.confidence || 0) >= 0.99;
        const geomOk =
          lx < rx &&
          barWidth >= 80 &&
          barWidth <= 230 &&
          leftProbe.y >= 90 &&
          rightProbe.y >= 90 &&
          py >= 90 &&
          Math.abs(leftProbe.y - rightProbe.y) <= 10 &&
          Math.abs(py - leftProbe.y) <= 16;
        if (confOk && geomOk) {
          minigameProbeStreak += 1;
          auxBarPlayerStreak += 1;
          auxValid = true;
        } else {
          minigameProbeStreak = 0;
          auxBarPlayerStreak = 0;
        }
      } else {
        minigameProbeStreak = 0;
        auxBarPlayerStreak = 0;
      }
    }

    // Entry rule: hook template as main signal, bar/player as auxiliary confirmation.
    if (hookedStreak >= 1 && (auxValid || auxBarPlayerStreak >= 1)) {
      ctx.logInfo("检测到鱼上钩了 + 耐力条/玩家条，开始小游戏");
      break;
    }
    if (interactStreak >= 8 && auxBarPlayerStreak >= 1) {
      ctx.logInfo("交互持续命中且检测到耐力条/玩家条，判定进入小游戏");
      break;
    }
    if (minigameProbeStreak >= 3) {
      ctx.logInfo("检测到控条三模板且几何有效，判定进入小游戏");
      break;
    }
  }
  ctx.logInfo("开始钓鱼");
  await playGame();
  await ctx.anti_detect.sleep({ms: 1000})
  await ctx.anti_detect.click({x: 966, y: 981})
  await ctx.anti_detect.sleep({ms: 1000})
}


async function start() {
  const tuning = taskParams();
  const sellEveryN = Math.max(0, Number(tuning.sell_every_n_catches || tuning.sellEveryNCatches || 0));
  let catchCount = 0;

  while (true) {
    ctx.logInfo(`累计钓鱼: ${catchCount} 次`);
    if (sellEveryN > 0 && catchCount >= sellEveryN) {
      ctx.logInfo(`已钓鱼 ${catchCount} 次，开始出售鱼获`);
      await ctx.anti_detect.sleep({ms: 2000});
      await ctx.anti_detect.click({x: 1523, y: 988})
      await ctx.anti_detect.sleep({ms: 1000})
      await sellFish();
      catchCount = 0;
      await ctx.anti_detect.sleep({ms: 2000});
    }
    await start1()
    catchCount += 1;
    await ctx.anti_detect.sleep({ms: 1000})
  }
}