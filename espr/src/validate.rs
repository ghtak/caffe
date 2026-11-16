pub trait Validate {
    type Error;

    fn validate(&self) -> Result<(), Self::Error>;
}
