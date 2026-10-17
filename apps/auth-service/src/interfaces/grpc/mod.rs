use crate::application::commands::{
    ChangePassword, DeleteUser, DisableUser, Login, Logout, LogoutAll, ManageMfa,
    RefreshAccessToken, RegisterUser, RequestEmailVerification, RequestPasswordReset,
    ResetPassword, UnlockAccount, UpdateProfile, VerifyEmail,
};
use crate::application::ports::{ActiveSession, SessionStore};
use crate::application::queries::{GetUser, GetUserByEmail, ListUsers, ValidateAccessToken};
use crate::domain::entities::User as DomainUser;
use crate::domain::errors::AuthError;
use common::ServiceJwtClaims;
use proto::auth_v1::auth_service_server::AuthService;
use proto::auth_v1::{
    ChangePasswordRequest, ChangePasswordResponse, ConfirmMfaRequest, ConfirmMfaResponse,
    DeleteUserRequest, DeleteUserResponse, DisableMfaRequest, DisableMfaResponse,
    DisableUserRequest, DisableUserResponse, EnableMfaRequest, EnableMfaResponse,
    GetUserByEmailRequest, GetUserByEmailResponse, GetUserRequest, GetUserResponse,
    HealthCheckRequest, HealthCheckResponse, ListSessionsRequest, ListSessionsResponse,
    ListUsersRequest, ListUsersResponse, LoginRequest, LoginResponse, LogoutAllRequest,
    LogoutAllResponse, LogoutRequest, LogoutResponse, RefreshRequest, RefreshResponse,
    RegisterRequest, RegisterResponse, RequestEmailVerificationRequest,
    RequestEmailVerificationResponse, RequestPasswordResetRequest, RequestPasswordResetResponse,
    ResetPasswordRequest, ResetPasswordResponse, RevokeSessionRequest, RevokeSessionResponse,
    UnlockAccountRequest, UnlockAccountResponse, UpdateProfileRequest, UpdateProfileResponse, User,
    ValidateAccessTokenRequest, ValidateAccessTokenResponse, VerifyEmailRequest,
    VerifyEmailResponse,
};
use std::collections::HashSet;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use uuid::Uuid;

pub struct AuthGrpcService {
    pub register: RegisterUser,
    pub verify_email: VerifyEmail,
    pub request_email_verification: RequestEmailVerification,
    pub login: Login,
    pub refresh: RefreshAccessToken,
    pub logout: Logout,
    pub logout_all: LogoutAll,
    pub request_reset: RequestPasswordReset,
    pub reset_password: ResetPassword,
    pub change_password: ChangePassword,
    pub manage_mfa: ManageMfa,
    pub update_profile: UpdateProfile,
    pub unlock_account: UnlockAccount,
    pub disable_user: DisableUser,
    pub delete_user: DeleteUser,
    pub get_user: GetUser,
    pub get_user_by_email: GetUserByEmail,
    pub validate_access_token: ValidateAccessToken,
    pub sessions: Arc<dyn SessionStore>,
    pub list_users: ListUsers,
    pub admin_callers: HashSet<String>,
}

fn require_admin_caller<T>(
    request: &Request<T>,
    admin_callers: &HashSet<String>,
) -> Result<(), Status> {
    let claims = request
        .extensions()
        .get::<ServiceJwtClaims>()
        .ok_or_else(|| Status::unauthenticated("missing service claims"))?;
    if admin_callers.contains(&claims.sub) {
        Ok(())
    } else {
        Err(Status::permission_denied(
            "caller is not allowed for admin operations",
        ))
    }
}

fn map_error(error: AuthError) -> Status {
    match error {
        AuthError::UserNotFound => Status::not_found("user not found"),
        AuthError::EmailAlreadyExists(email) => Status::already_exists(email),
        AuthError::InvalidCredentials => Status::unauthenticated("invalid credentials"),
        AuthError::AccountLocked(until) => Status::failed_precondition(until),
        AuthError::AccountDisabled => Status::failed_precondition("account disabled"),
        AuthError::EmailNotVerified => Status::failed_precondition("email not verified"),
        AuthError::InvalidEmailVerificationToken => {
            Status::unauthenticated("invalid email verification token")
        }
        AuthError::EmailVerificationExpired => {
            Status::failed_precondition("email verification token expired")
        }
        AuthError::EmailAlreadyVerified => Status::failed_precondition("email already verified"),
        AuthError::RateLimited(message) => Status::resource_exhausted(message),
        AuthError::DependencyUnavailable(message) => Status::unavailable(message),
        AuthError::InvalidRefreshToken => Status::unauthenticated("invalid refresh token"),
        AuthError::RefreshTokenRevoked => Status::failed_precondition("refresh token revoked"),
        AuthError::PasswordResetExpired => Status::failed_precondition("password reset expired"),
        AuthError::PasswordResetUsed => Status::failed_precondition("password reset already used"),
        AuthError::MfaRequired => Status::failed_precondition("mfa required"),
        AuthError::InvalidMfaCode => Status::unauthenticated("invalid mfa code"),
        AuthError::MfaAlreadyEnabled => Status::failed_precondition("mfa already enabled"),
        AuthError::MfaNotEnabled => Status::failed_precondition("mfa not enabled"),
        AuthError::PasswordTooWeak(msg) | AuthError::Validation(msg) => {
            Status::invalid_argument(msg)
        }
    }
}

