/**
 * Main path + optional restock.
 * Params come from global `config` (see taskParams).
 */
function taskParams() {
  return typeof globalThis.config === "object" && globalThis.config !== null
    ? globalThis.config
    : {};
}


async function start() {
  const cfg = taskParams();
  const enterKey = cfg.enter_key != null ? cfg.enter_key : "f5";
  const exitKey = cfg.exit_key != null ? cfg.exit_key : "esc";
  const enableRestock = cfg.enable_restock === true;
  const maxRounds = cfg.max_rounds != null ? cfg.max_rounds : 25;
  let round = 0;
  let isWithdrawMoney = false;
  let isRestock = false;
  let notNeedRestock = false;
  const isMainScreen = await ctx.lib.isMainScreen();
  ctx.logInfo("isMainScreen: " + JSON.stringify(isMainScreen));
  ctx.logInfo("enableRestock: " + JSON.stringify(config));
  if (isMainScreen) {
    await ctx.anti_detect.keyPress({key: enterKey});
    await ctx.anti_detect.sleep({ms: 5000});
  }
  while (true) {
    const cafe = await ctx.findTemplate("都市大亨", { roi: { x: 32, y: 39, width: 333, height: 142 }, grayscale: true });
    ctx.logInfo("cafe: " + JSON.stringify(cafe));
    if (cafe) {
      await ctx.anti_detect.click({x: 801, y: 781});
      await ctx.anti_detect.sleep({ms: 1000});
    }

    const shop = await ctx.findTemplate("商铺管理", { roi: { x: 16, y: 17, width: 334, height: 156 }, grayscale: true });
    ctx.logInfo("shop: " + JSON.stringify(shop));
    if (shop) {
      
      let needRestock = await ctx.findTemplate("需要补货", { roi: { x: 293, y: 462, width: 173, height: 168 }, threshold: 0.99, grayscale: true });
      ctx.logInfo("needRestock: " + JSON.stringify(needRestock));
      if (!needRestock && enableRestock) {
        const text = await ctx.ocr(207, 404, 184, 156);
        ctx.logInfo("enableRestock: " + JSON.stringify(text));
        if (text.indexOf('不足') >= -1) {
          needRestock = true;
        }
      }
      if (needRestock && enableRestock && !isRestock) {
        await ctx.anti_detect.click({x: 245, y: 574});
        await ctx.anti_detect.sleep({ms: 1000});
        await ctx.anti_detect.click({x: 478, y: 839});
        await ctx.anti_detect.sleep({ms: 1000});
        await ctx.anti_detect.click({x: 631, y: 839});
        await ctx.anti_detect.sleep({ms: 1000});
        await ctx.anti_detect.click({x: 817, y: 839});
        await ctx.anti_detect.sleep({ms: 1000});
        await ctx.anti_detect.click({x: 1359, y: 846});
        await ctx.anti_detect.sleep({ms: 4000});
        await ctx.anti_detect.click({x: 1133, y: 827});
        await ctx.anti_detect.sleep({ms: 1000});
        await ctx.anti_detect.click({x: 1144, y: 699});
        isRestock = true;
      }
      if (!needRestock) {
        notNeedRestock = true;
      }

      // Find template in region
        const text = await ctx.ocr(225, 654, 373, 166);
        ctx.logInfo("text: " + text);
      let withdrawMoney = await ctx.findTemplate("提取收益", { roi: { x: 221, y: 906, width: 368, height: 178 }, threshold: 0.95, grayscale: true });
      if (!isWithdrawMoney && !withdrawMoney) {
        const text = await ctx.ocr(225, 654, 373, 166);
        ctx.logInfo("text: " + text);
        const parts = text.split(' ')
        if (parts.length >= 3) {
          const time = parts[2]
          const hms = time.split(':')
          if (hms.length >= 3) {
            const hours = parseInt(hms[0])
            const minutes = parseInt(hms[1])
            const seconds = parseInt(hms[2])
            const totalSeconds = hours * 3600 + minutes * 60 + seconds
            if (totalSeconds > 60) {
              withdrawMoney = true
            }
          }
        }
      }


      ctx.logInfo("withdrawMoney: " + JSON.stringify(withdrawMoney));
      if (withdrawMoney && !isWithdrawMoney) {
        await ctx.anti_detect.click({x: 357, y: 936});
        await ctx.anti_detect.sleep({ms: 1000});
        await ctx.anti_detect.click({x: 963, y: 907});
        await ctx.anti_detect.sleep({ms: 3000});
        await ctx.anti_detect.click({x: 953, y: 954});
        await ctx.anti_detect.sleep({ms: 1000});
        isWithdrawMoney = true;
      }

    }

    round++;
    const isDone = isWithdrawMoney && (enableRestock ? isRestock || notNeedRestock : true);
    if (round >= maxRounds || isDone) {
      break;
    }
    await ctx.anti_detect.sleep({ms: 1000});
  }
  while (!await ctx.lib.isMainScreen()) {
    await ctx.anti_detect.keyPress({key: exitKey});
    await ctx.anti_detect.sleep({ms: 1000});
  }
}
