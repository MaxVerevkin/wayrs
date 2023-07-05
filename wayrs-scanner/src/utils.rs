pub fn snake_to_pascal(s: &str) -> String {
    let mut retval = String::new();

    for element in s.split('_') {
        let mut chars = element.chars();
        if let Some(c) = chars.next() {
            retval.push(c.to_ascii_uppercase());
            retval.push_str(chars.as_str());
        }
    }

    retval
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snake_to_pascal() {
        assert_eq!(snake_to_pascal("wl_display"), "WlDisplay");
        assert_eq!(snake_to_pascal("180"), "180");
        assert_eq!(snake_to_pascal("single"), "Single");
        assert_eq!(snake_to_pascal(""), "");
    }
}
