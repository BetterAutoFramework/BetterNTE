function taskParams() {
  return typeof globalThis.config === "object" && globalThis.config !== null
    ? globalThis.config
    : {};
}

async function start() {
    console.log('scale:', JSON.stringify(await ctx.getScaleFactors()));
  console.log('frame:', JSON.stringify(await ctx.getFrameSize()));
  const cfg = taskParams();
  const count = cfg.count || 999;
  const loopCount = 0;
  ctx.logInfo("循环次数: " + JSON.stringify(count));
    let l80Times = 0
  while (loopCount < count) {
    const text = await ctx.ocr(1146, 530, 277, 156);
    if (text.indexOf('店长特供') >= 0) {
      await ctx.keyDown("Alt");
      l80Times = 0
      try {
        await ctx.mouseMove(500, 400);
        await ctx.anti_detect.sleep({ms: 80});
        await ctx.mouseMove(1225, 556);
        await ctx.anti_detect.sleep({ms: 220});
        await ctx.anti_detect.click({x: 1225, y: 556});
      } finally {
        await ctx.keyUp("Alt");
      }
      await ctx.anti_detect.sleep({ms: 1000});
    }

    const isTitle = await ctx.findTemplate("店长特供标题", { roi: { x: 15, y: 16, width: 328, height: 170 }, threshold: 0.80, grayscale: true });
    if (isTitle) {
      await ctx.anti_detect.click({x: 175, y: 394})
      await ctx.anti_detect.sleep({ms: 1000});
      await ctx.anti_detect.click({x: 1726, y: 1012});
      await ctx.anti_detect.sleep({ms: 1000});
    }

    const isGame = await ctx.findTemplate("游戏界面", { roi: { x: 1531, y: 151, width: 339, height: 281 }, threshold: 0.80, grayscale: true });
    if (isGame) {
      await ctx.anti_detect.click({x: 102, y: 451})
      const count = await ctx.countColor("#FFD742", { tolerance: 10, roi: { x: 1812, y: 204, width: 70, height: 70 } });
      ctx.logInfo('count' + count)
      if (count >= 50) {
        await ctx.anti_detect.click({x: 51, y: 51})
        await ctx.anti_detect.sleep({ms: 2000});
        await ctx.anti_detect.click({x: 1175, y: 844});
        await ctx.anti_detect.sleep({ms: 2000});
        loopCount++;
      }

    }
    await ctx.anti_detect.sleep({ms: 1000});
  }
}
