#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoTarget {
    Copy,
    H264,
    H265,
    Vp9,
    Av1,
}
