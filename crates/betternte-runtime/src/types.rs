//! Core data structures for Flow Engine

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Flow — 流程容器
// ============================================================================

/// Flow 定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    /// 唯一标识
    pub id: String,
    /// 显示名称
    pub name: String,
    /// 描述
    #[serde(default)]
    pub description: String,
    /// 版本
    #[serde(default = "default_version")]
    pub version: String,
    /// 入口 Step ID
    pub entry: String,
    /// 步骤定义
    pub steps: IndexMap<String, Step>,
    /// 共享变量声明
    #[serde(default)]
    pub variables: HashMap<String, VariableDef>,
    /// 标签
    #[serde(default)]
    pub tags: Vec<String>,
    /// 输出 schema
    #[serde(default)]
    pub output_schema: Option<serde_json::Value>,
    /// 编排元数据（兼容 task-group 高级配置）
    #[serde(default)]
    pub orchestration: Option<FlowOrchestration>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// Flow 编排元数据（用于承载 legacy task-group 的高级配置）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FlowOrchestration {
    /// 执行模式（如 sequential/random）
    #[serde(default)]
    pub mode: Option<String>,
    /// 重试次数
    #[serde(default)]
    pub retry_count: Option<u32>,
    /// 错误处理策略
    #[serde(default)]
    pub error_handling: Option<String>,
    /// 重试配置（保留原结构，避免破坏兼容）
    #[serde(default)]
    pub retry: Option<serde_json::Value>,
    /// 失败通知
    #[serde(default)]
    pub notify_on_failure: Option<bool>,
    /// 调度配置（保留原结构，避免破坏兼容）
    #[serde(default)]
    pub schedule: Option<serde_json::Value>,
    /// 重复策略
    #[serde(default)]
    pub repeat_strategy: Option<String>,
    /// 来源标记（例如 legacy_task_group）
    #[serde(default)]
    pub source: Option<String>,
}

// ============================================================================
// Step — 执行单元
// ============================================================================

/// 步骤定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    /// 步骤类型
    pub kind: StepKind,
    /// 输入映射 (参数名 -> 变量引用)
    #[serde(default)]
    pub input: HashMap<String, String>,
    /// 输出映射 (变量引用 -> 值引用)
    #[serde(default)]
    pub output: HashMap<String, String>,
    /// 转换列表
    #[serde(default)]
    pub transitions: Vec<Transition>,
    /// 超时（毫秒）
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// 最大重试次数
    #[serde(default)]
    pub max_retries: u32,
    /// 重试耗尽后的目标 Step
    #[serde(default)]
    pub on_error: Option<String>,
}

// ============================================================================
// StepKind — 步骤类型
// ============================================================================

/// 步骤类型枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepKind {
    /// JS 脚本节点
    Script { script: String },
    /// 点击
    Click { x: i32, y: i32 },
    /// 滑动
    Swipe {
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        duration_ms: u32,
    },
    /// 按键
    KeyPress { key: String },
    /// 等待
    Wait { ms: u64 },
    /// 嵌套子流程
    Flow { flow: String },
    /// 引用任务组
    Group { group: String },
    /// 设置变量
    SetVariable {
        key: String,
        value: serde_json::Value,
    },
    /// 无操作（纯识别节点）
    None,
}

impl StepKind {
    /// Returns the type name string for registry dispatch.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Script { .. } => "script",
            Self::Click { .. } => "click",
            Self::Swipe { .. } => "swipe",
            Self::KeyPress { .. } => "key_press",
            Self::Wait { .. } => "wait",
            Self::Flow { .. } => "flow",
            Self::Group { .. } => "group",
            Self::SetVariable { .. } => "set_variable",
            Self::None => "none",
        }
    }
}

// ============================================================================
// Transition — 步骤间转换
// ============================================================================

/// 转换定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    /// 目标 Step ID
    pub target: String,
    /// 转换条件
    pub condition: Condition,
    /// 优先级 (0-255, 越高越优先)
    #[serde(default)]
    pub priority: u8,
    /// 是否中断当前 Step（每帧检查）
    #[serde(default)]
    pub interrupt: bool,
}

// ============================================================================
// Condition — 条件
// ============================================================================

