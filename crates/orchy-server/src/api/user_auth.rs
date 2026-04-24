use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use tower_cookies::Cookies;

use orchy_application::{
    AuthResponse, ChangePasswordCommand, GetCurrentUserCommand, InviteUserCommand,
    LoginUserCommand, RegisterUserCommand,
};

use crate::auth::{CookieConfig, clear_auth_cookie, set_auth_cookie};
use crate::container::Container;

use super::ApiError;

#[derive(Deserialize)]
pub struct RegisterRequest {
    email: String,
    password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Serialize)]
pub struct AuthSuccessResponse {
    user: orchy_application::UserDto,
    memberships: Vec<orchy_application::OrgMembershipDto>,
}

impl From<AuthResponse> for AuthSuccessResponse {
    fn from(r: AuthResponse) -> Self {
        Self {
            user: r.user,
            memberships: r.memberships,
        }
    }
}

pub async fn register(
    State(container): State<Arc<Container>>,
    Json(req): Json<RegisterRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let result = container
        .app
        .register_user
        .execute(
            RegisterUserCommand {
                email: req.email,
                password: req.password,
            },
            container.password_hasher.as_ref(),
        )
        .await
        .map_err(ApiError::from)?;

    Ok((StatusCode::CREATED, Json(result.user)))
}

pub async fn login(
    State(container): State<Arc<Container>>,
    cookies: Cookies,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let login_user = container.app.login_user.as_ref().ok_or_else(|| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "authentication not configured".to_string(),
        )
    })?;

    let result = login_user
        .execute(
            LoginUserCommand {
                email: req.email,
                password: req.password,
            },
            container.password_hasher.as_ref(),
        )
        .await
        .map_err(ApiError::from)?;

    let cookie_config = CookieConfig {
        secure: container.config.auth.cookie_secure,
        same_site: tower_cookies::cookie::SameSite::Lax,
        max_age_hours: container.config.auth.jwt_duration_hours,
    };

    set_auth_cookie(&cookies, &result.token, &cookie_config);

    Ok((
        StatusCode::OK,
        Json(AuthSuccessResponse {
            user: result.user,
            memberships: result.memberships,
        }),
    ))
}

pub async fn logout(_state: State<Arc<Container>>, cookies: Cookies) -> impl IntoResponse {
    clear_auth_cookie(&cookies);
    StatusCode::NO_CONTENT
}

pub async fn me(
    State(container): State<Arc<Container>>,
    cookies: Cookies,
) -> Result<impl IntoResponse, ApiError> {
    let auth = super::middleware::cookie_auth::extract_user_auth(&cookies, &container)
        .await
        .ok_or_else(|| {
            ApiError(
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "invalid or missing token".to_string(),
            )
        })?;

    let result = container
        .app
        .get_current_user
        .execute(GetCurrentUserCommand {
            user_id: auth.user_id.to_string(),
        })
        .await
        .map_err(ApiError::from)?;

    Ok(Json(AuthSuccessResponse::from(result)))
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    old_password: String,
    new_password: String,
}

pub async fn change_password(
    State(container): State<Arc<Container>>,
    cookies: Cookies,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let auth = super::middleware::cookie_auth::extract_user_auth(&cookies, &container)
        .await
        .ok_or_else(|| {
            ApiError(
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "invalid or missing token".to_string(),
            )
        })?;

    let result = container
        .app
        .change_password
        .execute(
            ChangePasswordCommand {
                user_id: auth.user_id.to_string(),
                old_password: req.old_password,
                new_password: req.new_password,
            },
            container.password_hasher.as_ref(),
        )
        .await
        .map_err(ApiError::from)?;

    Ok(Json(result))
}

#[derive(Deserialize)]
pub struct InviteRequest {
    email: String,
    role: String,
}

#[derive(Serialize)]
pub struct InviteResponse {
    user: orchy_application::UserDto,
    membership: orchy_application::OrgMembershipDto,
    is_new_user: bool,
}

pub async fn invite_user(
    State(container): State<Arc<Container>>,
    cookies: Cookies,
    axum::extract::Path(org_id): axum::extract::Path<String>,
    Json(req): Json<InviteRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let auth = super::middleware::cookie_auth::extract_user_auth(&cookies, &container)
        .await
        .ok_or_else(|| {
            ApiError(
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "invalid or missing token".to_string(),
            )
        })?;

    let org_id = orchy_core::organization::OrganizationId::new(&org_id)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, "INVALID_ORG", e.to_string()))?;

    let _membership = auth
        .get_org_role(&org_id)
        .filter(|r| r.can_invite())
        .ok_or_else(|| {
            ApiError(
                StatusCode::FORBIDDEN,
                "FORBIDDEN",
                "you don't have permission to invite users".to_string(),
            )
        })?;

    let result = container
        .app
        .invite_user
        .execute(
            InviteUserCommand {
                email: req.email,
                org_id: org_id.to_string(),
                role: req.role,
                invited_by_user_id: auth.user_id.to_string(),
            },
            container.password_hasher.as_ref(),
        )
        .await
        .map_err(ApiError::from)?;

    Ok((
        StatusCode::CREATED,
        Json(InviteResponse {
            user: result.user,
            membership: result.membership,
            is_new_user: result.is_new_user,
        }),
    ))
}
