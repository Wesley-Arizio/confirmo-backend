use std::sync::Arc;

use crate::application::core_service::CoreService;

use crate::graphql_api::model::GqlLawyer;
use actix_web::web;
use juniper::{Context, EmptySubscription, FieldResult, RootNode};

pub struct GraphqlContext {
    pub app_state: web::Data<AppState>,
    pub trace_id: String,
}
pub struct AppState {
    pub core_service: Arc<CoreService>,
}

impl AppState {
    pub fn new(core_service: Arc<CoreService>) -> Self {
        Self { core_service }
    }
}

impl Context for GraphqlContext {}

pub struct QueryRoot;

#[juniper::graphql_object(Context = GraphqlContext)]
impl QueryRoot {
    async fn lawyer(context: &GraphqlContext, email: String) -> FieldResult<GqlLawyer> {
        let result = context.app_state.core_service.lawyer(email).await?;

        Ok(result.into())
    }
}

pub struct MutationRoot;

#[juniper::graphql_object(Context = GraphqlContext)]
impl MutationRoot {
    async fn create_lawyer_account(
        context: &GraphqlContext,
        input: crate::graphql_api::input::CreateLawyerInput,
    ) -> FieldResult<GqlLawyer> {
        let result = context
            .app_state
            .core_service
            .create_lawyer_account(
                input.email,
                input.password,
                input.oab_number,
                input.name,
                input.firm_name,
            )
            .await?;

        Ok(result.into())
    }

    async fn verify_email(
        context: &GraphqlContext,
        input: crate::graphql_api::input::VerifyEmailInput,
    ) -> FieldResult<bool> {
        let result = context
            .app_state
            .core_service
            .verify_email(&input.email, &input.code)
            .await?;
        Ok(result)
    }

    async fn request_new_email_verifcation_code(
        context: &GraphqlContext,
        email: String,
    ) -> FieldResult<bool> {
        let result = context
            .app_state
            .core_service
            .request_new_email_verifcation_code(&email)
            .await?;
        Ok(result)
    }
}

pub type Schema = RootNode<QueryRoot, MutationRoot, EmptySubscription<GraphqlContext>>;

pub fn create_schema() -> Schema {
    Schema::new(
        QueryRoot {},
        MutationRoot {},
        EmptySubscription::<GraphqlContext>::new(),
    )
}
