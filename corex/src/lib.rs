///```rust
/// assert!(true);
/// ```
pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub name: String,
    pub age: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