/// 条件枚举
#[derive(Debug, Clone)]
pub enum Condition {
    /// 总是满足
    Always,
    /// 模板匹配
    Template {
        template: String,
        threshold: f32,
        roi: Option<RegionDef>,
    },
    /// OCR 文字检测
    Ocr {
        expected: String,
        roi: Option<RegionDef>,
    },
    /// 颜色匹配
    Color {
        x: i32,
        y: i32,
        color: String,
        tolerance: u8,
    },
    /// 变量判断
    Variable {
        key: String,
        op: CompareOp,
        value: serde_json::Value,
    },
    /// 热键
    Hotkey { key: String },
    /// 脚本判断（返回 true/false）
    Script { script: String },
    /// And 组合
    And(Vec<Box<Condition>>),
    /// Or 组合
    Or(Vec<Box<Condition>>),
    /// Not 取反
    Not(Box<Condition>),
}

// Manual Serialize/Deserialize for Condition to avoid serde derive recursion overflow.
// The derive macro monomorphizes the full type graph (Flow→Step→Transition→Condition→Vec<Condition>→...),
// which exceeds the compiler's trait recursion limit. Manual impl breaks this chain.

impl serde::Serialize for Condition {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Convert to a serde_json::Value first, then serialize that.
        // This breaks the monomorphization chain because serde_json::to_value
        // and serializer.serialize each use independent code paths.
        let value = match self {
            Condition::Always => serde_json::json!({"type": "always"}),
            Condition::Template {
                template,
                threshold,
                roi,
            } => {
                let mut m = serde_json::json!({
                    "type": "template",
                    "template": template,
                    "threshold": threshold,
                });
                if let Some(r) = roi {
                    m["roi"] = serde_json::to_value(r).unwrap_or_default();
                }
                m
            }
            Condition::Ocr { expected, roi } => {
                let mut m = serde_json::json!({
                    "type": "ocr",
                    "expected": expected,
                });
                if let Some(r) = roi {
                    m["roi"] = serde_json::to_value(r).unwrap_or_default();
                }
                m
            }
            Condition::Color {
                x,
                y,
                color,
                tolerance,
            } => {
                serde_json::json!({
                    "type": "color",
                    "x": x, "y": y,
                    "color": color,
                    "tolerance": tolerance,
                })
            }
            Condition::Variable { key, op, value } => {
                serde_json::json!({
                    "type": "variable",
                    "key": key,
                    "op": op,
                    "value": value,
                })
            }
            Condition::Hotkey { key } => serde_json::json!({"type": "hotkey", "key": key}),
            Condition::Script { script } => serde_json::json!({"type": "script", "script": script}),
            Condition::And(conditions) => {
                let arr: Vec<&Condition> = conditions.iter().map(|c| c.as_ref()).collect();
                // Serialize each sub-condition to Value
                let vals: Vec<serde_json::Value> = arr
                    .iter()
                    .map(|c| serde_json::to_value(*c).unwrap_or_default())
                    .collect();
                serde_json::json!({"type": "and", "conditions": vals})
            }
            Condition::Or(conditions) => {
                let vals: Vec<serde_json::Value> = conditions
                    .iter()
                    .map(|c| serde_json::to_value(c.as_ref()).unwrap_or_default())
                    .collect();
                serde_json::json!({"type": "or", "conditions": vals})
            }
            Condition::Not(inner) => {
                let val = serde_json::to_value(inner.as_ref()).unwrap_or_default();
                serde_json::json!({"type": "not", "condition": val})
            }
        };

        value.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Condition {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let obj = value
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("expected object for Condition"))?;

