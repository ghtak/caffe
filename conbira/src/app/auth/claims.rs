use serde_json::{Map, Value};

use crate::app::auth::entity::UserStatus;

/// 요청에 포함된 액세스 토큰의 검증된 클레임.
pub struct AuthClaims(pub espr::jwt::Claims);

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct UserClaims {
    pub id: i64,
    pub status: UserStatus,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

impl UserClaims {
    pub fn new(id: i64, status: UserStatus, roles: Vec<String>, permissions: Vec<String>) -> Self {
        Self {
            id,
            status,
            roles,
            permissions,
        }
    }

    pub fn has_role(&self, code: &str) -> bool { self.roles.iter().any(|role| role == code) }

    pub fn has_permission(&self, code: &str) -> bool {
        self.permissions.iter().any(|permission| permission == code)
    }

    pub fn require_permission(&self, code: &str, at: &'static str) -> crate::Result<()> {
        if self.has_permission(code) {
            Ok(())
        } else {
            Err(crate::Error::new(crate::Code::Forbidden, "Permission denied").at(at))
        }
    }

    pub fn require_permissions(&self, codes: &[&str], at: &'static str) -> crate::Result<()> {
        if self.has_permissions(codes) {
            Ok(())
        } else {
            Err(crate::Error::new(crate::Code::Forbidden, "Permission denied").at(at))
        }
    }

    pub fn require_any_permission(&self, codes: &[&str], at: &'static str) -> crate::Result<()> {
        if self.has_any_permission(codes) {
            Ok(())
        } else {
            Err(crate::Error::new(crate::Code::Forbidden, "Permission denied").at(at))
        }
    }

    /// 빈 목록은 false를 반환한다.
    pub fn has_any_permission(&self, codes: &[&str]) -> bool {
        codes.iter().any(|code| self.has_permission(code))
    }

    /// 빈 목록은 true를 반환한다.
    pub fn has_permissions(&self, codes: &[&str]) -> bool {
        codes.iter().all(|code| self.has_permission(code))
    }
}

impl TryFrom<AuthClaims> for UserClaims {
    type Error = crate::Error;

    fn try_from(AuthClaims(claims): AuthClaims) -> crate::Result<Self> {
        let user: Self = serde_json::from_value(Value::Object(claims.user)).map_err(|error| {
            crate::Error::new(crate::Code::Unauthorized, "Invalid user claims")
                .at(crate::here!())
                .cause(error)
        })?;
        if claims.sub != user.id.to_string() {
            return Err(
                crate::Error::new(crate::Code::Unauthorized, "Invalid user claims")
                    .at(crate::here!()),
            );
        }
        Ok(user)
    }
}

impl TryFrom<UserClaims> for Map<String, Value> {
    type Error = crate::Error;

    fn try_from(claims: UserClaims) -> crate::Result<Self> {
        use crate::error::ResultExt as _;

        let value = serde_json::to_value(claims)
            .map_err_internal("Failed to serialize user claims", crate::here!())?;

        match value {
            Value::Object(map) => Ok(map),
            _ => Err(crate::Error::internal("User claims must be an object").at(crate::here!())),
        }
    }
}

#[cfg(test)]
mod tests {
    mod contract {
        use super::super::*;

        #[test]
        fn require_permission_returns_forbidden_with_caller_location() {
            let claims = UserClaims::new(42, UserStatus::Active, vec![], vec!["rbac.read".into()]);
            // 보유한 권한은 통과하고, 없는 권한은 호출 위치를 보존해 거부한다.
            assert!(claims.require_permission("rbac.read", "caller").is_ok());
            let error = claims
                .require_permission("rbac.write", "caller")
                .unwrap_err();
            assert_eq!(error.code, crate::Code::Forbidden);
            assert_eq!(error.at, Some("caller"));
        }

        #[test]
        fn claims_convert_to_json_map_and_deserialize_for_each_user_status() -> anyhow::Result<()> {
            for (status, name) in [
                (UserStatus::Registered, "registered"),
                (UserStatus::Active, "active"),
                (UserStatus::Inactive, "inactive"),
                (UserStatus::Withdrawn, "withdrawn"),
            ] {
                let map: Map<String, Value> =
                    UserClaims::new(42, status, vec!["user".into()], vec!["rbac.read".into()])
                        .try_into()?;
                // 토큰에 사용자 ID, 상태와 권한 코드가 포함된다.
                assert_eq!(
                    Value::Object(map.clone()),
                    serde_json::json!({"id": 42, "status": name, "roles": ["user"], "permissions": ["rbac.read"]})
                );
                let decoded: UserClaims = serde_json::from_value(Value::Object(map))?;
                // 저장된 claim을 동일한 사용자 정보와 권한으로 읽는다.
                assert_eq!(decoded.id, 42);
                assert_eq!(decoded.status, status);
                assert_eq!(decoded.permissions, ["rbac.read"]);
                assert!(decoded.has_role("user"));
                assert!(!decoded.has_role("admin"));
            }
            Ok(())
        }

        #[test]
        fn older_claims_without_permissions_have_no_permissions() -> anyhow::Result<()> {
            let claims: UserClaims =
                serde_json::from_value(serde_json::json!({"id": 42, "status": "active"}))?;
            assert!(claims.permissions.is_empty());
            assert!(!claims.has_role("user"));
            let claims: UserClaims = serde_json::from_value(
                serde_json::json!({"id": 42, "status": "active", "permissions": ["rbac.read"]}),
            )?;
            assert!(claims.roles.is_empty());
            assert!(claims.has_permission("rbac.read"));
            Ok(())
        }

        #[test]
        fn rejects_unknown_user_status_when_deserializing_claims() {
            // 정의되지 않은 상태는 유효한 사용자 claim으로 읽지 않는다.
            assert!(
                serde_json::from_value::<UserClaims>(
                    serde_json::json!({"id": 42, "status": "unknown"})
                )
                .is_err()
            );
        }
    }
}
