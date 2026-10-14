//! Events: incidents, maintenances and announcements.
//!
//! Six orthogonal dimensions, checked by SQL constraints:
//! - `kind`: incident, maintenance or publication (an announcement).
//! - `severity`: impact on services, absent for publications.
//! - `planned`: a maintenance announced ahead, which starts and ends on its
//!   own at the planned times.
//! - `lifecycle`: workflow state, its values depend on the kind, absent for
//!   publications.
//! - `category`: kind of announcement, publications only.
//! - `keeps_services_up`: a maintenance that leaves its services in their
//!   state while it runs.
//!
//! An open incident may also say when its next update is due, and a
//! resolved one carry its review.

mod dimensions;
mod input;
mod next_update;
mod record;
mod review;
mod service_tag;
#[cfg(test)]
mod tests;

pub use dimensions::*;
pub use input::*;
pub use next_update::*;
pub use record::*;
pub use review::*;
pub use service_tag::*;