        let cond_type = obj
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| serde::de::Error::missing_field("type"))?;

        // Helper macro: extract required field or return missing_field error
        macro_rules! require_field {
            ($field:expr) => {
                obj.get($field)
                    .filter(|v| !v.is_null())
                    .cloned()
                    .ok_or_else(|| serde::de::Error::missing_field($field))?
            };
        }

        match cond_type {
            "always" => Ok(Condition::Always),
            "template" => Ok(Condition::Template {
                template: serde_json::from_value(require_field!("template"))
                    .map_err(serde::de::Error::custom)?,
                threshold: obj
                    .get("threshold")
                    .filter(|v| !v.is_null())
                    .map(|v| serde_json::from_value(v.clone()))
                    .transpose()
                    .map_err(serde::de::Error::custom)?
                    .unwrap_or(0.8),
                roi: obj
                    .get("roi")
                    .filter(|v| !v.is_null())
                    .map(|v| serde_json::from_value(v.clone()))
                    .transpose()
                    .map_err(serde::de::Error::custom)?,
            }),
            "ocr" => Ok(Condition::Ocr {
                expected: serde_json::from_value(require_field!("expected"))
                    .map_err(serde::de::Error::custom)?,
                roi: obj
                    .get("roi")
                    .filter(|v| !v.is_null())
                    .map(|v| serde_json::from_value(v.clone()))
                    .transpose()
                    .map_err(serde::de::Error::custom)?,
            }),
            "color" => Ok(Condition::Color {
                x: serde_json::from_value(require_field!("x")).map_err(serde::de::Error::custom)?,
                y: serde_json::from_value(require_field!("y")).map_err(serde::de::Error::custom)?,
                color: serde_json::from_value(require_field!("color"))
                    .map_err(serde::de::Error::custom)?,
                tolerance: obj
                    .get("tolerance")
                    .filter(|v| !v.is_null())
                    .map(|v| serde_json::from_value(v.clone()))
                    .transpose()
                    .map_err(serde::de::Error::custom)?
                    .unwrap_or(30),
            }),
            "variable" => Ok(Condition::Variable {
                key: serde_json::from_value(require_field!("key"))
                    .map_err(serde::de::Error::custom)?,
                op: serde_json::from_value(require_field!("op"))
                    .map_err(serde::de::Error::custom)?,
                value: require_field!("value"),
            }),
            "hotkey" => Ok(Condition::Hotkey {
                key: serde_json::from_value(require_field!("key"))
                    .map_err(serde::de::Error::custom)?,
            }),
            "script" => Ok(Condition::Script {
                script: serde_json::from_value(require_field!("script"))
                    .map_err(serde::de::Error::custom)?,
            }),
            "and" => {
                let arr = obj
                    .get("conditions")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| serde::de::Error::missing_field("conditions"))?;
                let conditions: Vec<Box<Condition>> = arr
                    .iter()
                    .map(|v| serde_json::from_value(v.clone()).map(Box::new))
                    .collect::<Result<_, _>>()
                    .map_err(serde::de::Error::custom)?;
                Ok(Condition::And(conditions))
            }
            "or" => {
                let arr = obj
                    .get("conditions")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| serde::de::Error::missing_field("conditions"))?;
                let conditions: Vec<Box<Condition>> = arr
                    .iter()
                    .map(|v| serde_json::from_value(v.clone()).map(Box::new))
                    .collect::<Result<_, _>>()
                    .map_err(serde::de::Error::custom)?;
                Ok(Condition::Or(conditions))
            }
            "not" => {
                let inner = obj
                    .get("condition")
                    .ok_or_else(|| serde::de::Error::missing_field("condition"))?;
                let cond: Condition =
                    serde_json::from_value(inner.clone()).map_err(serde::de::Error::custom)?;
                Ok(Condition::Not(Box::new(cond)))
            }
            other => Err(serde::de::Error::unknown_field(
                other,
                &[
                    "always", "template", "ocr", "color", "variable", "hotkey", "script", "and",
                    "or", "not",
                ],
            )),
        }
    }
}

/// 比较运算符
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Gte,
    Lte,
    In,
    Contains,
}

// ============================================================================
// RegionDef — 区域定义
// ============================================================================

/// 区域定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionDef {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

// ============================================================================
// VariableDef — 变量声明
// ============================================================================

/// 变量声明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableDef {
    /// 值类型 ("integer", "string", "boolean", "object")
    pub value_type: String,
    /// 默认值
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    /// 是否持久化
    #[serde(default)]
    pub persist: bool,
    /// JSON Schema
    #[serde(default)]
    pub schema: Option<serde_json::Value>,
}

// ============================================================================
// Trigger — 触发器
// ============================================================================

/// 触发器定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    /// 名称
    pub name: String,
    /// 触发条件
    pub condition: Condition,
    /// 触发动作
    pub action: TriggerAction,
    /// 优先级
    pub priority: u8,
    /// 冷却时间（毫秒）
    #[serde(default)]
    pub cooldown_ms: u64,
    /// 作用范围
    pub scope: TriggerScope,
}

/// 触发动作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerAction {
    /// 启动新 Flow
    StartFlow { flow: String },
    /// 跳转到当前 Flow 的某个 Step
    JumpTo { step: String },
    /// 中断当前 Flow
    Interrupt,
    /// 停止整个 GameSession
    Stop,
}

/// 触发器作用范围
///
/// 支持两种格式：
/// - `"global"` → Global
/// - `{"flow": ["a", "b"]}` → Flow
/// - `{"tag": ["x", "y"]}` → Tag
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TriggerScope {
    /// 只在指定 Flow 生效
    Flow {
        #[serde(alias = "flow")]
        flows: Vec<String>,
    },
    /// 按标签匹配
    Tag {
        #[serde(alias = "tag")]
        tags: Vec<String>,
    },
    /// 全局生效（字符串 "global"）
    #[serde(deserialize_with = "deserialize_global")]
    Global,
}

