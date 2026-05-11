

<div align="center">
  <img alt="LOGO" src="./assets/logo.png" width="256" height="256"  />


  <h1 align="center">BetterNTE</h1>

  <p align="center">
    <br/>
    基于计算机视觉技术的《异环》自动化工具
      <br/>
      使用 Rust 从零开始打造
  </p>


  <p align="center">
    <a href="https://github.com/BetterAutoFramework/BetterNTE/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/badge/license-GPL--v3-blue?style=flat-square" /></a>
    <a href="https://www.rust-lang.org/"><img alt="Rust" src="https://img.shields.io/badge/Rust-2021-orange?logo=rust&style=flat-square" /></a>
    <a href="https://tauri.app/"><img alt="Tauri" src="https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri&style=flat-square" /></a>
    <img src="https://img.shields.io/badge/Platform-Windows-0078D7?style=flat-square&logo=Windows" alt="Platform" />
    <br/>
    <a href="https://github.com/BetterAutoFramework/BetterNTE/issues"><img alt="Issues" src="https://img.shields.io/github/issues/BetterAutoFramework/BetterNTE?style=flat-square" /></a>
    <img src="https://img.shields.io/github/stars/BetterAutoFramework/BetterNTE?style=flat-square&logo=github&color=darkgreen" alt="Stars" />
    <img alt="commit" src="https://img.shields.io/github/commit-activity/m/BetterAutoFramework/BetterNTE?color=%23ff69b4" />
  </p>
</div>

> **[简体中文](README.md)** | [English](README_EN.md)

> 💡 本项目还处于早期开发阶段，可能存在诸多问题与不足。如果你在使用过程中遇到了 Bug、体验不佳、或有任何建议，**欢迎积极提交 [Issue](https://github.com/BetterAutoFramework/BetterNTE/issues) 反馈**，也可以加入 QQ 群一起讨论，帮助我们不断完善项目！

> 💬 QQ 交流群: 1102341902

> ⚠️ 请从官方 [Releases](https://github.com/BetterAutoFramework/BetterNTE/releases) 页面下载软件。通过非官方途径获取的软件可能**含有病毒**，并且一般不是最新的版本，请注意甄别。

## 功能一览

- **视觉引擎**：截图（多后端）、模板匹配、OCR 识别、目标检测、图像分类
- **输入模拟**：键鼠操作模拟
- **脚本系统**：JavaScript 脚本热重载，API 调用引擎能力，manifest 声明式配置
- **桌面客户端**：基于 Tauri 的精美 UI，支持脚本运行、日志查看

## 内置脚本

| 脚本 | 类型 | 说明 |
|------|------|------|
| **钓鱼辅助 V2** | 任务 | 自动钓鱼全流程：自动控制鱼竿小游戏、自动购买/切换鱼饵、自动出售鱼获 |
| **自动领取一咖舍收益** | 任务 | 进入都市大亨界面后自动领取一咖舍收益，可选自动补货 |
| **店长特供** | 任务 | 一咖舍柜台前自动完成店长特供小游戏，循环刷取奖励 |
| **自动跳过 / 传送确认** | 触发器 | 每帧检测并自动跳过剧情、确认传送（维特海默塔、电话亭）、领取开采凭证 |
| **工具库** | 库 | 通用辅助函数（模板查找、ROI 构造、主界面检测），供其他脚本依赖调用 |



## 下载

- [GitHub Releases](https://github.com/BetterAutoFramework/BetterNTE/releases)
- [夸克网盘](https://pan.quark.cn/s/be7f6bcea757)

## 使用方法

你的系统需要满足以下条件：

- Windows 10 或更高版本的 64 位系统

> 📌 游戏需要运行在 `1920x1080` 窗口化下。
>
> <div align="center">
>   <img src="assets/snapshots/game_setting.png" alt="游戏设置界面" width="800"/>
>   <p>主界面</p>
> </div>

## 常见问题

- **为什么需要管理员权限？**
  因为游戏通常以管理员权限运行，软件需要同等权限才能模拟输入操作。

- **支持哪些分辨率？**
  推荐在 `1920x1080` 窗口化下使用，目前仅支持 `16:9` 比例分辨率。

- **遇到问题怎么办？**
  请先查看 [Issues](https://github.com/BetterAutoFramework/BetterNTE/issues)，如未找到解决方案可以提交新的 Issue。

## 文档

| 文档 | 说明 |
|------|------|
| [开发指南](docs/development.md) | 环境配置、项目结构、构建与调试 |
| [脚本开发指南](docs/scripting-guide.md) | manifest、`ctx` API、触发器、Flow 定义 |

## ⚠️ 免责声明

BetterNTE 是一款开源、免费的辅助工具，仅用于学习与交流目的。

- **工作原理**：通过计算机视觉识别游戏界面并与之交互，不会修改任何游戏文件、内存或网络数据。
- **使用目的**：为玩家提供操作便利，不涉及破坏游戏平衡或获取不公平优势。
- **风险自担**：使用者应自行评估并承担因使用本工具而产生的一切后果，包括但不限于账号处罚。本项目及开发者不对任何因使用本软件导致的损失负责。
- **商业用途**：第三方利用本软件进行代练、收费等商业行为，与本项目无关。

> **请注意：** 根据[《异环》公平游戏宣言](https://yh.wanmei.com/news/gamebroad/20260202/260701.html)，严禁使用任何第三方工具破坏游戏公平性，违规者可能面临扣除收益、冻结或永久封禁账号等处罚。
>
> 使用本工具即表示您已充分了解上述风险并自愿承担相应后果。

## 鸣谢

本项目的完成离不开以下项目：

- [Tauri](https://tauri.app/) — 跨平台桌面应用框架
- [QuickJS](https://bellard.org/quickjs/)（通过 `rquickjs`）— 嵌入式 JavaScript 运行时
- [opencv-rust](https://github.com/twistedfall/opencv-rust) — OpenCV Rust 绑定，用于图像识别
- [better-genshin-impact](https://github.com/babalae/better-genshin-impact) — 参考了其架构设计与部分实现思路
- [MaaFramework](https://github.com/MaaXYZ/MaaFramework) — 参考了其架构设计与部分实现思路
- [MaaNTE](https://github.com/1bananachicken/MaaNTE) — 参考了其任务流程的部分实现思路

### 贡献者

感谢所有参与到测试与开发中的开发者！

[![Contributors](https://contributors-img.web.app/image?repo=BetterAutoFramework/BetterNTE&max=200&columns=15)](https://github.com/BetterAutoFramework/BetterNTE/graphs/contributors)

## 许可证

![GPL-v3](https://www.gnu.org/graphics/gplv3-127x51.png)

[GPL-v3 License](LICENSE)

---

如果觉得软件对你有帮助，帮忙点个 Star 吧！（网页最上方右上角的小星星），这就是对我们最大的支持了！
