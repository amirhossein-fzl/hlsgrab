pub fn count_digits(n: i64) -> u32 {
    let mut num = n;

    if num == 0 {
        return 1;
    }

    if num < 0 {
        num = -num;
    }

    let mut count = 0;

    while num > 0 {
        count += 1;
        num /= 10;
    }

    count
}

pub fn pad_with_zeros(a: usize, b: u64) -> String {
    format!("{:0>width$}", b, width = a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_digits() {
        assert_eq!(count_digits(0), 1);
        assert_eq!(count_digits(123), 3);
        assert_eq!(count_digits(-4567), 4);
        assert_eq!(count_digits(1000000000000000000), 19);
    }

    #[test]
    fn test_pad_with_zeros() {
        assert_eq!(pad_with_zeros(3, 10), "010");
        assert_eq!(pad_with_zeros(4, 8), "0008");
        assert_eq!(pad_with_zeros(6, 8542), "008542");
        assert_eq!(pad_with_zeros(5, 12345), "12345");
    }
}
