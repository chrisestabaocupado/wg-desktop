use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Proxy,
    Vpn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Disconnected,
    Validating,
    Preparing,
    Connecting,
    Connected,
    Reconnecting,
    Disconnecting,
    Blocked,
    Error,
}

#[derive(Debug, Error)]
#[error("invalid connection state transition")]
pub struct StateError;

impl ConnectionState {
    pub fn transition(&mut self, next: Self) -> Result<(), StateError> {
        use ConnectionState::*;
        let allowed = matches!(
            (*self, next),
            (Disconnected | Error, Validating)
                | (Validating, Preparing | Error | Disconnecting)
                | (Preparing, Connecting | Error | Blocked | Disconnecting)
                | (Connecting, Connected | Error | Blocked | Disconnecting)
                | (Connected, Reconnecting | Disconnecting | Error | Blocked)
                | (Reconnecting, Connected | Error | Blocked | Disconnecting)
                | (Disconnecting, Disconnected | Error | Blocked)
                | (Blocked | Error, Disconnecting)
        );
        if !allowed {
            return Err(StateError);
        }
        *self = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cannot_skip_setup_or_silently_release_blocking() {
        let mut state = ConnectionState::Disconnected;
        assert!(state.transition(ConnectionState::Connected).is_err());
        assert_eq!(state, ConnectionState::Disconnected);
        state.transition(ConnectionState::Validating).unwrap();
        state.transition(ConnectionState::Preparing).unwrap();
        state.transition(ConnectionState::Blocked).unwrap();
        assert!(state.transition(ConnectionState::Disconnected).is_err());
        state.transition(ConnectionState::Disconnecting).unwrap();
        state.transition(ConnectionState::Disconnected).unwrap();
    }
}