fn parse_uuid(field: &str, raw: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn require_non_empty(field: &str, raw: String) -> Result<String, Status> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(Status::invalid_argument(format!("{field} is required")));
    }
    Ok(value.to_string())
}

fn clean_optional(raw: Option<String>) -> Option<String> {
    raw.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_email(email: &str) -> Result<String, Status> {
    let normalized = email.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(Status::invalid_argument("email is required"));
    }

    let parts: Vec<&str> = normalized.split('@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(Status::invalid_argument("invalid email"));
    }

    Ok(normalized)
}

fn to_proto_user(user: &DomainUser) -> User {
    User {
        id: user.id.to_string(),
        email: user.email.clone(),
        display_name: user.display_name.clone(),
        avatar_url: user.avatar_url.clone(),
        mfa_enabled: user.mfa_enabled,
        status: user.status.clone(),
        role: user.role.clone(),
    }
}

fn to_proto_session(session: ActiveSession) -> proto::auth_v1::ActiveSession {
    proto::auth_v1::ActiveSession {
        session_id: session.session_id.to_string(),
        user_agent: session.user_agent.unwrap_or_default(),
        ip_address: session.ip_address.unwrap_or_default(),
        created_at: session.created_at,
        last_seen_at: session.last_seen_at,
    }
}

