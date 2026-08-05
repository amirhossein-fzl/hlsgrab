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

pub fn format_duration(duration: f64) -> String {
    let rounded = (duration * 1000.0).round() / 1000.0;
    if (rounded.fract()).abs() < f64::EPSILON {
        format!("{:.0}", rounded)
    } else {
        let mut s = format!("{:.3}", rounded);
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
        s
    }
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

    #[test]
    fn test_format_duration_whole_numbers() {
        // Whole numbers
        assert_eq!(format_duration(0.0), "0");
        assert_eq!(format_duration(1.0), "1");
        assert_eq!(format_duration(10.0), "10");
        assert_eq!(format_duration(100.0), "100");
        assert_eq!(format_duration(12345.0), "12345");

        // One decimal
        assert_eq!(format_duration(1.5), "1.5");
        assert_eq!(format_duration(1.1), "1.1");
        assert_eq!(format_duration(1.9), "1.9");
        assert_eq!(format_duration(10.5), "10.5");

        // Two decimal
        assert_eq!(format_duration(1.25), "1.25");
        assert_eq!(format_duration(1.20), "1.2");
        assert_eq!(format_duration(1.05), "1.05");
        assert_eq!(format_duration(10.75), "10.75");

        // Three decimal
        assert_eq!(format_duration(1.125), "1.125");
        assert_eq!(format_duration(1.120), "1.12");
        assert_eq!(format_duration(1.005), "1.005");
        assert_eq!(format_duration(1.001), "1.001");

        // Rounds to 3 decimal places
        assert_eq!(format_duration(1.0004), "1");
        assert_eq!(format_duration(1.0005), "1.001");
        assert_eq!(format_duration(1.9995), "2");
        assert_eq!(format_duration(1.9994), "1.999");

        // Small values
        assert_eq!(format_duration(0.001), "0.001");
        assert_eq!(format_duration(0.01), "0.01");
        assert_eq!(format_duration(0.1), "0.1");
        assert_eq!(format_duration(0.0005), "0.001");
        assert_eq!(format_duration(0.0004), "0");

        // Values that might have floating point representation issues
        assert_eq!(format_duration(1.0 / 3.0), "0.333");
        assert_eq!(format_duration(2.0 / 3.0), "0.667");
        assert_eq!(format_duration(1.0 / 6.0), "0.167");
        assert_eq!(format_duration(1.0 / 7.0), "0.143");
    }
}
