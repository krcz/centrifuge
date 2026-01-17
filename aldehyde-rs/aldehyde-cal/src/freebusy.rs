use polyepoxide_core::oxide;

/// Type of busy period
#[oxide]
pub enum BusyType {
    Busy,
    BusyUnavailable,
    BusyTentative,
    Free,
}

/// A single busy period
#[oxide]
pub struct BusyPeriod {
    pub start: i64,
    pub end: i64,
    pub busy_type: BusyType,
}

/// Free/busy information (VFREEBUSY)
#[oxide]
pub struct FreeBusy {
    pub start: i64,
    pub end: i64,
    pub periods: Vec<BusyPeriod>,
}
