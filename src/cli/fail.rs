use std::process::exit;

pub fn fail(message: impl AsRef<str>) -> ! {
    eprintln!("{}", message.as_ref());
    exit(1);
}

pub trait OrFail<T> {
    fn or_fail(self, context: &str) -> T;
}

impl<T, E: std::fmt::Display> OrFail<T> for Result<T, E> {
    fn or_fail(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(err) => fail(format!("{context}: {err}")),
        }
    }
}

impl<T> OrFail<T> for Option<T> {
    fn or_fail(self, context: &str) -> T {
        match self {
            Some(value) => value,
            None => fail(context),
        }
    }
}
