//! The game-neutral core of TCG Binder: cards and the copies owned, the collection
//! file, price history, filtering and statistics, the user's settings, and the
//! services every front end (desktop, phone, web) shares.
//!
//! Each trading card game plugs in through [`game::Game`]; Magic lives in the
//! `tcgbinder-mtg` crate.

pub mod collection;
pub mod datadir;
pub mod datafolder;
pub mod dates;
pub mod game;
pub mod instance;
pub mod jsonfile;
pub mod lists;
pub mod logging;
pub mod lots;
pub mod records;
pub mod sales;
pub mod scan;
pub mod sealed;
pub mod serde_skip;
pub mod service;
pub mod settings;
pub mod version;
pub mod watchlist;

pub use collection::{Card, Condition, Copies, Purchase};
