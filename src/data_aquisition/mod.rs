/*
 * This module provides data aquisition abilites for the application.
 * It doesn't care what it gets, just how.
 * This allows for adding support for new data sources and routing protocols.
 */

pub mod core;
pub mod snmp;
pub mod ssh;