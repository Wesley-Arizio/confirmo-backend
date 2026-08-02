use juniper::GraphQLInputObject;

#[derive(Debug, GraphQLInputObject)]
pub struct CreateLawyerInput {
    pub name: String,
    pub email: String,
    pub password: String,
    pub oab_number: String,
    pub firm_name: String,
}

#[derive(Debug, GraphQLInputObject)]
pub struct VerifyEmailInput {
    pub email: String,
    pub code: String,
}
