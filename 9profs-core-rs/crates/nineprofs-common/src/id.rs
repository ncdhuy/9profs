/// Generate an opaque identifier for runtime and event records.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn generated_ids_are_non_empty_and_distinct() {
        let first = super::new_id();
        let second = super::new_id();

        assert!(!first.is_empty());
        assert_ne!(first, second);
    }
}
