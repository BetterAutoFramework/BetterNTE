// 自动跳过剧情 / 传送确认
// 模板名、ROI、点击与等待均为脚本常量；界面仅五个布尔开关（ctx.params）。

const MATCH_THRESHOLD = 0.95;

const TPL_SKIP = "跳过按钮";
const TPL_STORY = "是否跳过剧情";
const TPL_TOWER = "维特海默塔";
const TPL_PHONE = "电话亭";
const TPL_DISMISS = "今日不在提示关闭";
const TPL_CLAIM = "开采凭证";
const TPL_AGE = "适龄提示";

/** 固定 ROI */
const ROI_SKIP = { x: 1833, y: 38, width: 147, height: 148 };
const ROI_STORY = { x: 844, y: 415, width: 332, height: 162 };
const ROI_TOWER = { x: 1401, y: 61, width: 425, height: 247 };
const ROI_PHONE = { x: 1396, y: 62, width: 480, height: 241 };
const ROI_DISMISS = { x: 836, y: 532, width: 157, height: 158 };
const ROI_CLAIM = { x: 793, y: 94, width: 439, height: 183 };
const ROI_AGE = { x: 49, y: 57, width: 191, height: 216 };

/** 固定点击 */
const CLICK_SKIP = { x: 1867, y: 94 };
const CLICK_STORY = { x: 1233, y: 667 };
const CLICK_TOWER = { x: 1527, y: 971 };
const CLICK_PHONE = { x: 1655, y: 965 };
const CLICK_DISMISS = { x: 867, y: 561 };
const CLICK_CLAIM = { x: 968, y: 936 };
const CLICK_AGE = { x: 968, y: 981 };

/** 固定等待（毫秒） */
const WAIT_DISMISS_MS = 500;
const WAIT_AFTER_SKIP_MS = 2000;
const WAIT_AFTER_TELEPORT_MS = 3000;
const WAIT_AFTER_LOGIN_UI_MS = 1500;

/** @param {Record<string, unknown>} raw */
function buildConfig(raw) {
  const p = raw && typeof raw === "object" ? raw : {};
  const defTrue = (key) => p[key] !== false;
  return {
    enable_skip: defTrue("enable_skip_button"),
    enable_story: defTrue("enable_story_confirm"),
    enable_tower: defTrue("enable_witte_tower"),
    enable_phone: defTrue("enable_phone_booth"),
    enable_claim: defTrue("enable_auto_claim_voucher"),
    enable_enter: defTrue("enable_auto_enter_game"),
    verbose: p.enable_verbose_log === true,
  };
}

async function scanMatches(c) {
  const parts = [];
  if (c.enable_skip) parts.push({ slot: 0, name: TPL_SKIP, roi: ROI_SKIP });
  if (c.enable_story) parts.push({ slot: 1, name: TPL_STORY, roi: ROI_STORY });
  if (c.enable_tower) parts.push({ slot: 2, name: TPL_TOWER, roi: ROI_TOWER });
  if (c.enable_phone) parts.push({ slot: 3, name: TPL_PHONE, roi: ROI_PHONE });
  if (c.enable_claim) parts.push({ slot: 4, name: TPL_CLAIM, roi: ROI_CLAIM });
  if (c.enable_enter) parts.push({ slot: 5, name: TPL_AGE, roi: ROI_AGE });

  const bySlot = [null, null, null, null, null, null];
  if (parts.length === 0) {
    return { bySlot, parts };
  }

  const batch = parts.map((x) => ({
    name: x.name,
    roi: x.roi,
    threshold: MATCH_THRESHOLD,
    grayscale: true,
  }));
  const results = await ctx.findTemplateBatch(batch);
  parts.forEach((p, i) => {
    bySlot[p.slot] = results[i];
  });
  return { bySlot, parts };
}

function onEnable() {
  ctx.logInfo("[auto_skip] 触发器已启用");
}

async function onTrigger(triggerCtx) {
  const c = buildConfig(triggerCtx.params);
  let { bySlot: m } = await scanMatches(c);
  if (c.verbose) {
    ctx.logInfo("[auto_skip] matches=" + JSON.stringify(m));
  }

  if (c.enable_skip && m[0]) {
    await ctx.anti_detect.click({x: CLICK_SKIP.x, y: CLICK_SKIP.y});
    await ctx.anti_detect.sleep({ms: WAIT_AFTER_SKIP_MS});
    const r = await scanMatches(c);
    m = r.bySlot;
    if (c.verbose) {
      ctx.logInfo("[auto_skip] after skip click matches=" + JSON.stringify(m));
    }
  }

  if (c.enable_story && m[1]) {
    const dismiss = await ctx.findTemplate(TPL_DISMISS, {
      roi: ROI_DISMISS,
      threshold: MATCH_THRESHOLD,
      grayscale: true,
    });
    if (dismiss) {
      await ctx.anti_detect.click({x: CLICK_DISMISS.x, y: CLICK_DISMISS.y});
      await ctx.anti_detect.sleep({ms: WAIT_DISMISS_MS});
    }
    await ctx.anti_detect.click({x: CLICK_STORY.x, y: CLICK_STORY.y});
    await ctx.anti_detect.sleep({ms: WAIT_AFTER_TELEPORT_MS});
  }

  if (c.enable_tower && m[2]) {
    await ctx.anti_detect.click({x: CLICK_TOWER.x, y: CLICK_TOWER.y});
    await ctx.anti_detect.sleep({ms: WAIT_AFTER_TELEPORT_MS});
  }

  if (c.enable_phone && m[3]) {
    await ctx.anti_detect.click({x: CLICK_PHONE.x, y: CLICK_PHONE.y});
    await ctx.anti_detect.sleep({ms: WAIT_AFTER_TELEPORT_MS});
  }

  if (c.enable_claim && m[4]) {
    await ctx.anti_detect.click({x: CLICK_CLAIM.x, y: CLICK_CLAIM.y});
    await ctx.anti_detect.sleep({ms: WAIT_AFTER_LOGIN_UI_MS});
  }

  if (c.enable_enter && m[5]) {
    await ctx.anti_detect.click({x: CLICK_AGE.x, y: CLICK_AGE.y});
    await ctx.anti_detect.sleep({ms: WAIT_AFTER_LOGIN_UI_MS});
  }
}

function onDisable() {
  ctx.logInfo("[auto_skip] 触发器已停用");
}