#[tonic::async_trait]
impl AuthService for AuthGrpcService {
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        match self
            .get_user_by_email
            .execute("healthcheck@internal.invalid")
            .await
        {
            Ok(_) => Ok(Response::new(HealthCheckResponse {
                ok: true,
                status: "ok".to_string(),
            })),
            Err(error) => Ok(Response::new(HealthCheckResponse {
                ok: false,
                status: format!("degraded: {}", error),
            })),
        }
    }

    async fn register(
        &self,
        request: Request<RegisterRequest>,
    ) -> Result<Response<RegisterResponse>, Status> {
        let req = request.into_inner();
        let email = normalize_email(&req.email)?;
        let password = require_non_empty("password", req.password)?;
        let display_name = clean_optional(req.display_name);

        let user = self
            .register
            .execute(email, password, display_name)
            .await
            .map_err(map_error)?;

        Ok(Response::new(RegisterResponse {
            user: Some(to_proto_user(&user)),
        }))
    }

    async fn verify_email(
        &self,
        request: Request<VerifyEmailRequest>,
    ) -> Result<Response<VerifyEmailResponse>, Status> {
        let req = request.into_inner();
        let token = require_non_empty("token", req.token)?;

        self.verify_email.execute(&token).await.map_err(map_error)?;

        Ok(Response::new(VerifyEmailResponse { success: true }))
    }

    async fn request_email_verification(
        &self,
        request: Request<RequestEmailVerificationRequest>,
    ) -> Result<Response<RequestEmailVerificationResponse>, Status> {
        let req = request.into_inner();
        let email = normalize_email(&req.email)?;
        self.request_email_verification
            .execute(&email)
            .await
            .map_err(map_error)?;

        Ok(Response::new(RequestEmailVerificationResponse {
            success: true,
        }))
    }

    async fn login(
        &self,
        request: Request<LoginRequest>,
    ) -> Result<Response<LoginResponse>, Status> {
        let req = request.into_inner();
        let email = normalize_email(&req.email)?;
        let password = require_non_empty("password", req.password)?;
        let mfa_code = clean_optional(req.mfa_code);

        let (access_token, refresh_token) = self
            .login
            .execute(
                &email,
                &password,
                mfa_code.as_deref(),
                clean_optional(req.client_ip),
                clean_optional(req.user_agent),
            )
            .await
            .map_err(map_error)?;

        Ok(Response::new(LoginResponse {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
        }))
    }

    async fn refresh(
        &self,
        request: Request<RefreshRequest>,
    ) -> Result<Response<RefreshResponse>, Status> {
        let req = request.into_inner();
        let refresh_token = require_non_empty("refresh_token", req.refresh_token)?;

        let (access_token, rotated_refresh_token) = self
            .refresh
            .execute(&refresh_token)
            .await
            .map_err(map_error)?;

        Ok(Response::new(RefreshResponse {
            access_token,
            refresh_token: rotated_refresh_token,
            token_type: "Bearer".to_string(),
        }))
    }

    async fn logout(
        &self,
        request: Request<LogoutRequest>,
    ) -> Result<Response<LogoutResponse>, Status> {
        let req = request.into_inner();
        let refresh_token = require_non_empty("refresh_token", req.refresh_token)?;

        self.logout
            .execute(&refresh_token)
            .await
            .map_err(map_error)?;

        Ok(Response::new(LogoutResponse { success: true }))
    }

    async fn logout_all(
        &self,
        request: Request<LogoutAllRequest>,
    ) -> Result<Response<LogoutAllResponse>, Status> {
        let user_id = parse_uuid("user_id", &request.into_inner().user_id)?;
        self.logout_all.execute(user_id).await.map_err(map_error)?;
        Ok(Response::new(LogoutAllResponse { success: true }))
    }

    async fn list_sessions(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let user_id = parse_uuid("user_id", &request.into_inner().user_id)?;
        let sessions = self
            .sessions
            .list_for_user(user_id)
            .await
            .map_err(map_error)?;
        Ok(Response::new(ListSessionsResponse {
            sessions: sessions.into_iter().map(to_proto_session).collect(),
        }))
    }

    async fn revoke_session(
        &self,
        request: Request<RevokeSessionRequest>,
    ) -> Result<Response<RevokeSessionResponse>, Status> {
        let request = request.into_inner();
        let user_id = parse_uuid("user_id", &request.user_id)?;
        let session_id = parse_uuid("session_id", &request.session_id)?;
        let revoked = self
            .sessions
            .revoke_for_user(user_id, session_id)
            .await
            .map_err(map_error)?;
        if !revoked {
            return Err(Status::not_found("session not found"));
        }
        Ok(Response::new(RevokeSessionResponse { success: true }))
    }

    async fn request_password_reset(
        &self,
        request: Request<RequestPasswordResetRequest>,
    ) -> Result<Response<RequestPasswordResetResponse>, Status> {
        let req = request.into_inner();
        let email = normalize_email(&req.email)?;
        self.request_reset
            .execute(&email)
            .await
            .map_err(map_error)?;

        Ok(Response::new(RequestPasswordResetResponse {
            success: true,
        }))
    }

    async fn reset_password(
        &self,
        request: Request<ResetPasswordRequest>,
    ) -> Result<Response<ResetPasswordResponse>, Status> {
        let req = request.into_inner();
        let token = require_non_empty("token", req.token)?;
        let new_password = require_non_empty("new_password", req.new_password)?;
        self.reset_password
            .execute(&token, &new_password)
            .await
            .map_err(map_error)?;

        Ok(Response::new(ResetPasswordResponse { success: true }))
    }

    async fn change_password(
        &self,
        request: Request<ChangePasswordRequest>,
    ) -> Result<Response<ChangePasswordResponse>, Status> {
        let req = request.into_inner();
        let user_id = parse_uuid("user_id", &req.user_id)?;
        let old_password = require_non_empty("old_password", req.old_password)?;
        let new_password = require_non_empty("new_password", req.new_password)?;
        self.change_password
            .execute(user_id, &old_password, &new_password)
            .await
            .map_err(map_error)?;

        Ok(Response::new(ChangePasswordResponse { success: true }))
    }

    async fn enable_mfa(
        &self,
        request: Request<EnableMfaRequest>,
    ) -> Result<Response<EnableMfaResponse>, Status> {
        let req = request.into_inner();
        let user_id = parse_uuid("user_id", &req.user_id)?;
        let account_name = req
            .account_name
            .ok_or_else(|| Status::invalid_argument("account_name is required"))?;
        let account_name = require_non_empty("account_name", account_name)?;

        let otpauth_uri = self
            .manage_mfa
            .enable(user_id, account_name)
            .await
            .map_err(map_error)?;

        Ok(Response::new(EnableMfaResponse { otpauth_uri }))
    }

    async fn confirm_mfa(
        &self,
        request: Request<ConfirmMfaRequest>,
    ) -> Result<Response<ConfirmMfaResponse>, Status> {
        let req = request.into_inner();
        let user_id = parse_uuid("user_id", &req.user_id)?;
        let code = require_non_empty("code", req.code)?;

        self.manage_mfa
            .confirm(user_id, &code)
            .await
            .map_err(map_error)?;

        Ok(Response::new(ConfirmMfaResponse { success: true }))
    }

    async fn disable_mfa(
        &self,
        request: Request<DisableMfaRequest>,
    ) -> Result<Response<DisableMfaResponse>, Status> {
        let req = request.into_inner();
        let user_id = parse_uuid("user_id", &req.user_id)?;
        let code = require_non_empty("code", req.code)?;

        self.manage_mfa
            .disable(user_id, &code)
            .await
            .map_err(map_error)?;

        Ok(Response::new(DisableMfaResponse { success: true }))
    }

    async fn get_user(
        &self,
        request: Request<GetUserRequest>,
    ) -> Result<Response<GetUserResponse>, Status> {
        let req = request.into_inner();
        let user_id = parse_uuid("user_id", &req.user_id)?;

        let user = self
            .get_user
            .execute(user_id)
            .await
            .map_err(map_error)?
            .ok_or_else(|| Status::not_found("user not found"))?;

        Ok(Response::new(GetUserResponse {
            user: Some(to_proto_user(&user)),
        }))
    }

    async fn get_user_by_email(
        &self,
        request: Request<GetUserByEmailRequest>,
    ) -> Result<Response<GetUserByEmailResponse>, Status> {
        let req = request.into_inner();
        let email = normalize_email(&req.email)?;

        let user = self
            .get_user_by_email
            .execute(&email)
            .await
            .map_err(map_error)?
            .ok_or_else(|| Status::not_found("user not found"))?;

        Ok(Response::new(GetUserByEmailResponse {
            user: Some(to_proto_user(&user)),
        }))
    }

    async fn validate_access_token(
        &self,
        request: Request<ValidateAccessTokenRequest>,
    ) -> Result<Response<ValidateAccessTokenResponse>, Status> {
        let access_token = require_non_empty("access_token", request.into_inner().access_token)?;
        let user = self
            .validate_access_token
            .execute(&access_token)
            .await
            .map_err(map_error)?;
        Ok(Response::new(ValidateAccessTokenResponse {
            user: Some(to_proto_user(&user)),
        }))
    }

    async fn update_profile(
        &self,
        request: Request<UpdateProfileRequest>,
    ) -> Result<Response<UpdateProfileResponse>, Status> {
        let req = request.into_inner();
        let user_id = parse_uuid("user_id", &req.user_id)?;
        let user = self
            .update_profile
            .execute(
                user_id,
                clean_optional(req.display_name),
                clean_optional(req.avatar_url),
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(UpdateProfileResponse {
            user: Some(to_proto_user(&user)),
        }))
    }

    async fn list_users(
        &self,
        request: Request<ListUsersRequest>,
    ) -> Result<Response<ListUsersResponse>, Status> {
        require_admin_caller(&request, &self.admin_callers)?;
        let req = request.into_inner();
        let limit = if req.limit == 0 { 20 } else { req.limit };
        let (users, next_cursor) = self
            .list_users
            .execute(None, limit, req.after_cursor)
            .await
            .map_err(map_error)?;
        Ok(Response::new(ListUsersResponse {
            users: users.iter().map(to_proto_user).collect(),
            next_cursor: next_cursor.unwrap_or_default(),
        }))
    }

    async fn unlock_account(
        &self,
        request: Request<UnlockAccountRequest>,
    ) -> Result<Response<UnlockAccountResponse>, Status> {
        require_admin_caller(&request, &self.admin_callers)?;
        let req = request.into_inner();
        let user_id = parse_uuid("user_id", &req.user_id)?;
        let admin_actor_id = parse_uuid("admin_actor_id", &req.admin_actor_id)?;
        self.unlock_account
            .execute(user_id, admin_actor_id)
            .await
            .map_err(map_error)?;
        Ok(Response::new(UnlockAccountResponse { success: true }))
    }

    async fn disable_user(
        &self,
        request: Request<DisableUserRequest>,
    ) -> Result<Response<DisableUserResponse>, Status> {
        require_admin_caller(&request, &self.admin_callers)?;
        let req = request.into_inner();
        let user_id = parse_uuid("user_id", &req.user_id)?;
        let admin_actor_id = parse_uuid("admin_actor_id", &req.admin_actor_id)?;
        self.disable_user
            .execute(user_id, admin_actor_id)
            .await
            .map_err(map_error)?;
        Ok(Response::new(DisableUserResponse { success: true }))
    }

    async fn delete_user(
        &self,
        request: Request<DeleteUserRequest>,
    ) -> Result<Response<DeleteUserResponse>, Status> {
        require_admin_caller(&request, &self.admin_callers)?;
        let req = request.into_inner();
        let user_id = parse_uuid("user_id", &req.user_id)?;
        let admin_actor_id = parse_uuid("admin_actor_id", &req.admin_actor_id)?;
        self.delete_user
            .execute(user_id, admin_actor_id)
            .await
            .map_err(map_error)?;
        Ok(Response::new(DeleteUserResponse { success: true }))
    }
}
