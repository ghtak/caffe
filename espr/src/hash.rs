pub trait PasswordHasher: Sync + Send {
    fn hash(&self, password: &str) -> crate::Result<String>;
    fn verify(&self, password: &str, hash: &str) -> crate::Result<bool>;
}

pub mod argon2;
pub mod pbkdf2;
