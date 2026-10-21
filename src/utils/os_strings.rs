use std::ffi::OsString;

pub fn push_strs(args: &mut Vec<OsString>, items: &[&str]) {
    args.extend(items.iter().map(|s| OsString::from(*s)));
}

#[cfg(test)]
mod test {
    use std::ffi::OsString;

    use crate::utils::os_strings::push_strs;

    #[test]
    fn test_push_strs() {
        let mut args: Vec<OsString> = vec![OsString::from("arg1")];
        push_strs(&mut args, &["arg2", "arg3"]);

        assert_eq!(
            args,
            vec![
                OsString::from("arg1"),
                OsString::from("arg2"),
                OsString::from("arg3")
            ]
        )
    }
}
