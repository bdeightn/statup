//! Group repository: named sets of members, used to choose who sees a service.

use super::service_repo::CHOSEN_ORDER;
use crate::db::DbPool;
use crate::models::MemberGroup;

pub struct GroupRepository;

impl GroupRepository {
    pub async fn create(pool: &DbPool, name: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("INSERT INTO member_groups (name) VALUES (?) RETURNING id")
            .bind(name)
            .fetch_one(pool)
            .await
    }

    /// Whether another group already has this name, ignoring case, accents
    /// included: SQLite folds the case of ASCII letters only, so "Équipe"
    /// and "équipe" are compared here. `except` is the group being renamed.
    pub async fn name_taken(
        pool: &DbPool,
        name: &str,
        except: Option<i64>,
    ) -> Result<bool, sqlx::Error> {
        let groups: Vec<(i64, String)> = sqlx::query_as("SELECT id, name FROM member_groups")
            .fetch_all(pool)
            .await?;
        let wanted = name.to_lowercase();
        Ok(groups
            .iter()
            .any(|(id, taken)| Some(*id) != except && taken.to_lowercase() == wanted))
    }

    /// Returns false when no such group exists.
    pub async fn rename(pool: &DbPool, id: i64, name: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("UPDATE member_groups SET name = ? WHERE id = ?")
            .bind(name)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Returns false when no such group exists. Memberships and service
    /// access to the group go with it.
    pub async fn delete(pool: &DbPool, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM member_groups WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Every group by name, with its count of active members.
    pub async fn list(pool: &DbPool) -> Result<Vec<MemberGroup>, sqlx::Error> {
        sqlx::query_as::<_, MemberGroup>(
            "SELECT g.id, g.name, \
                    (SELECT COUNT(*) FROM group_members gm \
                     JOIN users u ON u.id = gm.user_id \
                     WHERE gm.group_id = g.id AND u.is_active = 1) AS member_count \
             FROM member_groups g ORDER BY g.name COLLATE NOCASE",
        )
        .fetch_all(pool)
        .await
    }

    /// Every membership as (member, group), for a page that lists them all.
    pub async fn memberships(pool: &DbPool) -> Result<Vec<(i64, i64)>, sqlx::Error> {
        sqlx::query_as("SELECT user_id, group_id FROM group_members")
            .fetch_all(pool)
            .await
    }

    pub async fn ids_of_member(pool: &DbPool, user_id: i64) -> Result<Vec<i64>, sqlx::Error> {
        sqlx::query_scalar("SELECT group_id FROM group_members WHERE user_id = ? ORDER BY group_id")
            .bind(user_id)
            .fetch_all(pool)
            .await
    }

    /// Replaces the groups of a member. Ids of groups that do not exist are
    /// dropped.
    pub async fn set_member_groups(
        pool: &DbPool,
        user_id: i64,
        group_ids: &[i64],
    ) -> Result<(), sqlx::Error> {
        let ids = serde_json::to_string(group_ids).unwrap_or_default();
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM group_members WHERE user_id = ?")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO group_members (group_id, user_id) \
             SELECT id, ? FROM member_groups WHERE id IN (SELECT value FROM json_each(?))",
        )
        .bind(user_id)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
        tx.commit().await
    }

    /// Names of the restricted services whose only group is this one: once
    /// it is deleted, only publishers and admins see them.
    pub async fn services_closed_by_deleting(
        pool: &DbPool,
        group_id: i64,
    ) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar(&format!(
            "SELECT s.name FROM services s \
             WHERE s.visibility = 'groups' \
             AND EXISTS (SELECT 1 FROM service_access a \
                         WHERE a.service_id = s.id AND a.group_id = ?) \
             AND (SELECT COUNT(*) FROM service_access a WHERE a.service_id = s.id) = 1 \
             ORDER BY {CHOSEN_ORDER}"
        ))
        .bind(group_id)
        .fetch_all(pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Role, Visibility};
    use crate::repositories::{ServiceRepository, UserRepository};
    use crate::test_helpers::test_pool;

    async fn member(pool: &DbPool, email: &str) -> i64 {
        UserRepository::create(pool, email, "hash", "Someone", Role::Reader)
            .await
            .unwrap()
            .id
    }

    async fn service(pool: &DbPool, name: &str) -> i64 {
        ServiceRepository::create(pool, name, &name.to_lowercase(), None, None, None)
            .await
            .unwrap()
            .id
    }

    async fn count(pool: &DbPool, table: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_group_is_created_renamed_and_deleted() {
        let pool = test_pool().await;
        let id = GroupRepository::create(&pool, "Support").await.unwrap();
        assert!(
            GroupRepository::rename(&pool, id, "Helpdesk")
                .await
                .unwrap()
        );
        assert_eq!(
            GroupRepository::list(&pool).await.unwrap()[0].name,
            "Helpdesk"
        );
        assert!(
            !GroupRepository::rename(&pool, id + 1, "Nope")
                .await
                .unwrap()
        );

        assert!(GroupRepository::delete(&pool, id).await.unwrap());
        assert!(!GroupRepository::delete(&pool, id).await.unwrap());
        assert_eq!(GroupRepository::list(&pool).await.unwrap(), Vec::new());
    }

    #[tokio::test]
    async fn the_schema_refuses_two_groups_with_the_same_name_ignoring_case() {
        let pool = test_pool().await;
        GroupRepository::create(&pool, "Support").await.unwrap();
        assert!(GroupRepository::create(&pool, "SUPPORT").await.is_err());
    }

    #[tokio::test]
    async fn a_name_is_taken_ignoring_case_except_by_the_group_renamed() {
        let pool = test_pool().await;
        let id = GroupRepository::create(&pool, "Support").await.unwrap();
        assert!(
            GroupRepository::name_taken(&pool, "support", None)
                .await
                .unwrap()
        );
        assert!(
            !GroupRepository::name_taken(&pool, "support", Some(id))
                .await
                .unwrap()
        );
        assert!(
            GroupRepository::name_taken(&pool, "support", Some(id + 1))
                .await
                .unwrap()
        );
        assert!(
            !GroupRepository::name_taken(&pool, "Sales", None)
                .await
                .unwrap()
        );
        GroupRepository::create(&pool, "Équipe").await.unwrap();
        assert!(
            GroupRepository::name_taken(&pool, "équipe", None)
                .await
                .unwrap(),
            "an accented capital is the same letter"
        );
    }

    #[tokio::test]
    async fn deleting_a_group_removes_its_memberships_and_service_access() {
        let pool = test_pool().await;
        let group = GroupRepository::create(&pool, "Support").await.unwrap();
        let user = member(&pool, "a@b.com").await;
        let svc = service(&pool, "Wiki").await;
        GroupRepository::set_member_groups(&pool, user, &[group])
            .await
            .unwrap();
        ServiceRepository::set_visibility(&pool, svc, Visibility::Groups, &[group])
            .await
            .unwrap();

        GroupRepository::delete(&pool, group).await.unwrap();
        assert_eq!(count(&pool, "group_members").await, 0);
        assert_eq!(count(&pool, "service_access").await, 0);
    }

    #[tokio::test]
    async fn the_listing_counts_active_members_only() {
        let pool = test_pool().await;
        let group = GroupRepository::create(&pool, "support").await.unwrap();
        GroupRepository::create(&pool, "Admins").await.unwrap();
        let active = member(&pool, "a@b.com").await;
        let gone = member(&pool, "c@d.com").await;
        for user in [active, gone] {
            GroupRepository::set_member_groups(&pool, user, &[group])
                .await
                .unwrap();
        }
        UserRepository::set_active(&pool, gone, false)
            .await
            .unwrap();

        let groups = GroupRepository::list(&pool).await.unwrap();
        let summary: Vec<(&str, i64)> = groups
            .iter()
            .map(|g| (g.name.as_str(), g.member_count))
            .collect();
        assert_eq!(summary, vec![("Admins", 0), ("support", 1)]);
    }

    #[tokio::test]
    async fn setting_the_groups_of_a_member_replaces_them_and_drops_unknown_ids() {
        let pool = test_pool().await;
        let first = GroupRepository::create(&pool, "First").await.unwrap();
        let second = GroupRepository::create(&pool, "Second").await.unwrap();
        let user = member(&pool, "a@b.com").await;

        GroupRepository::set_member_groups(&pool, user, &[first])
            .await
            .unwrap();
        GroupRepository::set_member_groups(&pool, user, &[second, 999])
            .await
            .unwrap();
        assert_eq!(
            GroupRepository::ids_of_member(&pool, user).await.unwrap(),
            vec![second]
        );

        GroupRepository::set_member_groups(&pool, user, &[])
            .await
            .unwrap();
        assert_eq!(
            GroupRepository::ids_of_member(&pool, user).await.unwrap(),
            Vec::<i64>::new()
        );
    }

    #[tokio::test]
    async fn deleting_a_group_closes_only_the_services_it_alone_opens() {
        let pool = test_pool().await;
        let support = GroupRepository::create(&pool, "Support").await.unwrap();
        let sales = GroupRepository::create(&pool, "Sales").await.unwrap();
        let only = service(&pool, "Only").await;
        let shared = service(&pool, "Shared").await;
        let members = service(&pool, "Members").await;
        let elsewhere = service(&pool, "Elsewhere").await;
        let groups = Visibility::Groups;
        ServiceRepository::set_visibility(&pool, only, groups, &[support])
            .await
            .unwrap();
        ServiceRepository::set_visibility(&pool, shared, groups, &[support, sales])
            .await
            .unwrap();
        ServiceRepository::set_visibility(&pool, members, Visibility::Members, &[support])
            .await
            .unwrap();
        ServiceRepository::set_visibility(&pool, elsewhere, groups, &[sales])
            .await
            .unwrap();

        let closed = GroupRepository::services_closed_by_deleting(&pool, support)
            .await
            .unwrap();
        assert_eq!(closed, vec!["Only"]);
    }
}