fn deserialize_global<'de, D>(deserializer: D) -> Result<(), D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s == "global" {
        Ok(())
    } else {
        Err(serde::de::Error::custom(format!(
            "expected \"global\", got \"{}\"",
            s
        )))
    }
}

// ============================================================================
// Group — 任务组（语法糖）
// ============================================================================

/// 任务组定义（自动展开为线性 Flow）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    /// 唯一标识
    #[serde(default)]
    pub uuid: String,
    /// 名称
    pub name: String,
    /// 描述
    #[serde(default)]
    pub description: String,
    /// 执行模式
    #[serde(default = "default_sequential")]
    pub mode: String,
    /// 重试次数
    #[serde(default)]
    pub retry_count: u32,
    /// 步骤列表
    pub steps: Vec<GroupStep>,
    /// 错误处理策略
    #[serde(default)]
    pub error_handling: Option<String>,
    /// 重试配置
    #[serde(default)]
    pub retry: Option<serde_json::Value>,
    /// 失败时通知
    #[serde(default)]
    pub notify_on_failure: Option<bool>,
    /// 调度配置
    #[serde(default)]
    pub schedule: Option<serde_json::Value>,
    /// 重复策略
    #[serde(default)]
    pub repeat_strategy: Option<String>,
    /// Data subscription / plugin label (set when loading from disk).
    #[serde(default)]
    pub source: Option<String>,
}

fn default_sequential() -> String {
    "sequential".to_string()
}

/// 任务组步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupStep {
    /// 步骤类型
    #[serde(flatten)]
    pub kind: StepKind,
    /// 别名
    pub alias: String,
    /// 输入映射
    #[serde(default)]
    pub input: HashMap<String, String>,
    /// 超时
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// 重试次数
    #[serde(default)]
    pub max_retries: u32,
}

// ============================================================================
// ScriptManifest — 脚本清单
// ============================================================================

/// 脚本清单
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptManifest {
    /// 格式版本
    pub schema_version: u32,
    /// UUID
    #[serde(default)]
    pub uuid: Option<String>,
    /// 来源 (system/user/imported)
    #[serde(default)]
    pub source: Option<String>,
    /// 名称
    pub name: String,
    /// 显示名称
    pub display_name: String,
    /// 版本
    pub version: String,
    /// 类型
    #[serde(rename = "type")]
    pub script_type: ScriptType,
    /// 入口文件
    pub entry: String,
    /// 作者
    #[serde(default)]
    pub author: String,
    /// 描述
    #[serde(default)]
    pub description: String,
    /// 依赖
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// 权限
    #[serde(default)]
    pub permissions: Permissions,
    /// 参数 schema
    #[serde(default)]
    pub params_schema: Option<serde_json::Value>,
    /// 输出 schema
    #[serde(default)]
    pub output_schema: Option<serde_json::Value>,
    /// 标签
    #[serde(default)]
    pub tags: Vec<String>,
}

/// 脚本类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptType {
    Task,
    Trigger,
    Library,
}

/// 权限声明
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Permissions {
    /// 必需权限
    #[serde(default)]
    pub required: Vec<Permission>,
    /// 可选权限
    #[serde(default)]
    pub optional: Vec<Permission>,
}

/// 权限枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// 截图
    Capture,
    /// 输入模拟
    Input,
    /// 窗口管理
    Window,
    /// 文件读取
    FileRead { paths: Vec<String> },
    /// 文件写入
    FileWrite { paths: Vec<String> },
    /// 网络请求
    Network { domains: Vec<String> },
    /// 存储
    Storage,
    /// 调用其他脚本
    CallScript,
    /// 状态机操作
    StateMachine,
    /// 触发器管理
    Trigger,
    /// 通知
    Notify,
    /// 系统命令
    SystemCommand,
}

// ============================================================================
// TriggerDef — 触发器定义文件
// ============================================================================

