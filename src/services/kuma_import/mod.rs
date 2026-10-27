//! Importing the monitors of an Uptime Kuma status page as services, so the
//! reports its webhook sends find them by name.

mod import;
mod page;

pub use import::{
    FolderPlan, GroupPlan, Imported, LastImport, MonitorFate, MonitorPlan, import_groups,
    plan_import,
};
pub use page::{KumaError, KumaGroup, KumaMonitor, KumaPage, read_status_page};
