//! 安全护栏 — 权限声明 + 运行时检查

use std::collections::HashSet;

use crate::error::{FlowError, FlowResult};
use crate::types::{Permission, ScriptManifest};

/// 权限检查器
pub struct PermissionGuard {
    /// 已授权的权限
    granted: HashSet<PermissionKey>,
}

/// 权限键（用于 Hash）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PermissionKey {
    Capture,
    Input,
    Window,
    FileRead,
    FileWrite,
    Network,
    Storage,
    CallScript,
    StateMachine,
    Trigger,
    Notify,
    SystemCommand,
}

impl PermissionGuard {
    /// 创建新的权限检查器
    pub fn new(manifest: &ScriptManifest, source: &str) -> Self {
        let mut granted = HashSet::new();

        // system 源拥有全部权限
        if source == "system" {
            granted.insert(PermissionKey::Capture);
            granted.insert(PermissionKey::Input);
            granted.insert(PermissionKey::Window);
            granted.insert(PermissionKey::FileRead);
            granted.insert(PermissionKey::FileWrite);
            granted.insert(PermissionKey::Network);
            granted.insert(PermissionKey::Storage);
            granted.insert(PermissionKey::CallScript);
            granted.insert(PermissionKey::StateMachine);
            granted.insert(PermissionKey::Trigger);
            granted.insert(PermissionKey::Notify);
            granted.insert(PermissionKey::SystemCommand);
            return Self { granted };
        }

        // 授权 required + optional
        for perm in manifest
            .permissions
            .required
            .iter()
            .chain(manifest.permissions.optional.iter())
        {
            granted.insert(Self::to_key(perm));
        }

        Self { granted }
    }

    /// 检查权限
    pub fn check(&self, required: &Permission) -> FlowResult<()> {
        let key = Self::to_key(required);
        if self.granted.contains(&key) {
            Ok(())
        } else {
            Err(FlowError::PermissionDenied(format!(
                "Permission {:?} not declared in manifest",
                required
            )))
        }
    }

    /// 检查权限（不报错，返回 bool）
    pub fn has(&self, required: &Permission) -> bool {
        let key = Self::to_key(required);
        self.granted.contains(&key)
    }

    /// 转换为内部键
    fn to_key(perm: &Permission) -> PermissionKey {
        match perm {
            Permission::Capture => PermissionKey::Capture,
            Permission::Input => PermissionKey::Input,
            Permission::Window => PermissionKey::Window,
            Permission::FileRead { .. } => PermissionKey::FileRead,
            Permission::FileWrite { .. } => PermissionKey::FileWrite,
            Permission::Network { .. } => PermissionKey::Network,
            Permission::Storage => PermissionKey::Storage,
            Permission::CallScript => PermissionKey::CallScript,
            Permission::StateMachine => PermissionKey::StateMachine,
            Permission::Trigger => PermissionKey::Trigger,
            Permission::Notify => PermissionKey::Notify,
            Permission::SystemCommand => PermissionKey::SystemCommand,
        }
    }
}

/// 预检：验证 Flow 中所有引用的脚本存在且权限声明有效
pub fn precheck_flow_permissions(
    flow: &crate::types::Flow,
    script_manifests: &std::collections::HashMap<String, ScriptManifest>,
    sources: &std::collections::HashMap<String, String>,
) -> FlowResult<()> {
    for (step_id, step) in &flow.steps {
        if let crate::types::StepKind::Script { script } = &step.kind {
            let manifest = script_manifests.get(script);
            let source = sources.get(script).map(|s| s.as_str()).unwrap_or("user");

            match manifest {
                Some(m) => {
                    // system 源跳过预检
                    if source == "system" {
                        continue;
                    }
                    // 预检：required 权限必须被声明
                    for perm in &m.permissions.required {
                        let guard = PermissionGuard::new(m, source);
                        if !guard.has(perm) {
                            return Err(FlowError::PermissionDenied(format!(
                                "Step '{}' references script '{}' which requires {:?} but it's not granted",
                                step_id, script, perm
                            )));
                        }
                    }
                }
                None => {
                    return Err(FlowError::StepNotFound(format!(
                        "Step '{}' references unknown script '{}'",
                        step_id, script
                    )));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Permissions;

    fn make_manifest(required: Vec<Permission>, optional: Vec<Permission>) -> ScriptManifest {
        ScriptManifest {
            schema_version: 2,
            uuid: None,
            source: None,
            name: "test".to_string(),
            display_name: "Test".to_string(),
            version: "1.0.0".to_string(),
            script_type: crate::types::ScriptType::Task,
            entry: "main.js".to_string(),
            author: String::new(),
            description: String::new(),
            dependencies: vec![],
            permissions: Permissions { required, optional },
            params_schema: None,
            output_schema: None,
            tags: vec![],
        }
    }

    #[test]
    fn test_has_required_permission() {
        let manifest = make_manifest(vec![Permission::Capture], vec![]);
        let guard = PermissionGuard::new(&manifest, "user");

        assert!(guard.has(&Permission::Capture));
        assert!(!guard.has(&Permission::Input));
    }

    #[test]
    fn test_optional_permission() {
        let manifest = make_manifest(vec![], vec![Permission::Notify]);
        let guard = PermissionGuard::new(&manifest, "user");

        assert!(guard.has(&Permission::Notify));
    }

    #[test]
    fn test_system_source_has_all() {
        let manifest = make_manifest(vec![], vec![]);
        let guard = PermissionGuard::new(&manifest, "system");

        assert!(guard.has(&Permission::Capture));
        assert!(guard.has(&Permission::Input));
        assert!(guard.has(&Permission::SystemCommand));
    }

    #[test]
    fn test_permission_denied() {
        let manifest = make_manifest(vec![Permission::Capture], vec![]);
        let guard = PermissionGuard::new(&manifest, "user");

        let result = guard.check(&Permission::Input);
        assert!(result.is_err());
    }
}
