//! 常量定义：版本号、默认端口、默认路径等

/// BetterNTE 引擎版本号
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// API 默认端口
pub const DEFAULT_API_PORT: u16 = 23330;

/// API 默认绑定地址
pub const DEFAULT_API_HOST: &str = "127.0.0.1";

/// ADB 默认服务器地址
pub const DEFAULT_ADB_SERVER: &str = "127.0.0.1:5037";

/// 配置文件名
pub const ENGINE_CONFIG_FILENAME: &str = "engine.yaml";

/// 脚本目录名
pub const SCRIPTS_DIR: &str = "scripts";

/// 资源目录名
pub const ASSETS_DIR: &str = "assets";

/// 日志目录名
pub const LOGS_DIR: &str = "logs";

/// 任务组配置文件名
pub const TASK_GROUPS_FILENAME: &str = "task_groups.json";

/// 模板匹配默认阈值
pub const DEFAULT_TEMPLATE_MATCH_THRESHOLD: f64 = 0.8;

/// OCR 检测默认阈值
pub const DEFAULT_OCR_DET_THRESHOLD: f64 = 0.3;

/// OCR 识别默认阈值
pub const DEFAULT_OCR_REC_THRESHOLD: f64 = 0.5;

/// 帧环形缓冲区默认容量
pub const DEFAULT_FRAME_BUFFER_CAPACITY: usize = 3;

/// 输入队列默认容量
pub const DEFAULT_INPUT_QUEUE_CAPACITY: usize = 1000;

/// WebSocket 心跳默认间隔（秒）
pub const DEFAULT_WS_HEARTBEAT_INTERVAL: u64 = 30;

/// 日志文件默认最大大小（MB）
pub const DEFAULT_LOG_MAX_SIZE: u64 = 50;

/// 日志文件默认最大数量
pub const DEFAULT_LOG_MAX_FILES: u64 = 5;
