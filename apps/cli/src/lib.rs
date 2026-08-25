/// Process status contract.
pub mod exit {
    /// The command completed successfully.
    pub const SUCCESS: i32 = 0;
    /// The document failed validation.
    pub const DOCUMENT_INVALID: i32 = 1;
    /// The command line is invalid.
    pub const USAGE: i32 = 2;
    /// The input cannot be read or decoded.
    pub const INPUT: i32 = 3;
    /// The command cannot write its output.
    pub const OUTPUT: i32 = 4;
    /// Reserved until rendering lands.
    pub const RENDER: i32 = 5;
    /// Reserved for a future internal failure boundary.
    pub const INTERNAL: i32 = 70;
}

/// Adapter diagnostic code contract.
pub mod codes {
    /// The input path cannot be read or decoded as UTF-8.
    pub const INPUT001: &str = "INPUT001";
    /// The input exceeds the source size limit.
    pub const INPUT002: &str = "INPUT002";
    /// The command line contains invalid arguments.
    pub const USAGE001: &str = "USAGE001";
}