/// 触发器定义文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerDef {
    pub triggers: Vec<Trigger>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flow_parse() {
        let json = r#"{
            "id": "test",
            "name": "Test Flow",
            "entry": "step1",
            "steps": {
                "step1": {
                    "kind": { "type": "wait", "ms": 100 },
                    "transitions": []
                }
            }
        }"#;

        let flow: Flow = serde_json::from_str(json).unwrap();
        assert_eq!(flow.id, "test");
        assert_eq!(flow.entry, "step1");
        assert!(flow.steps.contains_key("step1"));
        assert!(flow.orchestration.is_none());
    }

    #[test]
    fn test_flow_parse_with_orchestration() {
        let json = r#"{
            "id": "tg-flow",
            "name": "Task Group Flow",
            "entry": "step1",
            "orchestration": {
                "mode": "sequential",
                "retry_count": 2,
                "error_handling": "interrupt",
                "repeat_strategy": "skip",
                "source": "legacy_task_group"
            },
            "steps": {
                "step1": {
                    "kind": { "type": "wait", "ms": 100 },
                    "transitions": []
                }
            }
        }"#;

        let flow: Flow = serde_json::from_str(json).unwrap();
        let orchestration = flow.orchestration.expect("missing orchestration");
        assert_eq!(orchestration.mode.as_deref(), Some("sequential"));
        assert_eq!(orchestration.retry_count, Some(2));
        assert_eq!(orchestration.source.as_deref(), Some("legacy_task_group"));
    }

    #[test]
    fn test_step_kind_script() {
        let json = r#"{"type": "script", "script": "test.js"}"#;
        let kind: StepKind = serde_json::from_str(json).unwrap();
        assert!(matches!(kind, StepKind::Script { .. }));
    }

    #[test]
    fn test_step_kind_click() {
        let json = r#"{"type": "click", "x": 100, "y": 200}"#;
        let kind: StepKind = serde_json::from_str(json).unwrap();
        assert!(matches!(kind, StepKind::Click { x: 100, y: 200 }));
    }

    #[test]
    fn test_condition_always() {
        let json = r#"{"type": "always"}"#;
        let cond: Condition = serde_json::from_str(json).unwrap();
        assert!(matches!(cond, Condition::Always));
    }

    #[test]
    fn test_condition_template() {
        let json = r#"{"type": "template", "template": "btn.png", "threshold": 0.9}"#;
        let cond: Condition = serde_json::from_str(json).unwrap();
        assert!(matches!(cond, Condition::Template { .. }));
    }

    #[test]
    fn test_condition_variable() {
        let json = r#"{"type": "variable", "key": "$variables.hp", "op": "gt", "value": 0}"#;
        let cond: Condition = serde_json::from_str(json).unwrap();
        assert!(matches!(cond, Condition::Variable { .. }));
    }

    #[test]
    fn test_transition_with_interrupt() {
        let json = r#"{
            "target": "next",
            "condition": { "type": "always" },
            "priority": 100,
            "interrupt": true
        }"#;
        let trans: Transition = serde_json::from_str(json).unwrap();
        assert!(trans.interrupt);
        assert_eq!(trans.priority, 100);
    }

    #[test]
    fn test_trigger_parse() {
        let json = r#"{
            "name": "stop",
            "condition": { "type": "hotkey", "key": "F12" },
            "action": { "type": "stop" },
            "priority": 255,
            "scope": "global"
        }"#;
        let trigger: Trigger = serde_json::from_str(json).unwrap();
        assert_eq!(trigger.name, "stop");
        assert_eq!(trigger.priority, 255);
    }

    #[test]
    fn test_group_parse() {
        let json = r#"{
            "name": "test",
            "steps": [
                { "type": "script", "script": "a", "alias": "step1" },
                { "type": "wait", "ms": 100, "alias": "step2" }
            ]
        }"#;
        let group: Group = serde_json::from_str(json).unwrap();
        assert_eq!(group.steps.len(), 2);
    }

    #[test]
    fn test_full_flow_with_variables() {
        let json = r#"{
            "id": "demo",
            "name": "Demo",
            "entry": "start",
            "variables": {
                "hp": {
                    "value_type": "integer",
                    "default": 100,
                    "persist": true
                }
            },
            "steps": {
                "start": {
                    "kind": { "type": "script", "script": "check" },
                    "input": { "current_hp": "$variables.hp" },
                    "output": { "$variables.hp": "$result.new_hp" },
                    "transitions": [
                        {
                            "target": "end",
                            "condition": {
                                "type": "variable",
                                "key": "$variables.hp",
                                "op": "lte",
                                "value": 0
                            }
                        }
                    ],
                    "max_retries": 3,
                    "on_error": "error_handler"
                },
                "error_handler": {
                    "kind": { "type": "click", "x": 100, "y": 100 },
                    "transitions": [
                        { "target": "start", "condition": { "type": "always" } }
                    ]
                },
                "end": {
                    "kind": { "type": "none" },
                    "transitions": []
                }
            }
        }"#;

        let flow: Flow = serde_json::from_str(json).unwrap();
        assert_eq!(flow.steps.len(), 3);
        assert!(flow.variables.contains_key("hp"));
    }
}
