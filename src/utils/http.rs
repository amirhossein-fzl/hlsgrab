pub fn parse_header(line: &str) -> Option<(String, String)> {
    let (key, value) = line.split_once(':')?;
    Some((key.trim().to_string(), value.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() {
        assert_eq!(
            parse_header("MyKey: Myvalue"),
            Some(("MyKey".to_string(), "Myvalue".to_string()))
        );
    }

    #[test]
    fn value_contains_colon() {
        assert_eq!(
            parse_header("MyPKey: wow:key"),
            Some(("MyPKey".to_string(), "wow:key".to_string()))
        );
    }

    #[test]
    fn no_colon() {
        assert_eq!(parse_header("NotAHeader"), None);
    }

    #[test]
    fn no_space_after_colon() {
        assert_eq!(
            parse_header("Key:Value"),
            Some(("Key".to_string(), "Value".to_string()))
        );
    }
}
