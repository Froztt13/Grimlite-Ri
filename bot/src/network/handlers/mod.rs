//! Packet-level combat & boss mechanic handlers driven by Ruffle's `CaptureProxy`.

pub mod anti_counter;
pub mod anti_mod;
pub mod astral_empyrean;
pub mod colossal_vordred;
pub mod dark_carnax;
pub mod gramiel;
pub mod maid;
pub mod queen_iona;
pub mod special;
pub mod ultra_dage;
pub mod ultra_speaker;

pub use anti_counter::AntiCounterHandler;
pub use anti_mod::{AntiModHandler, StaffAlert};
pub use astral_empyrean::AstralEmpyreanHandler;
pub use colossal_vordred::ColossalVordredHandler;
pub use dark_carnax::DarkCarnaxHandler;
pub use gramiel::GramielHandler;
pub use maid::MaidPacketRouter;
pub use queen_iona::QueenIonaHandler;
pub use special::{SpecialHandlerManager, SpecialZone, static_zone_coords};
pub use ultra_dage::UltraDageHandler;
pub use ultra_speaker::{SpeakerStep, UltraSpeakerHandler};
