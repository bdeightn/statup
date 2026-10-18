//! A named set of members, used to choose who sees a service.

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct MemberGroup {
    pub id: i64,
    pub name: String,
    /// Active members only; filled by the listing query.
    pub member_count: i64,
}
