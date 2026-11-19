pub mod permission_create;
pub mod permission_delete;
pub mod permission_list;
pub mod role_create;
pub mod role_delete;
pub mod role_list;
pub mod role_permission_add;
pub mod role_permission_remove;
pub mod user_role_add;
pub mod user_role_list;
pub mod user_role_remove;

#[cfg(test)]
mod permission_write_tests;
#[cfg(test)]
mod role_tests;
#[cfg(test)]
mod user_role_tests;
