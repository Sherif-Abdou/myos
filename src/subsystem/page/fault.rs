#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFaultError {
    Unhandled,
}

pub enum PageFaultType {
    // No valid translation
    Translation,
    // Access flag violation
    Access,
    // Permission violation
    Permission,
}

pub trait PageFaultHandler {
    fn page_fault(&self, fault: PageFaultType, vma: usize) -> Result<(), PageFaultError>;
}
