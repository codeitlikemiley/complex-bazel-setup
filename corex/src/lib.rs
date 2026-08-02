/// Adds two numbers.
///
/// ```rust
/// assert_eq!(corex::add(2, 2), 4);
/// assert_eq!(corex::add(0, u64::MAX), u64::MAX);
/// ```
pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub name: String,
    pub age: u8,
}

impl User {
    /// Builds a [`User`] from its parts.
    ///
    /// ```rust
    /// let user = corex::User::new("ada".to_string(), 36);
    /// assert_eq!(user.name, "ada");
    /// assert_eq!(user.age, 36);
    /// ```
    pub fn new(name: String, age: u8) -> Self {
        Self { name, age }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_sums_its_arguments() {
        assert_eq!(add(2, 2), 4);
        assert_eq!(add(0, 0), 0);
    }

    #[test]
    fn user_new_keeps_its_fields() {
        let user = User::new("ada".to_string(), 36);
        assert_eq!(user.name, "ada");
        assert_eq!(user.age, 36);
    }

    #[test]
    fn user_keeps_its_serde_derives() {
        // Fails to compile if either derive is dropped from `User`.
        fn assert_serde<T: serde::Serialize + serde::de::DeserializeOwned>() {}
        assert_serde::<User>();
    }
}
