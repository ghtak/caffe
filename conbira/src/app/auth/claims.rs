use serde_json::{Map, Value};

use crate::app::auth::entity::UserStatus;

/// 요청에 포함된 액세스 토큰의 검증된 클레임.
pub struct AuthClaims(pub espr::jwt::Claims);

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct UserClaims {
    pub id: i64,
    pub status: UserStatus,
}

impl UserClaims {
    pub fn new(id: i64, status: UserStatus) -> Self { Self { id, status } }
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
    use super::*;

    #[test]
    fn claims_convert_to_json_map_and_deserialize_for_each_user_status() -> anyhow::Result<()> {
        for (status, name) in [
            (UserStatus::Registered, "registered"),
            (UserStatus::Active, "active"),
            (UserStatus::Inactive, "inactive"),
            (UserStatus::Withdrawn, "withdrawn"),
        ] {
            let map: Map<String, Value> = UserClaims { id: 42, status }.try_into()?;
            // 토큰에 사용자 ID와 상태 문자열만 포함된다.
            assert_eq!(
                Value::Object(map.clone()),
                serde_json::json!({"id": 42, "status": name})
            );
            let decoded: UserClaims = serde_json::from_value(Value::Object(map))?;
            // 저장된 claim을 동일한 사용자 ID와 상태로 읽는다.
            assert_eq!(decoded.id, 42);
            assert_eq!(decoded.status, status);
        }
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
